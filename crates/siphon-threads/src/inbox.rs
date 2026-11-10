//! A thread's inbox: what is delivered to it and not yet taken into its log — an input, a change of how it runs —
//! each a file `inbox/<id>.json`, its id made as it came, so the names sort in the order they came. The thread's main
//! process takes them in that order, as a mail queue is drained, each into its log where it then stands, a change of
//! how it runs set on its directory in the same commit.

use beak::{Range, Snapshot};
use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use siphon_runtime::{Changes, Run};

use crate::config::{Config, ConfigChange};
use crate::log::{Address, Entry, Error, Writer};
use crate::thread;

/// What is delivered to a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Inbound {
    /// An input: whose it is — the person's, or a thread's — and what it says.
    Input { actor: String, content: Vec<InputContent> },
    /// A change of how the thread runs, by whom.
    Config {
        actor: String,
        #[serde(flatten)]
        change: ConfigChange,
    },
}

/// What waits in an inbox: its id, by which it is withdrawn, and what was delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    pub id: String,
    pub inbound: Inbound,
}

/// The inbox's directory in the thread `id`.
#[must_use]
pub fn path(id: &str) -> Bytes {
    thread::within(id, "inbox")
}

/// The file of what was delivered as `delivery` to the thread `id`.
#[must_use]
pub fn file(id: &str, delivery: &str) -> Bytes {
    [&path(id)[..], b"/", delivery.as_bytes(), b".json"].concat().into()
}

/// Puts `inbound` in the inbox of the thread `id` as `delivery`, with `changes`.
///
/// # Errors
/// What cannot be written as JSON.
pub(crate) fn put(changes: &mut Changes, id: &str, delivery: &str, inbound: &Inbound) -> serde_json::Result<()> {
    changes.write(file(id, delivery), serde_json::to_vec(inbound)?);
    Ok(())
}

/// What waits in the inbox of the thread `id`, in the order it came.
///
/// # Errors
/// The volume's failure; a file that is not what was delivered.
pub fn waiting(snapshot: &Snapshot, id: &str) -> Result<Vec<Waiting>, Error> {
    let range = Range {
        limit: usize::MAX,
        ..Range::default()
    };
    let mut out = Vec::new();
    for entry in snapshot.list(&path(id), &range)?.unwrap_or_default() {
        let name = String::from_utf8_lossy(&entry.name);
        let Some(delivery) = name.strip_suffix(".json") else { continue };
        let bytes = snapshot.read(&file(id, delivery), 0, u64::MAX)?.unwrap_or_default();
        out.push(Waiting {
            id: delivery.to_owned(),
            inbound: serde_json::from_slice(&bytes)?,
        });
    }
    Ok(out)
}

/// What was taken from an inbox into the log: the entry, and where it is.
#[derive(Debug, Clone, PartialEq)]
pub struct Taken {
    pub entry: Entry,
    pub address: Address,
}

/// Takes what waits in the inbox of the thread `id` into its log, in the order it came, with the run's commit: each
/// input as an entry; each change of how it runs as an entry of what it changed, from how the thread runs as the
/// changes before it left it, and set on the thread's directory — a change of nothing taken, and logged not at all.
///
/// # Errors
/// The volume's failure; what cannot be read or written as JSON.
pub fn take(run: &mut Run<'_>, id: &str, writer: &mut Writer) -> Result<Vec<Taken>, Error> {
    let waiting = waiting(run.snapshot(), id)?;
    if waiting.is_empty() {
        return Ok(Vec::new());
    }
    let directory = run
        .snapshot()
        .stat(&thread::path(id))?
        .and_then(|entry| entry.directory)
        .unwrap_or_default();
    let mut config = Config::of(&directory)?;
    let mut taken = Vec::new();
    for each in waiting {
        run.changes.delete(file(id, &each.id));
        let entry = match each.inbound {
            Inbound::Input { actor, content } => Entry::Input {
                id: run.new_id()?,
                actor,
                delivery: each.id,
                content,
            },
            Inbound::Config { actor, change } => {
                let (after, changed) = config.with(&change);
                if changed.is_empty() {
                    continue;
                }
                config = after;
                run.changes.meta(thread::path(id), Vec::new(), changed.annotations());
                Entry::Config {
                    id: run.new_id()?,
                    actor,
                    changed,
                }
            }
        };
        let address = writer.append(&mut run.changes, &entry)?;
        taken.push(Taken { entry, address });
    }
    Ok(taken)
}
