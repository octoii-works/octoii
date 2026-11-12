//! A thread's inbox: what is delivered to it and not yet taken into its log — an input, a change of how it runs —
//! each a file `inbox/<id>.json`, its id made as it came, so the names sort in the order they came. The thread's main
//! process takes them in that order, as a mail queue is drained, each into its log where it then stands, a change of
//! how it runs set on its directory in the same commit. A turn that works takes what joins at its next step, steering
//! it; an input that waits for the next turn stays till the turn ends, and begins the next.

use beak::{Range, Snapshot};
use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use siphon_runtime::{Changes, Mail, Run};

use crate::config::{Config, ConfigChange};
use crate::log::{Address, Entry, Error, Writer};
use crate::thread;

/// What is delivered to a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Inbound {
    /// An input: whose it is — the person's, or a thread's — what it says, who is told how the turn that takes it
    /// ends, where someone waits on it, and when it joins the thread's work.
    Input {
        actor: String,
        content: Vec<InputContent>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_to: Option<ReplyTo>,
        #[serde(default, skip_serializing_if = "Joins::is_step")]
        joins: Joins,
    },
    /// A change of how the thread runs, by whom.
    Config {
        actor: String,
        #[serde(flatten)]
        change: ConfigChange,
    },
}

/// When an input joins a thread's work: at the next step of the turn that works — as its calls end, steering it — or
/// as the next turn, once the turn that works ended; at once where none works.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum Joins {
    #[default]
    Step,
    Turn,
}

impl Joins {
    #[must_use]
    pub fn is_step(&self) -> bool {
        *self == Self::Step
    }
}

/// Who is told how the turn that takes an input ends, by mail of `key`: the process at `process`, by its path, as the
/// commit `started` started it — whichever lives there, where none — as `wait(2)` tells a parent its child's status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct ReplyTo {
    pub process: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<u64>,
    pub key: String,
}

/// How the turn that took an input ended, as who waits on it is told: replied, with what it said last; failed, and
/// why; stopped; or the input taken back before a turn took it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "ended", rename_all = "kebab-case")]
pub enum Ended {
    Replied { said: String },
    Failed { reason: String },
    Stopped,
    Withdrawn,
}

/// The name of the mail that tells how the turn that took an input ended.
pub const ENDED_MAIL: &str = "siphon.ended";

/// Adds to `changes` mail to `reply_to` that the turn that took its input ended as `ended`, where the process waits
/// still — one that ended is told nothing.
///
/// # Errors
/// The volume's failure.
pub fn tell(snapshot: &Snapshot, changes: &mut Changes, reply_to: &ReplyTo, ended: &Ended) -> beak::Result<()> {
    let process = Bytes::copy_from_slice(reply_to.process.as_bytes());
    let lives = snapshot
        .record(&process)?
        .is_some_and(|record| record.outcome.is_none() && reply_to.started.is_none_or(|started| record.created_seq == started));
    if lives {
        let mail = Mail {
            key: Bytes::copy_from_slice(reply_to.key.as_bytes()),
            name: Bytes::from_static(ENDED_MAIL.as_bytes()),
            payload: serde_json::to_vec(ended).unwrap_or_default().into(),
        };
        changes.send(process, mail, reply_to.started);
    }
    Ok(())
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

/// Puts `inbound` in the inbox of the thread `id` as `delivery`, with `changes`, for its main process to take — which
/// the caller sees runs.
///
/// # Errors
/// What cannot be written as JSON.
pub fn put(changes: &mut Changes, id: &str, delivery: &str, inbound: &Inbound) -> serde_json::Result<()> {
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
    take_of(run, id, writer, waiting(run.snapshot(), id)?)
}

/// Takes what waits in the inbox of the thread `id` as `take` does, as a turn that works takes it at its next step:
/// all but an input that waits for the next turn.
///
/// # Errors
/// As `take`'s.
pub fn take_steering(run: &mut Run<'_>, id: &str, writer: &mut Writer) -> Result<Vec<Taken>, Error> {
    let steering = waiting(run.snapshot(), id)?
        .into_iter()
        .filter(|each| !matches!(each.inbound, Inbound::Input { joins: Joins::Turn, .. }))
        .collect();
    take_of(run, id, writer, steering)
}

/// Takes `waiting`, of the inbox of the thread `id`, into its log, as `take` says.
fn take_of(run: &mut Run<'_>, id: &str, writer: &mut Writer, waiting: Vec<Waiting>) -> Result<Vec<Taken>, Error> {
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
            Inbound::Input {
                actor, content, reply_to, ..
            } => Entry::Input {
                id: run.new_id()?,
                actor,
                delivery: each.id,
                content,
                reply_to,
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
