//! Siphon's own topics, as a subscription tells them: a thread's entries from where the client stood, its status and
//! what streams of its requests; the threads of the list; the settings — each what it holds first, then each change
//! as it comes, till the subscription is let go, or it falls behind: then the topic to subscribe to again from where it
//! stood.

use std::collections::HashMap;

use serde_json::json;
use siphon_protocol::siphon::{Settings, SiphonEvent, SiphonTopic};
use siphon_threads::{Listed, Status, log};

use crate::plugin::{Full, Lagged, Subscription};

/// About how many bytes of a thread's log one event of entries carries.
const ENTRIES_BYTES: u64 = 256 << 10;

/// Tells `topic` to `subscription`'s client till it is let go.
pub(crate) async fn told(subscription: Subscription, topic: SiphonTopic) -> Result<(), Lagged> {
    match topic {
        SiphonTopic::Thread { thread, from } => {
            let mut cursor = from;
            let told = thread_told(&subscription, &thread, &mut cursor).await;
            told.map_err(|Full| Lagged(json!({ "type": "thread", "thread": thread, "from": cursor })))
        }
        SiphonTopic::Threads => threads_told(&subscription)
            .await
            .map_err(|Full| Lagged(json!({ "type": "threads" }))),
        SiphonTopic::Settings => settings_told(&subscription)
            .await
            .map_err(|Full| Lagged(json!({ "type": "settings" }))),
    }
}

/// The thread `thread`: its entries past `cursor`, kept where the client stands, and its status as it changes, after
/// each commit; what streams of its requests as it comes. A thread gone, or Siphon closing, ends it.
async fn thread_told(subscription: &Subscription, thread: &str, cursor: &mut Option<log::Cursor>) -> Result<(), Full> {
    let (siphon, sink) = (&subscription.context.siphon, &subscription.sink);
    let mut committed = siphon.runtime.committed();
    let mut streams = siphon.runtime.follow(thread).await;
    let mut told: Option<Status> = None;
    loop {
        loop {
            let Ok(listed) = siphon.threads.listed(thread) else {
                let gone = SiphonEvent::ThreadGone {
                    subscription: sink.id().to_owned(),
                    thread: thread.to_owned(),
                };
                return sink.offer(&gone);
            };
            let Some(more) = entries_told(subscription, thread, cursor)? else {
                return Ok(());
            };
            if told.as_ref() != Some(&listed.status) {
                sink.offer(&SiphonEvent::Status {
                    subscription: sink.id().to_owned(),
                    status: listed.status.clone(),
                })?;
                told = Some(listed.status);
            }
            if !more {
                break;
            }
        }
        tokio::select! {
            () = subscription.stop.cancelled() => return Ok(()),
            changed = committed.changed() => if changed.is_err() { return Ok(()) },
            published = streams.next() => {
                let Some(published) = published else { return Ok(()) };
                sink.offer(&SiphonEvent::Stream {
                    subscription: sink.id().to_owned(),
                    request: published.key.to_string(),
                    offset: published.offset,
                    data: serde_json::from_slice(&published.data).unwrap_or_default(),
                })?;
            }
        }
    }
}

/// The entries of the thread `thread` past `cursor`, told, `cursor` moved past them once they are: whether more are
/// left; none where the volume cannot be read.
fn entries_told(subscription: &Subscription, thread: &str, cursor: &mut Option<log::Cursor>) -> Result<Option<bool>, Full> {
    let (siphon, sink) = (&subscription.context.siphon, &subscription.sink);
    let Ok(snapshot) = siphon.runtime.snapshot() else { return Ok(None) };
    let Ok(read) = log::read_after(&snapshot, thread, cursor.as_ref(), ENTRIES_BYTES) else {
        return Ok(None);
    };
    let entries: Vec<_> = read.lines.iter().filter_map(|line| log::entry_of(line).ok()).collect();
    if let (false, Some(after)) = (entries.is_empty(), &read.cursor) {
        sink.offer(&SiphonEvent::Entries {
            subscription: sink.id().to_owned(),
            entries,
            cursor: after.clone(),
        })?;
    }
    if read.cursor.is_some() {
        *cursor = read.cursor;
    }
    Ok(Some(read.more))
}

/// The threads: each as it changed, and each gone, after each commit.
async fn threads_told(subscription: &Subscription) -> Result<(), Full> {
    let (siphon, sink) = (&subscription.context.siphon, &subscription.sink);
    let mut committed = siphon.runtime.committed();
    let mut shown: HashMap<String, Listed> = HashMap::new();
    loop {
        let Ok(listed) = siphon.threads.list() else { return Ok(()) };
        let mut now = HashMap::with_capacity(listed.len());
        for thread in listed {
            if shown.get(&thread.id) != Some(&thread) {
                sink.offer(&SiphonEvent::Thread {
                    subscription: sink.id().to_owned(),
                    thread: Box::new(thread.clone()),
                })?;
            }
            now.insert(thread.id.clone(), thread);
        }
        for gone in shown.keys().filter(|id| !now.contains_key(*id)) {
            sink.offer(&SiphonEvent::ThreadGone {
                subscription: sink.id().to_owned(),
                thread: gone.clone(),
            })?;
        }
        shown = now;
        tokio::select! {
            () = subscription.stop.cancelled() => return Ok(()),
            changed = committed.changed() => if changed.is_err() { return Ok(()) },
        }
    }
}

/// The settings — the environment's, and the session's device's — as they change, after each commit.
async fn settings_told(subscription: &Subscription) -> Result<(), Full> {
    let (context, sink) = (&subscription.context, &subscription.sink);
    let mut committed = context.siphon.runtime.committed();
    let mut told = None;
    loop {
        let Ok(snapshot) = context.siphon.runtime.snapshot() else {
            return Ok(());
        };
        let (Ok(environment), Ok(device)) = (
            siphon_settings::environment(&snapshot),
            siphon_settings::device(&snapshot, &context.session),
        ) else {
            return Ok(());
        };
        let settings = Settings { environment, device };
        if told.as_ref() != Some(&settings) {
            sink.offer(&SiphonEvent::Settings {
                subscription: sink.id().to_owned(),
                settings: settings.clone(),
            })?;
            told = Some(settings);
        }
        tokio::select! {
            () = subscription.stop.cancelled() => return Ok(()),
            changed = committed.changed() => if changed.is_err() { return Ok(()) },
        }
    }
}
