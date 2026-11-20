//! Accepting a thread and its first input. A thread with preparation waits behind a startup gate that its main
//! process advances, consuming durable control messages before inputs.

use std::path::Path;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bytes::Bytes;
use octoii_siphon_lib::{Batch, Changes, CommitEvent, Fiber, Run};
use octoii_siphon_lib::{Directory, Range, Snapshot};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
const CLAIMS: &str = "octoii.start";

/// An extension's accepted preparation request. Octoii does not interpret its payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct StartRequest {
    pub owner: String,
    pub payload: serde_json::Value,
}

/// The resolved directory and project identity in which a thread runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct StartWorkspace {
    pub workspace: String,
    pub project: Option<String>,
}

impl StartWorkspace {
    fn validate(&self) -> Result<(), &'static str> {
        if Path::new(&self.workspace).is_absolute() && self.project.as_deref().is_none_or(|path| Path::new(path).is_absolute()) {
            Ok(())
        } else {
            Err("startup paths must be absolute")
        }
    }
}

/// Where a thread starts. A current directory is already resolved by the host.
#[derive(Debug, Clone)]
pub enum StartDirectory {
    Current(StartWorkspace),
    Prepare(StartRequest),
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

    fn can_cancel(&self) -> bool {
        matches!(self.state, StartState::Pending | StartState::Failed { .. })
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct StartClaim {
    fingerprint: [u8; 32],
    thread: String,
}

impl<M: Fiber<Input = MainInput>> Threads<M> {
    /// Atomically accepts a thread, the first input, and any attachments. The thread starts in its resolved
    /// directory or waits for preparation to bind it. The caller's key is claimed with the same commit.
    ///
    /// # Errors
    /// A reused key with different input, a missing deleted thread, or a storage failure.
    pub async fn start(
        &self,
        caller: &str,
        key: &str,
        new: &NewThread,
        content: Vec<InputPart>,
        directory: StartDirectory,
    ) -> Result<String, Error> {
        idempotency::validate_key(key)?;
        if caller.trim().is_empty() || content.is_empty() {
            return Err(Error::Start("startup requires a caller and initial input".into()));
        }
        let key = Sha256::digest(serde_json::to_vec(&(caller, key))?);
        let path = Bytes::from(format!("{CLAIMS}/{}", URL_SAFE_NO_PAD.encode(key)));
        let preparation = match &directory {
            StartDirectory::Current(_) => None,
            StartDirectory::Prepare(request) => Some(request),
        };
        let fingerprint = input::fingerprint(new, &content, preparation)?;
        let runtime = self.runtime();
        let snapshot = runtime.snapshot()?;
        if let Some(thread) = claimed(&snapshot, &path, &fingerprint)? {
            return Ok(thread);
        }

        let mut bound = new.clone();
        let preparation = match directory {
            StartDirectory::Current(workspace) => {
                workspace.validate().map_err(|reason| Error::Start(reason.into()))?;
                bound.config.workspace = Some(workspace.workspace);
                bound.project = workspace.project;
                None
            }
            StartDirectory::Prepare(request) => {
                validate_preparation(new, &request)?;
                Some(request)
            }
        };
        let claim = StartClaim {
            fingerprint,
            thread: runtime.new_id()?,
        };
        let mut changes = Changes::new()?;
        Self::made(&mut changes, &claim.thread, &runtime.new_id()?, &bound)?;
        if let Some(request) = preparation {
            let start = Start {
                request,
                token: runtime.new_id()?,
                previous: None,
                state: StartState::Pending,
            };
            made(&mut changes, &claim.thread, &start)?;
        }
        let content = input::keep(&snapshot, &mut changes, &claim.thread, content)?;
        inbox::put(
            &mut changes,
            &claim.thread,
            &runtime.new_id()?,
            &Inbound::Input {
                actor: bound.actor,
                content,
                reply_to: None,
                joins: Joins::Step,
            },
        )?;
        // The sealed claim rejects a racing writer in the same commit as the thread and its files.
        changes.mkdir(CLAIMS)?.write(&path, serde_json::to_vec(&claim)?)?.seal(&path)?;
        drop(snapshot);
        if let Err(error) = runtime.commit(changes).await {
            return claimed(&*runtime.snapshot()?, &path, &fingerprint)?.ok_or_else(|| error.into());
        }
        Ok(claim.thread)
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
            let start = read(snapshot, id)?.ok_or_else(|| command.action.refused("thread has no startup gate".into()))?;
            if matches!(command.action, Action::Cancel) && (start.token != token || !start.can_cancel()) {
                return Err(Error::StaleStop);
            }
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
                return result.map_err(|reason| command.action.refused(reason));
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
}

fn validate_preparation(new: &NewThread, request: &StartRequest) -> Result<(), Error> {
    if new.project.as_deref().is_some_and(|project| !Path::new(project).is_absolute()) {
        return Err(Error::Start("the project path must be absolute".into()));
    }
    if request.owner.trim().is_empty() {
        return Err(Error::Start("preparation requires an owner".into()));
    }
    if new.config.workspace.is_some() {
        return Err(Error::Start("preparation supplies the workspace".into()));
    }
    Ok(())
}

fn claimed(snapshot: &Snapshot, path: &[u8], fingerprint: &[u8; 32]) -> Result<Option<String>, Error> {
    let Some(bytes) = snapshot.read_range(path, 0, u64::MAX)? else {
        return Ok(None);
    };
    let kept: StartClaim = serde_json::from_slice(&bytes)?;
    if &kept.fingerprint != fingerprint {
        return Err(Error::Start("startup key was used with different input".into()));
    }
    thread::directory(snapshot, &kept.thread)?.ok_or(Error::NoSuchThread)?;
    Ok(Some(kept.thread))
}

/// Stages cancellation in the same commit as a stop issued by a call's plan.
pub(super) fn stage_cancel(snapshot: &Snapshot, changes: &mut Batch<'_>, id: &str) -> Result<(), Error> {
    if let Some(directory) = thread::directory(snapshot, id)?
        && let Some(start) = Start::of(&directory)?
        && start.can_cancel()
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

impl Action {
    fn refused(&self, reason: String) -> Error {
        match self {
            Self::Cancel => Error::StaleStop,
            _ => Error::Start(reason),
        }
    }
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
            directory.validate()?;
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
        (_, Action::Cancel) if start.can_cancel() => StartState::Cancelled,
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
