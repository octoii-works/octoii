//! Siphon's own topics, as a subscription tells them: a thread's entries from where the client stood, its status and
//! what streams of its requests; the threads of the list; the settings — each what it holds first, then each change
//! as it comes, till the subscription is let go, or it falls behind: then the topic to subscribe to again from where it
//! stood.

use std::collections::{BTreeSet, HashMap};

use bytes::Bytes;
use serde_json::json;
use siphon_protocol::siphon::{Settings, SiphonEvent, SiphonTopic};
use siphon_threads::{Listed, Status, log, thread};

use super::journal::{Followed, Since, is_at_or_under};
use crate::Siphon;
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
/// each commit that changed anything of it; what streams of its requests as it comes. A thread gone, or Siphon
/// closing, ends it.
async fn thread_told(subscription: &Subscription, thread: &str, cursor: &mut Option<log::Cursor>) -> Result<(), Full> {
    let (siphon, sink) = (&subscription.caller.siphon, &subscription.sink);
    let mut committed = siphon.runtime.committed();
    let mut streams = siphon.runtime.follow(thread).await;
    let Ok(mut followed) = siphon.runtime.snapshot().map(|snapshot| Followed::at(&snapshot)) else {
        return Ok(());
    };
    let own = thread::path(thread);
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
        loop {
            tokio::select! {
                () = subscription.stop.cancelled() => return Ok(()),
                changed = committed.changed() => {
                    if changed.is_err() {
                        return Ok(());
                    }
                    if touched(siphon, &mut followed, |path| is_at_or_under(path, &own)) {
                        break;
                    }
                }
                published = streams.next() => {
                    let Some(published) = published else { return Ok(()) };
                    // What streamed goes on as it was written, never read again.
                    let Ok(data) = serde_json::value::RawValue::from_string(String::from_utf8_lossy(&published.data).into_owned()) else {
                        continue;
                    };
                    sink.offer(&SiphonEvent::Stream {
                        subscription: sink.id().to_owned(),
                        request: published.key.to_string(),
                        offset: published.offset,
                        data,
                    })?;
                }
            }
        }
    }
}

/// Whether the commits since `followed` stood changed a path `of` takes — or may have, where the journal no longer
/// holds them all, or cannot be read.
fn touched(siphon: &Siphon, followed: &mut Followed, of: impl Fn(&[u8]) -> bool) -> bool {
    let Ok(snapshot) = siphon.runtime.snapshot() else { return true };
    match followed.since(&snapshot) {
        Ok(Since::Paths(paths)) => paths.iter().any(|path| of(path)),
        Ok(Since::Missed) | Err(_) => true,
    }
}

/// The entries of the thread `thread` past `cursor`, told, `cursor` moved past them once they are: whether more are
/// left; none where the volume cannot be read.
fn entries_told(subscription: &Subscription, thread: &str, cursor: &mut Option<log::Cursor>) -> Result<Option<bool>, Full> {
    let (siphon, sink) = (&subscription.caller.siphon, &subscription.sink);
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

/// The threads: all of them as they stand first; then, after each commit, each it changed as it now stands, and each
/// it deleted gone — all of them read again where the journal no longer holds every commit since.
async fn threads_told(subscription: &Subscription) -> Result<(), Full> {
    let (siphon, sink) = (&subscription.caller.siphon, &subscription.sink);
    let mut committed = siphon.runtime.committed();
    let Ok(mut followed) = siphon.runtime.snapshot().map(|snapshot| Followed::at(&snapshot)) else {
        return Ok(());
    };
    let Ok(all) = siphon.threads.list() else { return Ok(()) };
    let mut shown: HashMap<String, Listed> = all.iter().map(|listed| (listed.id.clone(), listed.clone())).collect();
    sink.offer(&SiphonEvent::Threads {
        subscription: sink.id().to_owned(),
        threads: all,
    })?;
    loop {
        let ids = loop {
            tokio::select! {
                () = subscription.stop.cancelled() => return Ok(()),
                changed = committed.changed() => if changed.is_err() { return Ok(()) },
            }
            let Ok(snapshot) = siphon.runtime.snapshot() else { return Ok(()) };
            match followed.since(&snapshot) {
                Ok(Since::Paths(paths)) => {
                    let changed: BTreeSet<String> = paths.iter().filter_map(|path| thread::id_of(path)).map(str::to_owned).collect();
                    if !changed.is_empty() {
                        break changed;
                    }
                }
                Ok(Since::Missed) | Err(_) => {
                    let Ok(all) = siphon.threads.list() else { return Ok(()) };
                    break all.iter().map(|listed| listed.id.clone()).chain(shown.keys().cloned()).collect();
                }
            }
        };
        threads_changed(sink, &mut shown, |id| siphon.threads.listed(id).ok(), &ids)?;
    }
}

/// Each of `ids` told as it now stands where `shown` shows it otherwise, or gone where it is gone; `shown` kept as told.
fn threads_changed(
    sink: &crate::plugin::Sink,
    shown: &mut HashMap<String, Listed>,
    listed: impl Fn(&str) -> Option<Listed>,
    ids: &BTreeSet<String>,
) -> Result<(), Full> {
    for id in ids {
        match listed(id) {
            Some(listed) if shown.get(id) != Some(&listed) => {
                sink.offer(&SiphonEvent::Thread {
                    subscription: sink.id().to_owned(),
                    thread: Box::new(listed.clone()),
                })?;
                shown.insert(id.clone(), listed);
            }
            None if shown.remove(id).is_some() => {
                sink.offer(&SiphonEvent::ThreadGone {
                    subscription: sink.id().to_owned(),
                    thread: id.clone(),
                })?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// The settings — the environment's, and the session's device's — as they stand first, then after each commit that
/// changed either.
async fn settings_told(subscription: &Subscription) -> Result<(), Full> {
    let (context, sink) = (&subscription.caller, &subscription.sink);
    let mut committed = context.siphon.runtime.committed();
    let Ok(mut followed) = context.siphon.runtime.snapshot().map(|snapshot| Followed::at(&snapshot)) else {
        return Ok(());
    };
    let Ok(device_file) = siphon_settings::device_file(&context.session) else {
        return Ok(());
    };
    let directory = Bytes::from_static(siphon_settings::DIRECTORY.as_bytes());
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
        loop {
            tokio::select! {
                () = subscription.stop.cancelled() => return Ok(()),
                changed = committed.changed() => if changed.is_err() { return Ok(()) },
            }
            if touched(&context.siphon, &mut followed, |path| {
                *path == directory[..] || *path == device_file[..]
            }) {
                break;
            }
        }
    }
}
