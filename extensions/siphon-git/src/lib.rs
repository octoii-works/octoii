//! Git as an extension of Siphon's: the repository a folder of the machine's is in, as git tells of it — its changes, a
//! file's patch and its two versions, a branch's commits — each run of git recorded while it runs, for the Diff view.

pub mod git;
pub mod protocol;

use std::path::Path;

use serde::Serialize;
use siphon::connection::refusal;
use siphon::protocol::Error;
use siphon::{Caller, Extension, Registrar};
use tokio_util::sync::CancellationToken;

use crate::git::{Git, GitError};
use crate::protocol::{Commits, InRepository, Method, Patch, Versions};

/// Git, as a host adds it.
#[derive(Debug, Clone, Copy, Default)]
pub struct GitExtension;

impl Extension for GitExtension {
    fn name(&self) -> &'static str {
        "git"
    }

    fn title(&self) -> &'static str {
        "Git"
    }

    fn description(&self) -> &'static str {
        "What git tells of a thread's repository — its changes, patches and commits — for the Diff view."
    }

    fn build(self: Box<Self>, registrar: &mut Registrar<'_>) {
        registrar.methods(protocol::set(), Method::read, |caller: Caller, method| async move {
            answered(&caller, method).await
        });
    }
}

/// The kind of the directory a run of git is recorded under.
const RECORD_KIND: &str = "siphon.git";

fn failed(error: impl std::fmt::Display) -> Error {
    refusal("failed", &error.to_string())
}

fn told(error: &GitError) -> Error {
    match error {
        GitError::NoRepository(_) => refusal("no_repository", &error.to_string()),
        _ => failed(error),
    }
}

/// Does the git method `method` for the client of `caller`, its runs recorded while they run.
async fn answered(caller: &Caller, method: Method) -> Result<serde_json::Value, Error> {
    let machine = caller.person_machine().map_err(failed)?;
    let volume = caller.volume();
    let time = volume.now_ms();
    volume
        .recorded(RECORD_KIND, |record| async move {
            let git = Git {
                machine: &machine,
                under: &record,
                time,
                cancelled: CancellationToken::new(),
            };
            told_by(&git, method).await
        })
        .await
        .map_err(failed)?
}

fn json(value: &impl Serialize) -> Result<serde_json::Value, Error> {
    serde_json::to_value(value).map_err(failed)
}

async fn told_by(git: &Git<'_>, method: Method) -> Result<serde_json::Value, Error> {
    match method {
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
