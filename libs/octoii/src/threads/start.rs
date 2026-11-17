//! A thread's startup gate. The main process alone changes it, consuming durable control messages before inputs.
use beak::{Directory, Range, Snapshot};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use siphon_runtime::{Changes, Run};

use super::{
    Config,
    config::ConfigChange,
    log::{Entry, Writer},
    thread,
};

const ANNOTATION: &[u8] = b"siphon.start";

/// An extension's accepted preparation request. Octoii does not interpret its payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct StartRequest {
    pub owner: String,
    pub payload: serde_json::Value,
}

/// The final directory and project identity, resolved by the preparing extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct StartWorkspace {
    pub workspace: String,
    pub project: Option<String>,
}

/// Preparation never runs the agent until it becomes ready.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum StartState {
    Pending,
    Failed { reason: String },
    Cancelled,
    Ready { directory: StartWorkspace },
}

/// Durable startup state. Tokens fence results from an older preparation attempt; they are not authorization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct Start {
    pub request: StartRequest,
    pub token: String,
    pub previous: Option<String>,
    pub state: StartState,
}

impl Start {
    pub(crate) fn of(directory: &Directory) -> serde_json::Result<Option<Self>> {
        directory
            .annotation(ANNOTATION)
            .map(|bytes| serde_json::from_slice(bytes))
            .transpose()
    }

    #[must_use]
    pub fn blocks_execution(&self) -> bool {
        !matches!(self.state, StartState::Ready { .. })
    }
}

pub(crate) fn read(snapshot: &Snapshot, id: &str) -> anyhow::Result<Option<Start>> {
    let directory = snapshot
        .stat(&thread::path(id))?
        .and_then(|entry| entry.directory)
        .filter(|directory| !super::operations::removing(directory))
        .ok_or_else(|| anyhow::anyhow!("no such thread"))?;
    Ok(Start::of(&directory)?)
}

pub(crate) fn commands(id: &str) -> Bytes {
    thread::within(id, "start.commands")
}
fn results(id: &str) -> Bytes {
    thread::within(id, "start.results")
}
pub(crate) fn command_path(id: &str, key: &str) -> Bytes {
    [commands(id).as_ref(), b"/", key.as_bytes()].concat().into()
}
pub(crate) fn result_path(id: &str, key: &str) -> Bytes {
    [results(id).as_ref(), b"/", key.as_bytes()].concat().into()
}

pub(crate) fn keep(changes: &mut Changes, id: &str, start: &Start) -> serde_json::Result<()> {
    changes.meta(
        thread::path(id),
        vec![],
        vec![(Bytes::from_static(ANNOTATION), Some(serde_json::to_vec(start)?.into()))],
    );
    Ok(())
}

pub(crate) fn made(changes: &mut Changes, id: &str, start: &Start) -> serde_json::Result<()> {
    keep(changes, id, start)?;
    changes.mkdir(commands(id)).mkdir(results(id));
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Command {
    pub token: String,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum Action {
    Ready(StartWorkspace),
    Fail(String),
    Retry { next: String },
    Cancel,
}

pub(crate) enum Gate {
    Open,
    Blocked,
    Changed,
}

/// Only this thread's main process consumes these commands. Every decision and its reply share its commit.
pub(crate) fn advance(run: &mut Run<'_>, id: &str) -> anyhow::Result<Gate> {
    let Some(mut start) = read(run.snapshot(), id)? else {
        return Ok(Gate::Open);
    };
    let entries = run
        .snapshot()
        .list(
            &commands(id),
            &Range {
                limit: usize::MAX,
                ..Range::default()
            },
        )?
        .unwrap_or_default();
    if entries.is_empty() {
        return Ok(if start.blocks_execution() { Gate::Blocked } else { Gate::Open });
    }
    for entry in entries {
        let key = std::str::from_utf8(&entry.name)?;
        let path = command_path(id, key);
        let bytes = run
            .snapshot()
            .read(&path, 0, u64::MAX)?
            .ok_or_else(|| anyhow::anyhow!("missing startup command"))?;
        let command: Command = serde_json::from_slice(&bytes)?;
        let result = transitioned(&start, command);
        if let Ok(next) = &result
            && next != &start
        {
            if let StartState::Ready { directory } = &next.state {
                bind(run, id, directory)?;
            }
            start = next.clone();
        }
        run.changes.write(result_path(id, key), serde_json::to_vec(&result)?).delete(path);
    }
    keep(&mut run.changes, id, &start)?;
    Ok(Gate::Changed)
}

fn transitioned(start: &Start, command: Command) -> Result<Start, String> {
    if command.token != start.token {
        if matches!(command.action, Action::Retry { .. })
            && start.previous.as_deref() == Some(&command.token)
            && matches!(start.state, StartState::Pending)
        {
            return Ok(start.clone());
        }
        return Err("stale startup attempt".into());
    }
    let mut next = start.clone();
    next.state = match (&start.state, command.action) {
        (StartState::Pending, Action::Ready(directory)) => {
            if !std::path::Path::new(&directory.workspace).is_absolute()
                || directory
                    .project
                    .as_ref()
                    .is_some_and(|path| !std::path::Path::new(path).is_absolute())
            {
                return Err("startup paths must be absolute".into());
            }
            StartState::Ready { directory }
        }
        (StartState::Ready { directory: before }, Action::Ready(after)) if before == &after => return Ok(next),
        (StartState::Pending, Action::Fail(reason)) => StartState::Failed { reason },
        (StartState::Failed { reason: before }, Action::Fail(after)) if before == &after => return Ok(next),
        (StartState::Failed { .. }, Action::Retry { next: token }) => {
            next.previous = Some(start.token.clone());
            next.token = token;
            StartState::Pending
        }
        (StartState::Pending | StartState::Failed { .. } | StartState::Cancelled, Action::Cancel) => StartState::Cancelled,
        _ => return Err("startup transition is not allowed".into()),
    };
    Ok(next)
}

fn bind(run: &mut Run<'_>, id: &str, directory: &StartWorkspace) -> anyhow::Result<()> {
    let stored = run
        .snapshot()
        .stat(&thread::path(id))?
        .and_then(|entry| entry.directory)
        .ok_or_else(|| anyhow::anyhow!("no such thread"))?;
    let (_, changed) = Config::of(&stored)?.with(&ConfigChange {
        workspace: Some(Some(directory.workspace.clone())),
        ..ConfigChange::default()
    });
    run.changes.meta(
        thread::path(id),
        vec![(
            Bytes::from_static(thread::PROJECT_LABEL.as_bytes()),
            Some(Bytes::copy_from_slice(directory.project.as_deref().unwrap_or_default().as_bytes())),
        )],
        changed.annotations(),
    );
    if !changed.is_empty() {
        let entry = Entry::Config {
            id: run.new_id()?,
            actor: super::log::SIPHON.into(),
            changed,
        };
        Writer::open(run.snapshot(), id)?.append(&mut run.changes, &entry)?;
    }
    Ok(())
}
