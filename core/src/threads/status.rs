//! How a thread stands, as a list of threads and a thread's own view show it: how its latest turn went and when, what
//! its model requests used, what waits in its inbox, and what it asks the person.

use bytes::Bytes;
use octoii_siphon_lib::{Batch, FiberRecord, Named, RuntimeHandle, fiber_path};
use octoii_siphon_lib::{Directory, Snapshot};
use serde::{Deserialize, Serialize};

use crate::threads::config::Config;
use crate::threads::inbox::{self, Waiting};
use crate::threads::marks::{Marks, SNOOZE_NAME};
use crate::threads::operations::Error;
use crate::threads::thread::{self, PARENT_CALL_ANNOTATION, PARENT_LABEL, PROJECT_LABEL, SUB_THREAD_KIND_ANNOTATION};
use crate::volume::Volume;

/// How a thread's turns stand: none run yet, one running, or how the last ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "kebab-case")]
pub enum Turn {
    #[default]
    Created,
    Running,
    Replied,
    Failed,
    Stopped,
}

/// How a thread's turns stand, set by its main process in the commits that begin and end a turn.
pub const TURN_ANNOTATION: &str = "siphon.turn";
const TURN_ID_ANNOTATION: &str = "siphon.turn_id";
pub const BEGAN_AT_ANNOTATION: &str = "siphon.began_at";
pub const ENDED_AT_ANNOTATION: &str = "siphon.ended_at";

/// The changes that mark a turn of the thread `id` begun at `at`: running, its last end let go of.
///
/// # Errors
/// The volume's failure.
pub fn began(changes: &mut Batch<'_>, id: &str, turn: &str, at: i64) -> octoii_siphon_lib::Result<()> {
    let annotations = vec![
        named(TURN_ANNOTATION, Some(serde_json::to_vec(&Turn::Running)?)),
        named(TURN_ID_ANNOTATION, Some(turn.as_bytes().to_vec())),
        named(BEGAN_AT_ANNOTATION, Some(at.to_string().into_bytes())),
        named(ENDED_AT_ANNOTATION, None),
    ];
    changes.meta(thread::path(id), Vec::new(), annotations)?;
    Ok(())
}

/// The changes that mark the turn of the thread `id` ended as `turn` at `at`.
///
/// # Errors
/// The volume's failure.
pub fn ended(changes: &mut Batch<'_>, id: &str, turn: Turn, at: i64) -> octoii_siphon_lib::Result<()> {
    let annotations = vec![
        named(TURN_ANNOTATION, Some(serde_json::to_vec(&turn)?)),
        named(TURN_ID_ANNOTATION, None),
        named(ENDED_AT_ANNOTATION, Some(at.to_string().into_bytes())),
    ];
    changes.meta(thread::path(id), Vec::new(), annotations)?;
    Ok(())
}

fn named(name: &'static str, value: Option<Vec<u8>>) -> Named {
    (Bytes::from_static(name.as_bytes()), value.map(Bytes::from))
}

pub(super) fn turn_id(directory: &Directory) -> Option<&str> {
    directory
        .annotation(TURN_ID_ANNOTATION.as_bytes())
        .and_then(|id| std::str::from_utf8(id).ok())
}

/// What a thread's model requests used, as Beak counts it on its main process for every process under it, as cgroup v2
/// charges a group and its ancestors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct ThreadUsage {
    pub input_tokens: u64,
    pub cached_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    pub cost_nanodollars: u64,
}

impl ThreadUsage {
    /// The counters Beak keeps it as, by name.
    pub const COUNTERS: [&'static str; 5] = [
        "input-tokens",
        "cached-tokens",
        "output-tokens",
        "reasoning-tokens",
        "cost-nanodollars",
    ];

    /// What `used` charges, as the counters Beak keeps it as: each by its name, those it used none of left out.
    #[must_use]
    pub fn counters_of(used: &octoii_ai::vocabulary::Usage) -> Vec<(Bytes, u64)> {
        let values = [
            used.input_tokens,
            used.cached_tokens,
            used.output_tokens,
            used.reasoning_tokens,
            used.cost_nanodollars.unwrap_or(0),
        ];
        Self::COUNTERS
            .iter()
            .zip(values)
            .filter(|(_, value)| *value > 0)
            .map(|(name, value)| (Bytes::from_static(name.as_bytes()), value))
            .collect()
    }

    /// The usage `counters` hold; none where they hold none of it.
    #[must_use]
    pub fn of(counters: &[(Bytes, u64)]) -> Option<Self> {
        let mut usage = Self::default();
        let mut found = false;
        for (name, value) in counters {
            let Some(at) = Self::COUNTERS.iter().position(|counter| counter.as_bytes() == name) else {
                continue;
            };
            found = true;
            *[
                &mut usage.input_tokens,
                &mut usage.cached_tokens,
                &mut usage.output_tokens,
                &mut usage.reasoning_tokens,
                &mut usage.cost_nanodollars,
            ][at] = *value;
        }
        found.then_some(usage)
    }
}

/// A person prompt with its request identity, business key and answer schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct Ask {
    pub id: String,
    pub key: String,
    pub name: String,
    pub payload: serde_json::Value,
    pub answer_schema: serde_json::Value,
}

/// How a thread stands. The active turn's identity is recorded when the main process starts the turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct Status {
    /// Preparation state, absent for a normally created thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start: Option<super::Start>,
    pub turn: Turn,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub began_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<ThreadUsage>,
    /// What waits in its inbox, by its id, in the order it came.
    pub inbox: Vec<(String, inbox::Inbound)>,
    pub asks: Vec<Ask>,
    /// Whether it compacts its window now.
    pub compacting: bool,
    /// When it goes on, where the person asked it to at a time and it has not gone on since.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resuming_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub turn_id: Option<String>,
}

/// Where a sub-thread came from: the thread that delegated it, and its call that did, by Octoii's key of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct Parent {
    pub thread: String,
    pub call: String,
    pub kind: SubThreadKind,
}

/// A sub-thread's fixed scope, chosen when it is created and retained for every later task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum SubThreadKind {
    Explorer,
    GeneralPurpose,
}

impl Parent {
    /// Reads the sub-thread's immutable parent metadata and required kind.
    pub(crate) fn of(directory: &Directory) -> Result<Option<Self>, Error> {
        let Some(thread) = text(directory.label(PARENT_LABEL.as_bytes())) else {
            return Ok(None);
        };
        let kind = directory
            .annotation(SUB_THREAD_KIND_ANNOTATION.as_bytes())
            .ok_or(Error::MissingSubThreadKind)?;
        Ok(Some(Self {
            thread,
            call: text(directory.annotation(PARENT_CALL_ANNOTATION.as_bytes())).unwrap_or_default(),
            kind: serde_json::from_slice(kind)?,
        }))
    }
}

/// The thread and call that delegated `id`, if it belongs to a parent. A missing thread has no parent.
///
/// # Errors
/// The volume's failure, or invalid parent metadata.
pub fn parent_of(volume: &Volume, id: &str) -> anyhow::Result<Option<Parent>> {
    let directory = thread::directory(&*volume.snapshot()?, id)?;
    Ok(directory.as_ref().map(Parent::of).transpose()?.flatten())
}

/// A thread as a list of them shows it: its id, its project and parent, how it runs, how the person keeps it, and how
/// it stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct Listed {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<Parent>,
    pub config: Config,
    pub marks: Marks,
    pub status: Status,
}

fn text(value: Option<&Bytes>) -> Option<String> {
    value
        .filter(|value| !value.is_empty())
        .map(|value| String::from_utf8_lossy(value).into_owned())
}

fn time(directory: &Directory, name: &str) -> Option<i64> {
    directory
        .annotation(name.as_bytes())
        .and_then(|value| std::str::from_utf8(value).ok()?.parse().ok())
}

/// The thread `id`, whose directory is `directory`, as a list shows it.
///
/// # Errors
/// The volume's failure; a directory whose annotations or inbox are not what Octoii writes.
pub fn listed(runtime: &RuntimeHandle, snapshot: &Snapshot, id: &str, directory: &Directory) -> Result<Listed, Error> {
    let parent = Parent::of(directory)?;
    let snooze = snapshot.stat(&thread::within(id, SNOOZE_NAME))?;
    Ok(Listed {
        id: id.to_owned(),
        project: text(directory.label(PROJECT_LABEL.as_bytes())),
        parent,
        config: Config::of(directory)?,
        marks: Marks::of(directory, snooze.as_ref()),
        status: status(runtime, snapshot, id, directory)?,
    })
}

fn status(runtime: &RuntimeHandle, snapshot: &Snapshot, id: &str, directory: &Directory) -> Result<Status, Error> {
    let turn = directory
        .annotation(TURN_ANNOTATION.as_bytes())
        .map(|value| serde_json::from_slice(value))
        .transpose()?
        .unwrap_or_default();
    let main = thread::main(id);
    let usage = snapshot.fiber(&main)?.and_then(|record| ThreadUsage::of(&record.usage));
    let inbox = inbox::waiting(snapshot, id)?
        .into_iter()
        .map(|Waiting { id, inbound }| (id, inbound))
        .collect();
    let running = |key: &str| -> Result<Option<FiberRecord>, Error> {
        let record = snapshot.fiber(&fiber_path(&main, key.as_bytes()))?;
        Ok(record.filter(|record| record.outcome.is_none()))
    };
    Ok(Status {
        start: super::Start::of(directory)?.map(super::Start::observed),
        turn,
        turn_id: turn_id(directory).map(str::to_owned),
        began_at: time(directory, BEGAN_AT_ANNOTATION),
        ended_at: time(directory, ENDED_AT_ANNOTATION),
        usage,
        inbox,
        asks: asks(runtime, snapshot, &main)?,
        compacting: running_compaction_child(snapshot, &main)?.is_some() || turn_compacting(snapshot, &main)?,
        resuming_at: running(thread::RESUME_KEY)?.and_then(|record| record.waits_until),
    })
}

/// The compaction record under `parent`, whether it still holds the slot or can be reaped.
pub(crate) fn compaction_child(snapshot: &Snapshot, parent: &[u8]) -> octoii_siphon_lib::Result<Option<FiberRecord>> {
    snapshot.fiber(&fiber_path(parent, thread::COMPACTION_KEY.as_bytes()))
}

/// A compaction holds its slot until its descendants exit and it can be reaped.
pub(crate) fn running_compaction_child(snapshot: &Snapshot, parent: &[u8]) -> octoii_siphon_lib::Result<Option<FiberRecord>> {
    Ok(compaction_child(snapshot, parent)?.filter(|record| !record.reapable))
}

pub(crate) fn turn_compacting(snapshot: &Snapshot, main: &[u8]) -> octoii_siphon_lib::Result<bool> {
    for turn in snapshot.fibers(main, thread::TURN_KIND.as_bytes())? {
        if running_compaction_child(snapshot, turn.path())?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// What the thread asks the person: each request of its Fibers in the order of their paths.
fn asks(runtime: &RuntimeHandle, snapshot: &Snapshot, main: &[u8]) -> Result<Vec<Ask>, Error> {
    runtime
        .person_requests(snapshot, main)?
        .into_iter()
        .map(|prompt| {
            Ok(Ask {
                id: prompt.id.to_token(),
                key: String::from_utf8_lossy(prompt.id.key()).into_owned(),
                name: String::from_utf8_lossy(&prompt.name).into_owned(),
                payload: serde_json::from_slice(&prompt.payload)?,
                answer_schema: prompt.answer_schema,
            })
        })
        .collect()
}
