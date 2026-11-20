//! Explicit worktree preparation for an accepted Thread. Inspection never adopts an existing directory.
use std::path::Path;

use octoii::Caller;
use octoii::machine::{Base, Writes};
use octoii::protocol::Scope;
use octoii::threads::{Config, InputPart, NewThread, Start, StartDirectory, StartRequest, StartState, StartWorkspace};
use octoii::volume::Volume;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::git::{Git, WorktreeEntry};
use crate::protocol::CreateWorktree;

const OWNER: &str = "git.worktree";

/// Prepare or inspect a worktree for an accepted Thread. Prepare and retry are explicit writes.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum StartAction {
    Prepare { thread: String, token: String },
    Retry { thread: String, token: String },
    Inspect { thread: String },
}

#[derive(Debug, Serialize, Deserialize, TS)]
pub struct ThreadStart {
    pub thread: String,
    pub start: Start,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub worktrees: Option<Vec<WorktreeEntry>>,
}

/// Called under the extension's preparation lock, shared by every connection to this node.
pub(crate) async fn answer(caller: &Caller, action: StartAction) -> anyhow::Result<ThreadStart> {
    let volume = caller.volume();
    let thread = match &action {
        StartAction::Prepare { thread, .. } | StartAction::Retry { thread, .. } | StartAction::Inspect { thread } => thread.clone(),
    };
    let mut start = volume
        .thread_start(&thread)?
        .ok_or_else(|| anyhow::anyhow!("thread has no preparation"))?;
    anyhow::ensure!(start.request.owner == OWNER, "thread preparation belongs to another extension");
    let request: CreateWorktree = serde_json::from_value(start.request.payload.clone())?;
    match &action {
        StartAction::Prepare { token, .. } => {
            anyhow::ensure!(start.token == *token, "stale preparation attempt");
            if !matches!(start.state, StartState::Pending) {
                return Ok(ThreadStart {
                    thread,
                    start,
                    worktrees: None,
                });
            }
        }
        StartAction::Retry { token, .. } => start = volume.retry_thread_start(&thread, token).await?,
        StartAction::Inspect { .. } => {}
    }
    let commands = volume.commands();
    let machine = if matches!(action, StartAction::Inspect { .. }) {
        caller.person_machine()?
    } else {
        commands.open(
            caller.host(),
            Base {
                workspace: "/".into(),
                home: caller.host().home.clone(),
            },
            &Writes::Anywhere,
        )?
    };
    let git = Git {
        machine: &machine,
        commands: &commands,
        cancelled: tokio_util::sync::CancellationToken::new(),
    };
    if matches!(action, StartAction::Inspect { .. }) {
        let worktrees = git.worktrees(Path::new(&request.path)).await?;
        return Ok(ThreadStart {
            thread,
            start,
            worktrees: Some(worktrees),
        });
    }
    let made = async {
        let source = git
            .repository(Path::new(&request.path))
            .await?
            .ok_or_else(|| anyhow::anyhow!("source is not a repository"))?;
        let worktree = git
            .create_worktree(
                Path::new(&request.path),
                Path::new(&request.destination),
                &request.branch,
                &request.base,
            )
            .await?;
        Ok::<_, anyhow::Error>(StartWorkspace {
            workspace: worktree.path,
            project: Some(source.root),
        })
    }
    .await;
    let start = match made {
        Ok(directory) => volume.finish_thread_start(&thread, &start.token, directory).await?,
        Err(error) => volume.fail_thread_start(&thread, &start.token, error.to_string()).await?,
    };
    Ok(ThreadStart {
        thread,
        start,
        worktrees: None,
    })
}

pub(crate) fn authorize(caller: &Caller) -> Result<(), octoii::protocol::Error> {
    caller.require_scope(Scope::GitOperate)?;
    caller.require_scope(Scope::ThreadsOperate)
}

/// Accept a worktree task and its input behind the Thread's startup gate.
/// The host must authorize Git and Thread operations before calling this trusted interface.
///
/// # Errors
/// Invalid task parameters, conflicting idempotency key, attachment processing or storage failure.
pub async fn accept(
    volume: &Volume,
    caller: &str,
    key: &str,
    config: Config,
    content: Vec<InputPart>,
    worktree: CreateWorktree,
) -> anyhow::Result<String> {
    anyhow::ensure!(Path::new(&worktree.path).is_absolute(), "source must be absolute");
    anyhow::ensure!(Path::new(&worktree.destination).is_absolute(), "destination must be absolute");
    anyhow::ensure!(
        crate::git::is_commit_id(&worktree.base),
        "preparation requires a full commit ID, not a moving revision"
    );
    anyhow::ensure!(config.workspace.is_none(), "the workspace is supplied by worktree preparation");
    let new = NewThread {
        actor: "person".into(),
        config,
        project: Some(worktree.path.clone()),
        ..NewThread::default()
    };
    volume
        .start_thread(
            caller,
            key,
            &new,
            content,
            StartDirectory::Prepare(StartRequest {
                owner: OWNER.into(),
                payload: serde_json::to_value(worktree)?,
            }),
        )
        .await
}
