//! Each method a call names, done on Siphon for the person whose session made it: its result as JSON, or why it was
//! refused.

use std::path::{Path, PathBuf};

use serde::Serialize;
use siphon_protocol::siphon::{Created, Delivered, Method, ThreadRead, ThreadsListed, Withdrawn};
use siphon_protocol::{Error, Nothing};
use siphon_threads::{NewThread, log};

use super::refusal;
use crate::Siphon;
use crate::plugin::Context;

/// Who a person's calls are made as, as the threads' logs name them.
const PERSON: &str = "person";

/// Does `method` for the client of `context`.
pub(crate) async fn answered(context: Context, method: Method) -> Result<serde_json::Value, Error> {
    let Context { siphon, session } = &context;
    let threads = &siphon.threads;
    match method {
        Method::ListThreads(Nothing {}) => json(&ThreadsListed {
            threads: threads.list().map_err(|error| failed(&error))?,
        }),
        Method::CreateThread(create) => {
            let project = create.config.workspace.as_deref().map(|workspace| project_of(Path::new(workspace)));
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
        Method::Deliver(deliver) => json(&Delivered {
            delivery: threads
                .deliver(&deliver.thread, PERSON, deliver.content, &deliver.key)
                .await
                .map_err(|error| failed(&error))?,
        }),
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
            threads.delete(&of.thread).await.map_err(|error| failed(&error))?;
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
        Method::SetSettings(change) => {
            siphon_settings::set(&siphon.runtime, session, &change)
                .await
                .map_err(|error| refusal("bad_settings", &error.to_string()))?;
            json(&Nothing {})
        }
    }
}

/// A page of the thread `thread`'s entries, the latest `limit` before the entry `before`.
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
    Ok(ThreadRead { entries, complete })
}

/// The project of a thread working in `workspace`: the repository the workspace is in, by its root, else the
/// workspace itself.
fn project_of(workspace: &Path) -> PathBuf {
    workspace
        .ancestors()
        .find(|folder| folder.join(".git").exists())
        .unwrap_or(workspace)
        .to_path_buf()
}

/// Why an operation on threads was not done, as a client is told.
fn failed(error: &siphon_threads::Error) -> Error {
    let code = match error {
        siphon_threads::Error::NoSuchThread => "no_such_thread",
        siphon_threads::Error::NotGiven(_) => "not_given",
        _ => "failed",
    };
    refusal(code, &error.to_string())
}

fn json(result: &impl Serialize) -> Result<serde_json::Value, Error> {
    serde_json::to_value(result).map_err(|error| refusal("failed", &error.to_string()))
}
