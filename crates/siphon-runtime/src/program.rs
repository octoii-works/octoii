//! A program: the code Beak's processes of one kind run, a phase at a time. Each run reads what the last commit left,
//! decides once, and gives back the changes of one commit and how it ends — runnable again, asleep on a wait, done,
//! failed — which Beak commits together; to go on from what it committed, a run ends `Next`. Its input, its checkpoint
//! and its result are JSON, each the program's own type.

use std::panic::AssertUnwindSafe;

use beak::{Kernel, Selector, Snapshot, layout};
use bytes::Bytes;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::changes::{Changes, Mail};
use crate::request::{Answer, Request, answer_of};

/// How a run that threw runs again before its process fails: `count` times, waiting twice as long each time from the
/// least delay to the most.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retries {
    pub count: u32,
    pub delay_min_ms: i64,
    pub delay_max_ms: i64,
}

impl Retries {
    pub const NONE: Self = Self {
        count: 0,
        delay_min_ms: 0,
        delay_max_ms: 0,
    };
}

/// The code the processes of one kind run.
pub trait Program: Send + Sync + 'static {
    /// The processes' kind: `siphon.` and what they are, as `siphon.turn`.
    const KIND: &'static str;
    const VERSION: u32 = 1;
    /// Whether a process asked to stop runs once more, a stop handler, before it ends.
    const STOP_HANDLER: bool = false;
    const RETRIES: Retries = Retries::NONE;
    type Input: Serialize + DeserializeOwned;
    type Checkpoint: Serialize + DeserializeOwned;
    type Output: Serialize + DeserializeOwned;

    /// Runs a phase of the process `run` is for, from its input and the checkpoint the last phase ended with — none
    /// for its first. An error throws: nothing of the run is committed, and the phase runs again while retries last.
    ///
    /// # Errors
    /// What stopped the phase.
    fn run(
        &self,
        run: &mut Run<'_>,
        input: Self::Input,
        checkpoint: Option<Self::Checkpoint>,
    ) -> anyhow::Result<Ending<Self::Checkpoint, Self::Output>>;
}

/// How a phase ends.
#[derive(Debug, Clone)]
pub enum Ending<C, O> {
    /// Runnable again, its next phase from the checkpoint, as a process yields.
    Next(C),
    /// Asleep until the first of what the wait names is met, then runnable from the checkpoint.
    Wait(C, Wait),
    Done(O),
    Fail(String),
    /// The stop handler's end: the process ends stopped, with its result where it has one.
    Stopped(Option<O>),
}

/// What a process waits for, the first met ending the wait, as `poll` waits on descriptors and a timeout at once.
#[derive(Debug, Clone, Default)]
pub struct Wait {
    children: Vec<Bytes>,
    changes: Vec<Bytes>,
    mail: Vec<Bytes>,
    selected: Option<Selector>,
    deadline: Option<i64>,
}

impl Wait {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Mail of `key` in the process's mailbox — at once, where it is there already.
    #[must_use]
    pub fn mail(mut self, key: impl Into<Bytes>) -> Self {
        self.mail.push(key.into());
        self
    }

    /// The child keyed `key`, every child named ending.
    #[must_use]
    pub fn child(mut self, key: impl Into<Bytes>) -> Self {
        self.children.push(key.into());
        self
    }

    /// The directory at `path` changing from what the last commit left.
    #[must_use]
    pub fn change(mut self, path: impl Into<Bytes>) -> Self {
        self.changes.push(path.into());
        self
    }

    /// A directory `selector` selects written, already there or made.
    #[must_use]
    pub fn selected(mut self, selector: Selector) -> Self {
        self.selected = Some(selector);
        self
    }

    /// The time `at`, at the latest.
    #[must_use]
    pub fn deadline(mut self, at: i64) -> Self {
        self.deadline = Some(at);
        self
    }
}

/// A run in hand: the process it is for, the volume as the last commit left it, and the changes it makes.
#[derive(Debug)]
pub struct Run<'a> {
    /// The run as Beak gave it.
    pulled: &'a beak::Run,
    snapshot: &'a Snapshot,
    kernel: &'a Kernel,
    time_ms: i64,
    pub changes: Changes,
}

impl Run<'_> {
    /// The process's path.
    #[must_use]
    pub fn path(&self) -> &[u8] {
        &self.pulled.path
    }

    /// The time of the dispatch that runs the phase.
    #[must_use]
    pub fn time_ms(&self) -> i64 {
        self.time_ms
    }

    /// Whether this is the process's stop handler, run once it was asked to stop: its ending is the process's.
    #[must_use]
    pub fn is_stop_handler(&self) -> bool {
        self.pulled.stop_handler
    }

    /// The volume as the last commit left it.
    #[must_use]
    pub fn snapshot(&self) -> &Snapshot {
        self.snapshot
    }

    /// A new id, made at the run's time.
    ///
    /// # Errors
    /// Past the last id there is.
    pub fn new_id(&self) -> beak::Result<String> {
        self.kernel.new_id(self.time_ms)
    }

    /// Asks the outside `request`, keyed by `key`, with the run's commit: its answer comes as mail of the same key.
    ///
    /// # Errors
    /// A payload that cannot be written as JSON.
    pub fn request<R: Request>(&mut self, key: &str, request: &R) -> serde_json::Result<()> {
        let mail = Mail {
            key: Bytes::copy_from_slice(key.as_bytes()),
            name: Bytes::from_static(R::NAME.as_bytes()),
            payload: serde_json::to_vec(request)?.into(),
        };
        self.changes.request(self.pulled.path.clone(), mail, Some(self.pulled.started));
        Ok(())
    }

    /// The answer to the request of `key`, where its mail has come.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn answer<R: Request>(&self, key: &str) -> beak::Result<Option<Answer<R::Answer>>> {
        let path = layout::mail(&self.pulled.path, key.as_bytes());
        let Some(entry) = self.snapshot.stat(&path)? else { return Ok(None) };
        let name = entry
            .directory
            .as_ref()
            .and_then(|directory| directory.label(layout::MAIL_LABEL))
            .cloned()
            .unwrap_or_default();
        let payload = self.snapshot.read(&layout::payload(&path), 0, u64::MAX)?.unwrap_or_default();
        Ok(Some(answer_of(&name, &payload)))
    }

    /// The mail of `key` taken out of the mailbox, with the run's commit.
    pub fn take_mail(&mut self, key: &str) {
        self.changes.delete(layout::mail(&self.pulled.path, key.as_bytes()));
    }

    /// A process of `P` started under this one, keyed by `key`, from `input`, with the run's commit.
    ///
    /// # Errors
    /// An input that cannot be written as JSON.
    pub fn start<P: Program>(&mut self, key: &str, input: &P::Input) -> serde_json::Result<()> {
        self.changes.start::<P>(self.pulled.path.clone(), key, input)?;
        Ok(())
    }
}

/// A program of any kind, as the dispatcher runs it: from the bytes Beak keeps, to the ending as bytes.
pub(crate) trait Runs: Send + Sync {
    fn runs(&self, run: &mut Run<'_>, input: &[u8], checkpoint: &[u8]) -> anyhow::Result<Ended>;
}

/// An ending, as bytes.
#[derive(Debug)]
pub(crate) enum Ended {
    Next(Vec<u8>),
    Wait(Vec<u8>, Wait),
    Done(Vec<u8>),
    Fail(String),
    Stopped(Option<Vec<u8>>),
}

impl<P: Program> Runs for P {
    fn runs(&self, run: &mut Run<'_>, input: &[u8], checkpoint: &[u8]) -> anyhow::Result<Ended> {
        let input = serde_json::from_slice(input)?;
        let checkpoint = if checkpoint.is_empty() {
            None
        } else {
            Some(serde_json::from_slice(checkpoint)?)
        };
        Ok(match self.run(run, input, checkpoint)? {
            Ending::Next(checkpoint) => Ended::Next(serde_json::to_vec(&checkpoint)?),
            Ending::Wait(checkpoint, wait) => Ended::Wait(serde_json::to_vec(&checkpoint)?, wait),
            Ending::Done(output) => Ended::Done(serde_json::to_vec(&output)?),
            Ending::Fail(reason) => Ended::Fail(reason),
            Ending::Stopped(output) => Ended::Stopped(output.map(|output| serde_json::to_vec(&output)).transpose()?),
        })
    }
}

/// Runs `run` with `program` and finishes it at `time_ms`: its changes and its ending in one commit, or, where the
/// program threw or panicked, its message, nothing of it committed.
pub(crate) fn dispatch(kernel: &Kernel, snapshot: &Snapshot, program: &dyn Runs, run: &beak::Run, time_ms: i64) {
    let mut context = Run {
        pulled: run,
        snapshot,
        kernel,
        time_ms,
        changes: Changes::new(),
    };
    let ran = std::panic::catch_unwind(AssertUnwindSafe(|| program.runs(&mut context, &run.input, &run.checkpoint)));
    let ended = match ran {
        Ok(Ok(ended)) => Ok(ended),
        Ok(Err(error)) => Err(format!("{error:#}")),
        Err(panic) => Err(format!("the phase panicked: {}", panic_message(&*panic))),
    };
    let finished = match &ended {
        Ok(ended) => finish(kernel, snapshot, run, time_ms, ended, &context.changes),
        Err(message) => kernel.finish(time_ms, run, &beak::Ending::Threw(message.as_bytes()), &[]),
    };
    if let Err(error) = finished {
        tracing::warn!(process = %String::from_utf8_lossy(&run.path), %error, "a run was not finished");
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> &str {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("no message")
}

fn finish(kernel: &Kernel, snapshot: &Snapshot, run: &beak::Run, time_ms: i64, ended: &Ended, changes: &Changes) -> beak::Result<u64> {
    let versions = match ended {
        Ended::Wait(_, wait) => versions(snapshot, &run.path, wait)?,
        _ => Vec::new(),
    };
    let versions: Vec<(&[u8], Option<u64>)> = versions.iter().map(|(path, seq)| (&path[..], *seq)).collect();
    let children: Vec<&[u8]> = match ended {
        Ended::Wait(_, wait) => wait.children.iter().map(|key| &key[..]).collect(),
        _ => Vec::new(),
    };
    let ending = match ended {
        Ended::Next(checkpoint) => beak::Ending::Next(checkpoint),
        Ended::Wait(checkpoint, wait) => beak::Ending::Wait {
            checkpoint,
            wait: beak::Wait {
                children: &children,
                changes: &versions,
                selected: wait.selected.as_ref(),
                deadline: wait.deadline,
            },
        },
        Ended::Done(output) => beak::Ending::Done(output),
        Ended::Fail(reason) => beak::Ending::Fail(reason.as_bytes()),
        Ended::Stopped(output) => beak::Ending::Stopped(output.as_deref()),
    };
    changes.lent(|lent| kernel.finish(time_ms, run, &ending, lent))
}

/// Each directory a wait watches, with the commit that last changed it — none for one not there — the directory of
/// each mail it waits for among them, waited on as not there, so its coming ends the wait.
fn versions(snapshot: &Snapshot, process: &[u8], wait: &Wait) -> beak::Result<Vec<(Bytes, Option<u64>)>> {
    let mut versions = Vec::with_capacity(wait.changes.len() + wait.mail.len());
    for path in &wait.changes {
        let seq = snapshot
            .stat(path)?
            .and_then(|entry| entry.directory)
            .map(|directory| directory.changed_seq);
        versions.push((path.clone(), seq));
    }
    versions.extend(wait.mail.iter().map(|key| (layout::mail(process, key), None)));
    Ok(versions)
}
