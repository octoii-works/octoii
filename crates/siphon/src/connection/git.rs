//! The machine's repositories as the person reads them, by git run on the machine as the person reads it: each run
//! recorded in the volume as every program is, under a directory of its own that goes as it ends.

use std::path::Path;

use bytes::Bytes;
use serde::Serialize;
use siphon_machine::git::{Git, GitError};
use siphon_protocol::Error;
use siphon_protocol::siphon::{Commits, InRepository, Method, Patch, Versions};
use siphon_runtime::Changes;
use tokio_util::sync::CancellationToken;

use super::refusal;
use crate::Siphon;

/// The kind of the directory a run of git is recorded under.
const RECORD_KIND: &[u8] = b"siphon.git";

fn failed(error: impl std::fmt::Display) -> Error {
    refusal("failed", &error.to_string())
}

fn told(error: &GitError) -> Error {
    match error {
        GitError::NoRepository(_) => refusal("no_repository", &error.to_string()),
        _ => failed(error),
    }
}

/// Does the git method `method` on `siphon`.
pub(super) async fn answered(siphon: &Siphon, method: Method) -> Result<serde_json::Value, Error> {
    let machine = siphon.person_machine().map_err(failed)?;
    let runtime = &siphon.runtime;
    let record = beak::layout::child(b"", RECORD_KIND, runtime.new_id().map_err(failed)?.as_bytes());
    let mut changes = Changes::new();
    changes.mkdir(record.clone());
    runtime.commit(changes).await.map_err(failed)?;
    let git = Git {
        machine: &machine,
        under: &record,
        time: runtime.now_ms(),
        cancelled: CancellationToken::new(),
    };
    let answer = told_by(&git, method).await;
    let mut changes = Changes::new();
    changes.force_remove(Bytes::clone(&record), None);
    let _ = runtime.commit(changes).await;
    answer
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
        _ => Err(refusal("bad_call", "no method of git's")),
    }
}
