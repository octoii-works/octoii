//! Accepting a thread and its first input behind a startup gate. The main process alone advances that gate,
//! consuming durable control messages before inputs.

use bytes::Bytes;
use octoii_siphon_lib::{Batch, Changes, CommitEvent, Fiber, Run};
use octoii_siphon_lib::{Directory, Range, Snapshot};
use serde::{Deserialize, Serialize};

use crate::volume::commit_checked;

use super::{
    Config, Error, InputPart, MainInput, NewThread, Threads,
    config::ConfigChange,
    idempotency,
    inbox::{self, Inbound, Joins},
    input,
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

#[derive(Debug, Serialize, Deserialize)]
struct StartClaim {
    fingerprint: [u8; 32],
    thread: String,
}

impl<M: Fiber<Input = MainInput>> Threads<M> {
    /// Atomically accepts a thread and its first input, without permitting execution.
    /// The immutable claim has no time-based expiry; deleting its thread never recreates it on replay.
    ///
    /// # Errors
    /// A reused key with different input, a missing deleted thread, or a storage failure.
    pub async fn prepare(
        &self,
        new: &NewThread,
        content: Vec<InputPart>,
        request: StartRequest,
        key: &str,
    ) -> Result<(String, Start), Error> {
        let runtime = self.runtime();
        idempotency::done_before(&*runtime.snapshot()?, key, "threads.prepare", None)?;
        if request.owner.trim().is_empty() || content.is_empty() {
            return Err(Error::Start("preparation requires an owner and initial input".into()));
        }
        let path: Bytes = [b"siphon.start/", key.as_bytes()].concat().into();
        let claim = StartClaim {
            fingerprint: input::fingerprint(new, &content, &request)?,
            thread: runtime.new_id()?,
        };
        if let Some(bytes) = runtime.snapshot()?.read_range(&path, 0, u64::MAX)? {
            return self.claimed(&bytes, &claim);
        }
        let start = Start {
            request,
            token: runtime.new_id()?,
            previous: None,
            state: StartState::Pending,
        };
        let mut changes = Changes::new()?;
        Self::made(&mut changes, &claim.thread, &runtime.new_id()?, new)?;
        made(&mut changes, &claim.thread, &start)?;
        let content = input::keep(&*runtime.snapshot()?, &mut changes, &claim.thread, content)?;
        inbox::put(
            &mut changes,
            &claim.thread,
            &runtime.new_id()?,
            &Inbound::Input {
                actor: new.actor.clone(),
                content,
                reply_to: None,
                joins: Joins::Step,
            },
        )?;
        changes
            .mkdir(b"siphon.start")?
            .write(&path, serde_json::to_vec(&claim)?)?
            .seal(&path)?;
        if let Err(error) = runtime.commit(changes).await {
            if let Some(bytes) = runtime.snapshot()?.read_range(&path, 0, u64::MAX)? {
                return self.claimed(&bytes, &claim);
            }
            return Err(error.into());
        }
        Ok((claim.thread, start))
    }

    fn claimed(&self, bytes: &[u8], asked: &StartClaim) -> Result<(String, Start), Error> {
        let kept: StartClaim = serde_json::from_slice(bytes)?;
        if kept.fingerprint != asked.fingerprint {
            return Err(Error::Start("startup key was used with different input".into()));
        }
        let start = self
            .start_state(&kept.thread)?
            .ok_or_else(|| Error::Start("missing startup state".into()))?;
        Ok((kept.thread, start))
    }

    /// Reads startup state without exposing its storage paths.
    ///
    /// # Errors
    /// A missing thread or invalid stored state.
    pub fn start_state(&self, id: &str) -> Result<Option<Start>, Error> {
        read(&*self.runtime().snapshot()?, id)
    }

    pub(crate) async fn start_command(&self, id: &str, token: &str, action: Action) -> Result<Start, Error> {
        let runtime = self.runtime();
        let command = Command {
            token: token.into(),
            action,
        };
        let mut commits = runtime.commits()?;
        let key = commit_checked(runtime, |snapshot, changes| -> Result<_, Error> {
            read(snapshot, id)?.ok_or_else(|| Error::Start("thread has no startup gate".into()))?;
            let key = runtime.new_id()?;
            changes.write(command_path(id, &key), serde_json::to_vec(&command)?)?;
            Self::revive(snapshot, changes, id)?;
            Ok(key)
        })
        .await?;
        loop {
            let snapshot = runtime.snapshot()?;
            if let Some(bytes) = snapshot.read_range(&result_path(id, &key), 0, u64::MAX)? {
                let result: Result<Start, String> = serde_json::from_slice(&bytes)?;
                return result.map_err(Error::Start);
            }
            thread::directory(&snapshot, id)?.ok_or(Error::NoSuchThread)?;
            if snapshot.fiber(&thread::main(id))?.is_some_and(|record| record.outcome.is_some()) {
                return Err(Error::Start("thread process ended before startup completed".into()));
            }
            match commits.next().await? {
                Some(CommitEvent::Head(_)) => {}
                Some(CommitEvent::Fault { error, .. }) => return Err(error.into()),
                Some(CommitEvent::Closed(_)) | None => {
                    return Err(Error::Start("runtime closed before startup completed".into()));
                }
            }
        }
    }

    /// Before stopping an existing thread, wait for the startup gate to acknowledge cancellation.
    pub(super) async fn cancel_start(&self, id: &str) -> Result<(), Error> {
        if let Some(start) = self.start_state(id)?
            && start.blocks_execution()
            && let Err(error) = self.start_command(id, &start.token, Action::Cancel).await
            && self.start_state(id)?.is_some_and(|current| current.blocks_execution())
        {
            return Err(error);
        }
        Ok(())
    }
}

/// Stages cancellation in the same commit as a stop issued by a call's plan.
pub(super) fn stage_cancel(snapshot: &Snapshot, changes: &mut Batch<'_>, id: &str) -> Result<(), Error> {
    if let Some(directory) = thread::directory(snapshot, id)?
        && let Some(start) = Start::of(&directory)?
        && matches!(start.state, StartState::Pending | StartState::Failed { .. })
    {
        // Sort before UUID-keyed completions so a queued stop wins before preparation is released.
        let command = Command {
            token: start.token,
            action: Action::Cancel,
        };
        changes.write(command_path(id, "0-stop"), serde_json::to_vec(&command)?)?;
    }
    Ok(())
}

fn read(snapshot: &Snapshot, id: &str) -> Result<Option<Start>, Error> {
    let directory = thread::directory(snapshot, id)?.ok_or(Error::NoSuchThread)?;
    Ok(Start::of(&directory)?)
}

pub(crate) fn commands(id: &str) -> Bytes {
    thread::within(id, "start.commands")
}
fn results(id: &str) -> Bytes {
    thread::within(id, "start.results")
}
fn command_path(id: &str, key: &str) -> Bytes {
    [commands(id).as_ref(), b"/", key.as_bytes()].concat().into()
}
fn result_path(id: &str, key: &str) -> Bytes {
    [results(id).as_ref(), b"/", key.as_bytes()].concat().into()
}

fn keep(changes: &mut Batch<'_>, id: &str, start: &Start) -> octoii_siphon_lib::Result<()> {
    changes.meta(
        thread::path(id),
        vec![],
        vec![(Bytes::from_static(ANNOTATION), Some(serde_json::to_vec(start)?.into()))],
    )?;
    Ok(())
}

fn made(changes: &mut Batch<'_>, id: &str, start: &Start) -> octoii_siphon_lib::Result<()> {
    keep(changes, id, start)?;
    changes.mkdir(commands(id))?.mkdir(results(id))?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Command {
    token: String,
    action: Action,
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
            .read_range(&path, 0, u64::MAX)?
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
        run.changes
            .write(result_path(id, key), serde_json::to_vec(&result)?)?
            .delete(path)?;
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
    )?;
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
