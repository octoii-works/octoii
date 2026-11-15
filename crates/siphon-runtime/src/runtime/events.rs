//! The kernel's events, each handled as it comes.

use std::collections::HashMap;
use std::sync::Arc;

use beak::{Event, Events};
use bytes::Bytes;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::builder::Answering;
use super::{Collection, Runtime, Shared};
use crate::changes::{Changes, Mail};
use crate::request::INTERRUPTED;

/// What a task started for an event came to.
enum Done {
    /// The request of the process and key was answered, or left.
    Answered(Bytes, Bytes),
    /// The volume was collected: what it keeps now; or why not.
    Collected(beak::Result<u64>),
}

/// Handles the kernel's events till the runtime stops: tells whoever follows the volume of each commit, wakes the
/// dispatcher, weighs the volume for collection, starts a task for each request a handler answers and cuts off those
/// Beak interrupts. As it stops, the requests being answered are dropped unanswered.
pub(super) async fn handle_events(shared: Arc<Shared>, mut events: Events, collection: Collection, mut base: u64) {
    let mut tasks: JoinSet<Done> = JoinSet::new();
    let mut asked: HashMap<(Bytes, Bytes), CancellationToken> = HashMap::new();
    let mut collecting = false;
    loop {
        tokio::select! {
            () = shared.shutdown.cancelled() => break,
            event = events.next() => {
                let Some(event) = event else { break };
                match event {
                    Event::Committed(seq) => {
                        shared.committed.send_replace(seq);
                        shared.dispatch.notify_one();
                        if !collecting && collection.is_due(base, shared.kernel.usage().unwrap_or(0)) {
                            collecting = true;
                            let kernel = shared.kernel.clone();
                            tasks.spawn_blocking(move || Done::Collected(kernel.collect().and_then(|_| kernel.usage())));
                        }
                    }
                    Event::Request { process, key, name, payload, started, retold } => {
                        let Some(handler) = shared.handlers.get(&name) else { continue };
                        let answering = if retold {
                            interrupted()
                        } else {
                            let cancelled = CancellationToken::new();
                            asked.insert((process.clone(), key.clone()), cancelled.clone());
                            let key = String::from_utf8_lossy(&key).into_owned();
                            Arc::clone(handler).handle(payload, process.clone(), key, cancelled, Runtime { shared: Arc::clone(&shared) })
                        };
                        tasks.spawn(answer(Arc::clone(&shared), process, key, started, answering));
                    }
                    Event::Interrupted { process, key } => {
                        if let Some(cancelled) = asked.get(&(process, key)) {
                            cancelled.cancel();
                        }
                    }
                    Event::Limit { .. } | Event::Signalled { .. } => {}
                }
            }
            Some(done) = tasks.join_next(), if !tasks.is_empty() => match done {
                Ok(Done::Answered(process, key)) => {
                    asked.remove(&(process, key));
                }
                Ok(Done::Collected(after)) => {
                    collecting = false;
                    match after {
                        Ok(after) => base = after,
                        Err(error) => tracing::warn!(%error, "the volume could not be collected"),
                    }
                }
                Err(error) => tracing::error!(%error, "a task of the runtime's failed"),
            },
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
}

/// The answer to a request retold, made before the volume opened: interrupted, its handler never asked, for the process
/// that asked to decide — the host that ended may have acted on it, and nothing is acted on twice.
fn interrupted() -> Answering {
    Box::pin(std::future::ready((INTERRUPTED, Bytes::new())))
}

/// Answers a request with what `answering` gives, committed — durable as it returns — once its stream ends.
async fn answer(shared: Arc<Shared>, process: Bytes, key: Bytes, started: u64, answering: Answering) -> Done {
    let (name, payload) = answering.await;
    let mut changes = Changes::new();
    let mail = Mail {
        key: key.clone(),
        name: Bytes::from_static(name.as_bytes()),
        payload,
    };
    changes.answer(process.clone(), mail, Some(started));
    if let Err(error) = commit_answer(&shared.shutdown, || shared.commit(changes.clone())).await {
        tracing::error!(%error, "the runtime stopped: a request's answer could not be committed");
        shared.shutdown.cancel();
    }
    shared.streams.end(&String::from_utf8_lossy(&key));
    Done::Answered(process, key)
}

/// Retry only the commit, retaining the handler's result until it is accepted or the request is gone.
async fn commit_answer<F, A>(shutdown: &CancellationToken, mut commit: F) -> beak::Result<()>
where
    F: FnMut() -> A,
    A: std::future::Future<Output = beak::Result<u64>>,
{
    let mut delay = std::time::Duration::from_millis(50);
    loop {
        if shutdown.is_cancelled() {
            return Ok(());
        }
        match commit().await {
            Ok(_) => return Ok(()),
            Err(error) if error.status == beak::Status::Refused => return Ok(()),
            Err(error) if error.status == beak::Status::Failed => {
                tracing::warn!(%error, "retrying a request's answer commit");
            }
            Err(error) => return Err(error),
        }
        tokio::select! {
            () = shutdown.cancelled() => return Ok(()),
            () = tokio::time::sleep(delay) => {}
        }
        delay = (delay * 2).min(std::time::Duration::from_secs(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(status: beak::Status) -> beak::Error {
        beak::Error {
            status,
            reason: "injected failure".to_owned(),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_answer_commit_is_retried_until_accepted() {
        let mut attempts = 0;
        commit_answer(&CancellationToken::new(), || {
            attempts += 1;
            std::future::ready(if attempts < 3 { Err(failure(beak::Status::Failed)) } else { Ok(1) })
        })
        .await
        .unwrap();
        assert_eq!(attempts, 3);
    }

    #[tokio::test(start_paused = true)]
    async fn a_gone_request_ends_retries_and_an_invalid_commit_fails() {
        for status in [beak::Status::Refused, beak::Status::Invalid] {
            let mut attempts = 0;
            let result = commit_answer(&CancellationToken::new(), || {
                attempts += 1;
                std::future::ready(Err(failure(status)))
            })
            .await;
            assert_eq!(attempts, 1);
            assert_eq!(result.is_ok(), status == beak::Status::Refused);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_interrupts_answer_commit_retries() {
        let shutdown = CancellationToken::new();
        let mut attempts = 0;
        commit_answer(&shutdown, || {
            attempts += 1;
            shutdown.cancel();
            std::future::ready(Err(failure(beak::Status::Failed)))
        })
        .await
        .unwrap();
        assert_eq!(attempts, 1);
    }
}
