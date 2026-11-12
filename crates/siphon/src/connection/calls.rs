//! Each method a call names, done on Siphon for the person whose session made it: its result as JSON, or why it was
//! refused.

use std::path::Path;

use crate::protocol::siphon::{Created, Delivered, Method, ThreadRead, ThreadsListed, Withdrawn};
use crate::protocol::{Error, Nothing};
use crate::threads::inbox::Joins;
use crate::threads::{NewThread, PERSON, log};
use crate::tools::projects::Place;
use serde::Serialize;

use super::refusal;
use crate::Siphon;
use crate::extension::Caller;

/// Does `method` for the client of `caller`, by the group it is of.
pub(crate) async fn answered(caller: Caller, method: Method) -> Result<serde_json::Value, Error> {
    let Caller { siphon, session } = &caller;
    match method {
        Method::ListFolder(asked) => json(&super::machine::list(siphon, &asked)?),
        Method::ReadFile(asked) => json(&super::machine::read(siphon, &asked)?),
        Method::SearchFiles(asked) => json(&super::machine::search(siphon, &asked)?),
        method @ (Method::ListProviders(_)
        | Method::StatusOfProvider(_)
        | Method::SignIn(_)
        | Method::FinishSignIn(_)
        | Method::SetCredential(_)
        | Method::RemoveCredential(_)
        | Method::AddAccount(_)
        | Method::RenameAccount(_)
        | Method::RemoveAccount(_)
        | Method::ListModels(_)
        | Method::RefreshModels(_)) => super::providers::answered(siphon, method).await,
        Method::ListGrants(Nothing {}) => {
            let snapshot = siphon.runtime.snapshot().map_err(|error| refusal("failed", &error.to_string()))?;
            let projects = crate::tools::projects::all_grants(&snapshot).map_err(|error| refusal("failed", &error.to_string()))?;
            json(&crate::protocol::siphon::GrantsListed { projects })
        }
        Method::RevokeGrants(less) => {
            let snapshot = siphon.runtime.snapshot().map_err(|error| refusal("failed", &error.to_string()))?;
            let changes = crate::tools::projects::revoke(&snapshot, &less.folder, &less.grants)
                .map_err(|error| refusal("failed", &error.to_string()))?;
            siphon
                .runtime
                .commit(changes)
                .await
                .map_err(|error| refusal("failed", &error.to_string()))?;
            json(&Nothing {})
        }
        Method::SetSettings(change) => {
            crate::settings::set(&siphon.runtime, session, &change)
                .await
                .map_err(|error| refusal("bad_settings", &error.to_string()))?;
            json(&Nothing {})
        }
        method => threads_answered(siphon, method).await,
    }
}

/// Does the threads' method `method` on `siphon`.
async fn threads_answered(siphon: &Siphon, method: Method) -> Result<serde_json::Value, Error> {
    let threads = &siphon.threads;
    match method {
        Method::ListThreads(Nothing {}) => json(&ThreadsListed {
            threads: threads.list().map_err(|error| failed(&error))?,
        }),
        Method::CreateThread(create) => {
            let project = create
                .config
                .workspace
                .as_deref()
                .map(|workspace| Place::of(Path::new(workspace)).project);
            let new = NewThread {
                actor: PERSON.to_owned(),
                config: create.config,
                project: project.map(|project| project.to_string_lossy().into_owned()),
                parent: None,
            };
            json(&Created {
                thread: threads.create(&new, &create.key).await.map_err(|error| failed(&error))?,
            })
        }
        Method::ConfigureThread(configure) => {
            threads
                .configure(&configure.thread, PERSON, configure.change)
                .await
                .map_err(|error| failed(&error))?;
            json(&Nothing {})
        }
        Method::MarkThread(mark) => {
            threads.mark(&mark.thread, &mark.change).await.map_err(|error| failed(&error))?;
            json(&Nothing {})
        }
        Method::Deliver(deliver) => {
            let delivered = match deliver.joins {
                Joins::Step => threads.deliver(&deliver.thread, PERSON, deliver.content, &deliver.key).await,
                Joins::Turn => threads.queue(&deliver.thread, PERSON, deliver.content, &deliver.key).await,
            };
            json(&Delivered {
                delivery: delivered.map_err(|error| failed(&error))?,
            })
        }
        Method::Withdraw(withdraw) => json(&Withdrawn {
            withdrawn: threads
                .withdraw(&withdraw.thread, &withdraw.delivery)
                .await
                .map_err(|error| failed(&error))?,
        }),
        Method::StopThread(of) => {
            threads.stop(&of.thread, PERSON).await.map_err(|error| failed(&error))?;
            json(&Nothing {})
        }
        Method::DeleteThread(of) => {
            siphon.delete_thread(&of.thread).await.map_err(|error| failed(&error))?;
            json(&Nothing {})
        }
        Method::AnswerThread(answer) => {
            threads
                .answer(&answer.thread, &answer.key, &answer.answer)
                .await
                .map_err(|error| failed(&error))?;
            json(&Nothing {})
        }
        Method::ReadThread(read) => json(&read_thread(siphon, &read.thread, read.before.as_deref(), read.limit)?),
        Method::ThreadContext(of) => {
            let snapshot = siphon.runtime.snapshot().map_err(|error| refusal("failed", &error.to_string()))?;
            threads.listed(&of.thread).map_err(|error| failed(&error))?;
            let window = crate::threads::Window::read(&snapshot, &of.thread).map_err(|error| refusal("failed", &error.to_string()))?;
            json(&crate::agent::context::composition(&window))
        }
        Method::CompactThread(of) => {
            threads.listed(&of.thread).map_err(|error| failed(&error))?;
            let snapshot = siphon.runtime.snapshot().map_err(|error| refusal("failed", &error.to_string()))?;
            let mut changes = siphon_runtime::Changes::new();
            crate::agent::compaction::start(&snapshot, &mut changes, &of.thread).map_err(|error| refusal("failed", &error.to_string()))?;
            commit(siphon, changes).await?;
            json(&Nothing {})
        }
        Method::ResumeThread(resume) => {
            resume_thread(siphon, &resume.thread, resume.at).await?;
            json(&Nothing {})
        }
        _ => Err(refusal("bad_call", "no method of the threads'")),
    }
}

/// A page of the thread `thread`'s entries, the latest `limit` before the entry `before`, and where its log ends.
fn read_thread(siphon: &Siphon, thread: &str, before: Option<&str>, limit: u32) -> Result<ThreadRead, Error> {
    let snapshot = siphon.runtime.snapshot().map_err(|error| refusal("failed", &error.to_string()))?;
    siphon.threads.listed(thread).map_err(|error| failed(&error))?;
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let (lines, complete) = log::read_before(&snapshot, thread, before, limit).map_err(|error| refusal("failed", &error.to_string()))?;
    let entries = lines
        .iter()
        .map(|line| log::entry_of(line))
        .collect::<Result<_, _>>()
        .map_err(|error| refusal("failed", &error.to_string()))?;
    let end = log::end(&snapshot, thread).map_err(|error| refusal("failed", &error.to_string()))?;
    Ok(ThreadRead { entries, complete, end })
}

/// Has the thread `thread` go on at `at`, unless the person goes on first — the time it went on at before let go of —
/// or at no time, where `at` is none.
async fn resume_thread(siphon: &Siphon, thread: &str, at: Option<i64>) -> Result<(), Error> {
    siphon.threads.listed(thread).map_err(|error| failed(&error))?;
    let mut changes = siphon_runtime::Changes::new();
    crate::agent::resume::disarm(&mut changes, thread);
    commit(siphon, changes).await?;
    let Some(at) = at else { return Ok(()) };
    let snapshot = siphon.runtime.snapshot().map_err(|error| refusal("failed", &error.to_string()))?;
    let input = crate::agent::ResumeInput::now(&snapshot, thread, at, PERSON).map_err(|error| refusal("failed", &error.to_string()))?;
    let mut changes = siphon_runtime::Changes::new();
    crate::agent::resume::arm(&snapshot, &mut changes, &input).map_err(|error| refusal("failed", &error.to_string()))?;
    commit(siphon, changes).await
}

/// Commits `changes` to Siphon's volume.
async fn commit(siphon: &Siphon, changes: siphon_runtime::Changes) -> Result<(), Error> {
    siphon
        .runtime
        .commit(changes)
        .await
        .map(|_| ())
        .map_err(|error| refusal("failed", &error.to_string()))
}

/// Why an operation on threads was not done, as a client is told.
fn failed(error: &crate::threads::Error) -> Error {
    let code = match error {
        crate::threads::Error::NoSuchThread => "no_such_thread",
        crate::threads::Error::NotGiven(_) => "not_given",
        _ => "failed",
    };
    refusal(code, &error.to_string())
}

fn json(result: &impl Serialize) -> Result<serde_json::Value, Error> {
    serde_json::to_value(result).map_err(|error| refusal("failed", &error.to_string()))
}
