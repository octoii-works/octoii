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
                            handler.handle(&payload, process.clone(), key, cancelled, Runtime { shared: Arc::clone(&shared) })
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
    shared.streams.end(&String::from_utf8_lossy(&key));
    let mut changes = Changes::new();
    let mail = Mail {
        key: key.clone(),
        name: Bytes::from_static(name.as_bytes()),
        payload,
    };
    changes.answer(process.clone(), mail, Some(started));
    if let Err(error) = shared.commit(changes).await {
        // Refused where it was answered already, withdrawn, or gone with its process.
        if error.status == beak::Status::Refused {
            tracing::debug!(%error, "a request's answer was refused");
        } else {
            tracing::warn!(%error, "a request went unanswered");
        }
    }
    Done::Answered(process, key)
}
