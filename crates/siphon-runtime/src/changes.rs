//! The changes of one commit, owned: built up by whoever commits — a run, a handler answering, the node — and lent to
//! Beak as its borrowed changes when the commit is made.

use beak::Signal;
use bytes::Bytes;

use crate::program::Program;

/// A name and its value, or none to remove it.
pub type Named = (Bytes, Option<Bytes>);

/// The changes one commit makes, in order.
#[derive(Debug, Clone, Default)]
pub struct Changes(Vec<Owned>);

#[derive(Debug, Clone)]
enum Owned {
    Mkdir(Bytes),
    Write(Bytes, Bytes),
    Append(Bytes, Bytes),
    Seal(Bytes),
    Rename(Bytes, Bytes),
    Delete(Bytes),
    Expire(Bytes, Option<i64>),
    Meta(Bytes, Vec<Named>, Vec<Named>),
    Start(Start),
    Request(Bytes, Mail, Option<u64>),
    Send(Bytes, Mail, Option<u64>),
    Answer(Bytes, Mail, Option<u64>),
    Signal(Bytes, Signal, Option<Bytes>),
    Reap(Bytes),
    Remove(Bytes, Option<Bytes>),
    ForceRemove(Bytes, Option<Bytes>),
}

#[derive(Debug, Clone)]
struct Start {
    parent: Bytes,
    key: Bytes,
    kind: Bytes,
    version: u32,
    input: Bytes,
}

/// Mail, or a request: its key, its name, and its payload.
#[derive(Debug, Clone)]
pub struct Mail {
    pub key: Bytes,
    pub name: Bytes,
    pub payload: Bytes,
}

impl Mail {
    fn lent(&self) -> beak::Mail<'_> {
        beak::Mail {
            key: &self.key,
            name: &self.name,
            payload: &self.payload,
        }
    }
}

fn lent(named: &[Named]) -> Vec<(&[u8], Option<&[u8]>)> {
    named.iter().map(|(name, value)| (&name[..], value.as_deref())).collect()
}

impl Changes {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The directory at `path` made, its parent there; one there left as it is.
    pub fn mkdir(&mut self, path: impl Into<Bytes>) -> &mut Self {
        self.push(Owned::Mkdir(path.into()))
    }

    /// The file at `path` given `bytes` whole.
    pub fn write(&mut self, path: impl Into<Bytes>, bytes: impl Into<Bytes>) -> &mut Self {
        self.push(Owned::Write(path.into(), bytes.into()))
    }

    /// `bytes` added at the end of the file at `path`, made where there is none.
    pub fn append(&mut self, path: impl Into<Bytes>, bytes: impl Into<Bytes>) -> &mut Self {
        self.push(Owned::Append(path.into(), bytes.into()))
    }

    /// The file at `path` sealed: it takes no change from then on.
    pub fn seal(&mut self, path: impl Into<Bytes>) -> &mut Self {
        self.push(Owned::Seal(path.into()))
    }

    /// What `from` names given the name `to`.
    pub fn rename(&mut self, from: impl Into<Bytes>, to: impl Into<Bytes>) -> &mut Self {
        self.push(Owned::Rename(from.into(), to.into()))
    }

    /// The name at `path` deleted, with everything under it.
    pub fn delete(&mut self, path: impl Into<Bytes>) -> &mut Self {
        self.push(Owned::Delete(path.into()))
    }

    /// The name at `path` gone at `at`; never, where it has none.
    pub fn expire(&mut self, path: impl Into<Bytes>, at: Option<i64>) -> &mut Self {
        self.push(Owned::Expire(path.into(), at))
    }

    /// The labels and annotations of the directory at `path` set, each to its value, or removed where it has none.
    pub fn meta(&mut self, path: impl Into<Bytes>, labels: Vec<Named>, annotations: Vec<Named>) -> &mut Self {
        self.push(Owned::Meta(path.into(), labels, annotations))
    }

    /// A process of `P` started under `parent` — a directory, a live process, or the root, empty — keyed by `key`,
    /// from `input`.
    ///
    /// # Errors
    /// An input that cannot be written as JSON.
    pub fn start<P: Program>(&mut self, parent: impl Into<Bytes>, key: &str, input: &P::Input) -> serde_json::Result<&mut Self> {
        let start = Start {
            parent: parent.into(),
            key: Bytes::copy_from_slice(key.as_bytes()),
            kind: Bytes::from_static(P::KIND.as_bytes()),
            version: P::VERSION,
            input: serde_json::to_vec(input)?.into(),
        };
        Ok(self.push(Owned::Start(start)))
    }

    /// A request the process at `process`, started by the commit `started`, makes of the outside.
    pub(crate) fn request(&mut self, process: Bytes, mail: Mail, started: Option<u64>) -> &mut Self {
        self.push(Owned::Request(process, mail, started))
    }

    /// Mail sent to the process at `process`; where it was started by the commit `started` alone, given.
    pub fn send(&mut self, process: impl Into<Bytes>, mail: Mail, started: Option<u64>) -> &mut Self {
        self.push(Owned::Send(process.into(), mail, started))
    }

    /// The request of the mail's key the process at `process` made, answered by it.
    pub(crate) fn answer(&mut self, process: Bytes, mail: Mail, started: Option<u64>) -> &mut Self {
        self.push(Owned::Answer(process, mail, started))
    }

    /// The process at `process`, and every one under it, sent `signal` for `reason`.
    pub fn signal(&mut self, process: impl Into<Bytes>, signal: Signal, reason: Option<Bytes>) -> &mut Self {
        self.push(Owned::Signal(process.into(), signal, reason))
    }

    /// The record of the ended process at `process` let go of, as a parent reaps its child.
    pub fn reap(&mut self, process: impl Into<Bytes>) -> &mut Self {
        self.push(Owned::Reap(process.into()))
    }

    /// The name at `path` removed, the processes under it asked to stop first.
    pub fn remove(&mut self, path: impl Into<Bytes>, reason: Option<Bytes>) -> &mut Self {
        self.push(Owned::Remove(path.into(), reason))
    }

    /// The name at `path` removed, the processes under it killed first.
    pub fn force_remove(&mut self, path: impl Into<Bytes>, reason: Option<Bytes>) -> &mut Self {
        self.push(Owned::ForceRemove(path.into(), reason))
    }

    fn push(&mut self, change: Owned) -> &mut Self {
        self.0.push(change);
        self
    }

    /// Calls `call` with the changes as Beak takes them, lent from these.
    pub(crate) fn lent<T>(&self, call: impl FnOnce(&[beak::Change<'_>]) -> T) -> T {
        let named: Vec<_> = self
            .0
            .iter()
            .map(|change| match change {
                Owned::Meta(_, labels, annotations) => (lent(labels), lent(annotations)),
                _ => (Vec::new(), Vec::new()),
            })
            .collect();
        let changes: Vec<beak::Change<'_>> = self
            .0
            .iter()
            .zip(&named)
            .map(|(change, (labels, annotations))| lend(change, labels, annotations))
            .collect();
        call(&changes)
    }
}

fn lend<'a>(
    change: &'a Owned,
    labels: &'a [(&'a [u8], Option<&'a [u8]>)],
    annotations: &'a [(&'a [u8], Option<&'a [u8]>)],
) -> beak::Change<'a> {
    use beak::Change;
    match change {
        Owned::Mkdir(path) => Change::Mkdir { path },
        Owned::Write(path, bytes) => Change::Write { path, bytes },
        Owned::Append(path, bytes) => Change::Append {
            path,
            bytes,
            append_only: false,
        },
        Owned::Seal(path) => Change::Seal { path },
        Owned::Rename(from, to) => Change::Rename { from, to },
        Owned::Delete(path) => Change::Delete { path },
        Owned::Expire(path, at) => Change::Expire { path, at: *at },
        Owned::Meta(path, _, _) => Change::Meta { path, labels, annotations },
        Owned::Start(start) => Change::Start {
            parent: &start.parent,
            key: &start.key,
            kind: &start.kind,
            version: start.version,
            input: &start.input,
            checkpoint: b"",
            priority: 0,
            namespace: None,
        },
        Owned::Request(process, mail, started) => Change::Request {
            process,
            mail: mail.lent(),
            started: *started,
        },
        Owned::Send(process, mail, started) => Change::Send {
            process,
            mail: mail.lent(),
            started: *started,
        },
        Owned::Answer(process, mail, started) => Change::Answer {
            process,
            mail: mail.lent(),
            started: *started,
        },
        Owned::Signal(process, signal, reason) => Change::Signal {
            process,
            signal: *signal,
            reason: reason.as_deref(),
            started: None,
        },
        Owned::Reap(process) => Change::Reap { process },
        Owned::Remove(path, reason) => Change::Remove {
            path,
            reason: reason.as_deref(),
        },
        Owned::ForceRemove(path, reason) => Change::ForceRemove {
            path,
            reason: reason.as_deref(),
        },
    }
}
