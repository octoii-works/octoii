//! Octoii's topics send a committed view first, then changes and live events while the subscription is held.

use std::collections::{BTreeSet, HashMap};

use crate::protocol::octoii::{OctoiiEvent, OctoiiTopic, Settings};
use crate::threads::{self, Listed, Status, log};
use bytes::Bytes;
use octoii_siphon_lib::{CommitEvent, StreamEvent};
use serde_json::json;

use super::{
    calls::history,
    journal::{Followed, Since},
};
use crate::Octoii;
use crate::extension::{Full, Lagged, Subscription};

/// About how many bytes of a thread's log one event of entries carries.
const ENTRIES_BYTES: u64 = 256 << 10;

/// Tells `topic` to `subscription`'s client till it is let go.
pub(crate) async fn told(subscription: Subscription, topic: OctoiiTopic) -> Result<(), Lagged> {
    match topic {
        OctoiiTopic::Thread { thread, limit } => thread_told(&subscription, &thread, limit)
            .await
            .map_err(|Full| Lagged(json!({ "type": "thread", "thread": thread, "limit": limit }))),
        OctoiiTopic::Threads => threads_told(&subscription)
            .await
            .map_err(|Full| Lagged(json!({ "type": "threads" }))),
        OctoiiTopic::Settings => settings_told(&subscription)
            .await
            .map_err(|Full| Lagged(json!({ "type": "settings" }))),
    }
}

/// Reads each durable update from a single snapshot. A bounded replacement view covers large backlogs.
async fn thread_told(subscription: &Subscription, thread: &str, limit: u32) -> Result<(), Full> {
    let (siphon, sink) = (&subscription.caller.siphon, &subscription.sink);
    let Ok(mut committed) = siphon.runtime.commits() else {
        return Ok(());
    };
    let Ok(mut streams) = siphon.runtime.follow(thread) else {
        return Ok(());
    };
    let mut told: Option<Status> = None;
    let mut cursor = None;
    loop {
        let Ok(snapshot) = siphon.runtime.snapshot() else { return Ok(()) };
        let listed = match siphon.threads.listed_at(&snapshot, thread) {
            Ok(listed) => listed,
            Err(threads::Error::NoSuchThread) => {
                return sink.offer(&OctoiiEvent::ThreadGone {
                    subscription: sink.id().to_owned(),
                    thread: thread.to_owned(),
                });
            }
            Err(_) => return Ok(()),
        };
        let (Ok(sequence), Ok(end)) = (snapshot.sequence(), log::end(&snapshot, thread)) else {
            return Ok(());
        };
        let read = if told.is_some() && cursor != end {
            let Ok(read) = log::read_after(&snapshot, thread, cursor.as_ref(), ENTRIES_BYTES) else {
                return Ok(());
            };
            Some(read)
        } else {
            None
        };
        if told.is_none() || read.as_ref().is_some_and(|read| read.more || read.cursor != end) {
            let Ok(history) = history(&snapshot, thread, None, limit) else {
                return Ok(());
            };
            sink.offer(&OctoiiEvent::ThreadSnapshot {
                subscription: sink.id().to_owned(),
                sequence: sequence.to_string(),
                history,
                status: Box::new(listed.status.clone()),
            })?;
        } else if read.is_some() || told.as_ref() != Some(&listed.status) {
            let entries = match read {
                Some(read) => {
                    let Ok(entries) = read.lines.iter().map(|line| log::entry_of(line)).collect::<Result<_, _>>() else {
                        return Ok(());
                    };
                    entries
                }
                None => Vec::new(),
            };
            sink.offer(&OctoiiEvent::ThreadUpdate {
                subscription: sink.id().to_owned(),
                sequence: sequence.to_string(),
                entries,
                status: Box::new(listed.status.clone()),
            })?;
        }
        cursor = end;
        told = Some(listed.status);
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
                        StreamEvent::Reset => OctoiiEvent::StreamsReset { subscription },
                        StreamEvent::Closed(id) => OctoiiEvent::StreamClosed { subscription, request: id.to_token() },
                        StreamEvent::Data(part) => {
                            let Ok(data) = serde_json::from_slice::<Box<serde_json::value::RawValue>>(&part.data) else { continue };
                            OctoiiEvent::Stream {
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

/// The threads as they stand first, then the entries that changed on each visible head.
async fn threads_told(subscription: &Subscription) -> Result<(), Full> {
    let (siphon, sink) = (&subscription.caller.siphon, &subscription.sink);
    let Ok(mut committed) = siphon.runtime.commits() else {
        return Ok(());
    };
    let Ok(all) = siphon.threads.list() else { return Ok(()) };
    let mut shown: HashMap<String, Listed> = all.iter().map(|listed| (listed.id.clone(), listed.clone())).collect();
    sink.offer(&OctoiiEvent::Threads {
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
                Some(listed) if shown.get(id) != Some(listed) => sink.offer(&OctoiiEvent::Thread {
                    subscription: sink.id().to_owned(),
                    thread: Box::new(listed.clone()),
                })?,
                None => sink.offer(&OctoiiEvent::ThreadGone {
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
            sink.offer(&OctoiiEvent::Settings {
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
