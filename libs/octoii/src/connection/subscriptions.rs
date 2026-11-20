//! Octoii's own topics, as a subscription tells them: a thread's entries from where the client stood, its status and
//! what streams of its requests; the threads of the list; the settings — each what it holds first, then each change
//! as it comes, till the subscription is let go, or it falls behind: then the topic to subscribe to again from where it
//! stood.

use std::collections::{BTreeSet, HashMap};

use crate::protocol::siphon::{Settings, SiphonEvent, SiphonTopic};
use crate::threads::{Listed, Status, log};
use bytes::Bytes;
use octoii_siphon_lib::{CommitEvent, Snapshot, StreamEvent};
use serde_json::json;

use super::journal::{Followed, Since};
use crate::Octoii;
use crate::extension::{Full, Lagged, Subscription};

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
/// each commit that changed anything of it; what streams of its requests as it comes. A thread gone, or Octoii
/// closing, ends it.
async fn thread_told(subscription: &Subscription, thread: &str, cursor: &mut Option<log::Cursor>) -> Result<(), Full> {
    let (siphon, sink) = (&subscription.caller.siphon, &subscription.sink);
    let Ok(mut committed) = siphon.runtime.commits() else {
        return Ok(());
    };
    let Ok(mut streams) = siphon.runtime.follow(thread) else {
        return Ok(());
    };
    let mut told: Option<Status> = None;
    loop {
        let Ok(snapshot) = siphon.runtime.snapshot() else { return Ok(()) };
        let Ok(listed) = siphon.threads.listed_at(&snapshot, thread) else {
            return sink.offer(&SiphonEvent::ThreadGone {
                subscription: sink.id().to_owned(),
                thread: thread.to_owned(),
            });
        };
        let Some(mut more) = entries_told(sink, &snapshot, thread, cursor)? else {
            return Ok(());
        };
        if told.as_ref() != Some(&listed.status) {
            sink.offer(&SiphonEvent::Status {
                subscription: sink.id().to_owned(),
                status: listed.status.clone(),
            })?;
            told = Some(listed.status);
        }
        while more {
            let Some(next) = entries_told(sink, &snapshot, thread, cursor)? else {
                return Ok(());
            };
            more = next;
        }
        drop(snapshot);
        loop {
            tokio::select! {
                () = subscription.stop.cancelled() => return Ok(()),
                event = committed.next() => match event {
                    Ok(Some(CommitEvent::Head(_))) => break,
                    _ => return Ok(()),
                },
                event = streams.next() => {
                    let Ok(Some(event)) = event else { return Ok(()) };
                    let subscription = sink.id().to_owned();
                    let event = match event {
                        StreamEvent::Reset => SiphonEvent::StreamsReset { subscription },
                        StreamEvent::Closed(id) => SiphonEvent::StreamClosed { subscription, request: id.to_token() },
                        StreamEvent::Data(part) => {
                            let Ok(data) = serde_json::from_slice::<Box<serde_json::value::RawValue>>(&part.data) else { continue };
                            SiphonEvent::Stream {
                                subscription,
                                request: part.id.to_token(),
                                key: String::from_utf8_lossy(part.id.key()).into_owned(),
                                offset: part.offset.to_string(),
                                data,
                            }
                        }
                    };
                    sink.offer(&event)?;
                }
            }
        }
    }
}

/// Whether the commits since `followed` stood changed a path `of` takes — or may have, where the journal no longer
/// holds them all, or cannot be read.
fn touched(siphon: &Octoii, followed: &mut Followed, of: impl Fn(&[u8]) -> bool) -> bool {
    let Ok(snapshot) = siphon.runtime.snapshot() else { return true };
    match followed.since(&snapshot) {
        Ok(Since::Paths(paths)) => paths.iter().any(|path| of(path)),
        Ok(Since::Missed) | Err(_) => true,
    }
}

/// The entries of the thread `thread` past `cursor`, told, `cursor` moved past them once they are: whether more are
/// left; none where the volume cannot be read.
fn entries_told(
    sink: &crate::extension::Sink,
    snapshot: &Snapshot,
    thread: &str,
    cursor: &mut Option<log::Cursor>,
) -> Result<Option<bool>, Full> {
    let Ok(read) = log::read_after(snapshot, thread, cursor.as_ref(), ENTRIES_BYTES) else {
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

/// The threads as they stand first, then the entries that changed on each visible head.
async fn threads_told(subscription: &Subscription) -> Result<(), Full> {
    let (siphon, sink) = (&subscription.caller.siphon, &subscription.sink);
    let Ok(mut committed) = siphon.runtime.commits() else {
        return Ok(());
    };
    let Ok(all) = siphon.threads.list() else { return Ok(()) };
    let mut shown: HashMap<String, Listed> = all.iter().map(|listed| (listed.id.clone(), listed.clone())).collect();
    sink.offer(&SiphonEvent::Threads {
        subscription: sink.id().to_owned(),
        threads: all,
    })?;
    loop {
        tokio::select! {
            () = subscription.stop.cancelled() => return Ok(()),
            event = committed.next() => if !matches!(event, Ok(Some(CommitEvent::Head(_)))) { return Ok(()) },
        }
        // Requests and Fiber state can change without a user-data path in the journal.
        let Ok(all) = siphon.threads.list() else { return Ok(()) };
        let current: HashMap<_, _> = all.into_iter().map(|listed| (listed.id.clone(), listed)).collect();
        for id in shown.keys().chain(current.keys()).collect::<BTreeSet<_>>() {
            match current.get(id) {
                Some(listed) if shown.get(id) != Some(listed) => sink.offer(&SiphonEvent::Thread {
                    subscription: sink.id().to_owned(),
                    thread: Box::new(listed.clone()),
                })?,
                None => sink.offer(&SiphonEvent::ThreadGone {
                    subscription: sink.id().to_owned(),
                    thread: id.clone(),
                })?,
                _ => {}
            }
        }
        shown = current;
    }
}

/// The settings — the environment's, and the session's device's — as they stand first, then after each commit that
/// changed either.
async fn settings_told(subscription: &Subscription) -> Result<(), Full> {
    let (context, sink) = (&subscription.caller, &subscription.sink);
    let Ok(mut committed) = context.siphon.runtime.commits() else {
        return Ok(());
    };
    let Ok(mut followed) = context.siphon.runtime.snapshot().and_then(|snapshot| Followed::at(&snapshot)) else {
        return Ok(());
    };
    let Ok(device_file) = crate::settings::device_file(&context.session) else {
        return Ok(());
    };
    let directory = Bytes::from_static(crate::settings::DIRECTORY.as_bytes());
    let mut told = None;
    loop {
        let Ok(snapshot) = context.siphon.runtime.snapshot() else {
            return Ok(());
        };
        let (Ok(environment), Ok(device)) = (
            crate::settings::environment(&snapshot),
            crate::settings::device(&snapshot, &context.session),
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
        loop {
            tokio::select! {
                () = subscription.stop.cancelled() => return Ok(()),
                event = committed.next() => if !matches!(event, Ok(Some(CommitEvent::Head(_)))) { return Ok(()) },
            }
            if touched(&context.siphon, &mut followed, |path| {
                *path == directory[..] || *path == device_file[..]
            }) {
                break;
            }
        }
    }
}
