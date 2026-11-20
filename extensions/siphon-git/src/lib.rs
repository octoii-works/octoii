//! Git as an extension of Octoii's: the repository a folder of the machine's is in, as git tells of it — its changes, a
//! file's patch and its two versions, a branch's commits — for the Diff view.

mod context;
pub mod git;
pub mod protocol;
pub mod start;
pub mod tool;

use std::path::Path;

use octoii::connection::refusal;
use octoii::machine::{Base, Writes};
use octoii::protocol::{Error, Scope};
use octoii::{Caller, Extension, Registrar};
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::git::{Git, GitError};
use crate::protocol::{Commits, InRepository, Method, Patch, Versions, Worktrees};

/// Git, as a host adds it.
#[derive(Debug, Clone)]
pub struct GitExtension;

impl Extension for GitExtension {
    fn name(&self) -> &'static str {
        start::OWNER
    }

    fn title(&self) -> &'static str {
        "Git"
    }

    fn description(&self) -> &'static str {
        "What git tells of a thread's repository — its changes, patches and commits — for the Diff view."
    }

    fn build(self: Box<Self>, registrar: &mut Registrar<'_>) {
        registrar.tool(tool::GitTool);
        registrar.teller(octoii::agent::Shared::Machine, std::sync::Arc::new(context::GitContext));
        registrar.teller(octoii::agent::Shared::Project, std::sync::Arc::new(context::RepositoryContext));
        registrar.workspace_preparer(start::WorktreePreparation);
        registrar.methods(protocol::set(), Method::read, |caller: Caller, method| async move {
            answered(&caller, method).await
        });
    }
}

fn failed(error: impl std::fmt::Display) -> Error {
    refusal("failed", &error.to_string())
}

fn told(error: &GitError) -> Error {
    match error {
        GitError::NoRepository(_) => refusal("no_repository", &error.to_string()),
        _ => failed(error),
    }
}

/// Does the git method `method` for the client of `caller`.
async fn answered(caller: &Caller, method: Method) -> Result<serde_json::Value, Error> {
    let volume = caller.volume();
    if let Method::GitRetryStart(asked) = &method {
        caller.require_scope(Scope::ThreadsOperate)?;
        return json(&start::retry(&volume, asked).await.map_err(failed)?);
    }
    let commands = volume.commands();
    let machine = match &method {
        Method::GitCreateWorktree(_) => {
            caller.require_scope(Scope::GitOperate)?;
            commands.open(
                caller.host(),
                Base {
                    workspace: "/".into(),
                    home: caller.host().home.clone(),
                },
                &Writes::Anywhere,
            )
        }
        _ => caller.person_machine(),
    }
    .map_err(failed)?;
    let git = Git {
        machine: &machine,
        commands: &commands,
        cancelled: CancellationToken::new(),
    };
    told_by(&git, method).await
}

fn json(value: &impl Serialize) -> Result<serde_json::Value, Error> {
    serde_json::to_value(value).map_err(failed)
}

async fn told_by(git: &Git<'_>, method: Method) -> Result<serde_json::Value, Error> {
    match method {
        Method::GitBranches(asked) => {
            let root = Path::new(&asked.path);
            let repository = git
                .repository(root)
                .await
                .map_err(|error| told(&error))?
                .ok_or_else(|| refusal("no_repository", "source is not a repository"))?;
            json(&protocol::Branches {
                repository,
                branches: git.branches(root).await.map_err(|error| told(&error))?,
            })
        }
        Method::GitRetryStart(_) => unreachable!("startup retry is handled before ordinary Git queries"),
        Method::GitWorktrees(asked) => json(&Worktrees {
            worktrees: git.worktrees(Path::new(&asked.path)).await.map_err(|error| told(&error))?,
        }),
        Method::GitCreateWorktree(asked) => json(
            &git.create_worktree(Path::new(&asked.path), Path::new(&asked.destination), &asked.branch, &asked.base)
                .await
                .map_err(|error| told(&error))?,
        ),
        Method::GitRepository(asked) => json(&InRepository {
            repository: git.repository(Path::new(&asked.path)).await.map_err(|error| told(&error))?,
        }),
        Method::GitChanges(asked) => json(
            &git.changes(Path::new(&asked.path), &asked.scope)
                .await
                .map_err(|error| told(&error))?,
        ),
        Method::GitPatch(asked) => {
            let patch = git
                .patch(Path::new(&asked.path), &asked.comparison, &asked.file, asked.context)
                .await;
            json(&Patch {
                patch: patch.map_err(|error| told(&error))?,
            })
        }
        Method::GitVersions(asked) => {
            let (old, new) = git
                .versions(Path::new(&asked.path), &asked.comparison, &asked.file)
                .await
                .map_err(|error| told(&error))?;
            json(&Versions { old, new })
        }
        Method::GitCommits(asked) => {
            let (commits, truncated) = git
                .commits(Path::new(&asked.path), asked.base.as_deref())
                .await
                .map_err(|error| told(&error))?;
            json(&Commits { commits, truncated })
        }
    }
}
