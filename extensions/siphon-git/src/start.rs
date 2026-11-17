//! Explicit worktree preparation for an accepted Thread. Inspection never adopts an existing directory.
use std::path::Path;

use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use siphon::Caller;
use siphon::machine::{Base, Writes};
use siphon::protocol::Scope;
use siphon::threads::{Config, NewThread, Start, StartRequest, StartState, StartWorkspace};
use ts_rs::TS;

use crate::git::{Git, WorktreeEntry};
use crate::protocol::CreateWorktree;

const OWNER: &str = "git.worktree";

/// Accept saves input without executing Git or the Agent. Prepare and retry are explicit writes.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum StartAction {
    Accept {
        key: String,
        config: Config,
        content: Vec<InputContent>,
        worktree: CreateWorktree,
    },
    Prepare {
        thread: String,
        token: String,
    },
    Retry {
        thread: String,
        token: String,
    },
    Inspect {
        thread: String,
    },
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
    if let StartAction::Accept {
        key,
        config,
        content,
        worktree,
    } = action
    {
        return accept(&volume, &key, config, content.into_iter().map(Into::into).collect(), worktree).await;
    }

    let thread = match &action {
        StartAction::Prepare { thread, .. } | StartAction::Retry { thread, .. } | StartAction::Inspect { thread } => thread.clone(),
        StartAction::Accept { .. } => unreachable!(),
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
        _ => {}
    }
    let machine = if matches!(action, StartAction::Inspect { .. }) {
        caller.person_machine()?
    } else {
        volume.commands().open(
            caller.host(),
            Base {
                workspace: "/".into(),
                home: caller.host().home.clone(),
            },
            &Writes::Anywhere,
        )?
    };
    let time = volume.now_ms();
    volume
        .clone()
        .recorded(crate::RECORD_KIND, |under| async move {
            let git = Git {
                machine: &machine,
                under: &under,
                time,
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
            Ok::<_, anyhow::Error>(ThreadStart {
                thread,
                start,
                worktrees: None,
            })
        })
        .await?
}

pub(crate) fn authorize(caller: &Caller) -> Result<(), siphon::protocol::Error> {
    caller.require_scope(Scope::GitOperate)?;
    caller.require_scope(Scope::ThreadsOperate)
}

/// Accept a worktree task through either transport using the same Git validation and atomic Thread operation.
/// The host must authorize Git and Thread operations before calling this trusted interface.
///
/// # Errors
/// Invalid task parameters, conflicting idempotency key, attachment processing or storage failure.
pub async fn accept(
    volume: &siphon::volume::Volume,
    key: &str,
    config: Config,
    content: Vec<siphon::threads::InputPart>,
    worktree: CreateWorktree,
) -> anyhow::Result<ThreadStart> {
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
    let (thread, start) = volume
        .prepare_thread(
            &new,
            content,
            StartRequest {
                owner: OWNER.into(),
                payload: serde_json::to_value(worktree)?,
            },
            key,
        )
        .await?;
    Ok(ThreadStart {
        thread,
        start,
        worktrees: None,
    })
}
