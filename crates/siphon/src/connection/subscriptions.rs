//! A subscription: a task that tells its client what its topic holds, then each change of it as it comes — a thread's
//! entries from where the client stood, and its status and what streams of its requests; the threads of the list; the
//! settings — till the client lets it go, its connection ends, or it falls behind: then it is told it lagged, and where
//! it stood.

use std::collections::HashMap;

use siphon_protocol::{Event, Scope, Settings, Topic};
use siphon_threads::{Listed, Status, log};
use tokio::sync::mpsc::{self, error::TrySendError};
use tokio_util::sync::CancellationToken;

use super::{Outgoing, text};
use crate::Siphon;

/// About how many bytes of a thread's log one event of entries carries.
const ENTRIES_BYTES: u64 = 256 << 10;

/// The scope a session needs to subscribe to `topic`.
pub(super) fn scope_of(topic: &Topic) -> Scope {
    match topic {
        Topic::Thread { .. } | Topic::Threads | Topic::Settings => Scope::ThreadsRead,
    }
}

/// A subscription's task: Siphon, the session it is of, its id, the client's queue, and what ends it.
pub(super) struct Task {
    pub(super) siphon: Siphon,
    pub(super) session: String,
    pub(super) id: String,
    pub(super) outgoing: mpsc::Sender<Outgoing>,
    pub(super) stop: CancellationToken,
}

/// Why a subscription ended before it was let go: its client fell behind, or is gone.
enum Ended {
    Lagged,
    Gone,
}

impl Task {
    /// Tells the client what `topic` holds and how it changes, till the subscription ends.
    pub(super) async fn run(self, topic: Topic) {
        let mut cursor = None;
        let ended = match topic {
            Topic::Thread { thread, from } => {
                cursor = from;
                self.thread(&thread, &mut cursor).await
            }
            Topic::Threads => self.threads().await,
            Topic::Settings => self.settings().await,
        };
        if let Err(Ended::Lagged) = ended {
            let lagged = Event::Lagged {
                subscription: self.id.clone(),
                cursor,
            };
            let _ = self.outgoing.send(text(&lagged)).await;
        }
    }

    /// Sends `event` where the client's queue has room; where it has none, the subscription lags.
    fn offer(&self, event: &Event) -> Result<(), Ended> {
        match self.outgoing.try_send(text(event)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(Ended::Lagged),
            Err(TrySendError::Closed(_)) => Err(Ended::Gone),
        }
    }

    /// The thread `thread`: its entries past `cursor`, kept where the client stands, and its status as it changes,
    /// after each commit; what streams of its requests as it comes.
    async fn thread(&self, thread: &str, cursor: &mut Option<log::Cursor>) -> Result<(), Ended> {
        let runtime = &self.siphon.runtime;
        let mut committed = runtime.committed();
        let mut streams = runtime.follow(thread).await;
        let mut told: Option<Status> = None;
        loop {
            loop {
                let Ok(listed) = self.siphon.threads.listed(thread) else {
                    let gone = Event::ThreadGone {
                        subscription: self.id.clone(),
                        thread: thread.to_owned(),
                    };
                    return self.offer(&gone);
                };
                let more = self.entries(thread, cursor)?;
                if told.as_ref() != Some(&listed.status) {
                    self.offer(&Event::Status {
                        subscription: self.id.clone(),
                        status: listed.status.clone(),
                    })?;
                    told = Some(listed.status);
                }
                if !more {
                    break;
                }
            }
            tokio::select! {
                () = self.stop.cancelled() => return Ok(()),
                changed = committed.changed() => changed.map_err(|_| Ended::Gone)?,
                published = streams.next() => {
                    let published = published.ok_or(Ended::Gone)?;
                    self.offer(&Event::Stream {
                        subscription: self.id.clone(),
                        request: published.key.to_string(),
                        offset: published.offset,
                        data: serde_json::from_slice(&published.data).unwrap_or_default(),
                    })?;
                }
            }
        }
    }

    /// The entries of the thread `thread` past `cursor`, sent, `cursor` moved past them once they are: whether more
    /// are left.
    fn entries(&self, thread: &str, cursor: &mut Option<log::Cursor>) -> Result<bool, Ended> {
        let Ok(snapshot) = self.siphon.runtime.snapshot() else {
            return Err(Ended::Gone);
        };
        let Ok(read) = log::read_after(&snapshot, thread, cursor.as_ref(), ENTRIES_BYTES) else {
            return Err(Ended::Gone);
        };
        let entries: Vec<_> = read.lines.iter().filter_map(|line| log::entry_of(line).ok()).collect();
        if let (false, Some(after)) = (entries.is_empty(), &read.cursor) {
            self.offer(&Event::Entries {
                subscription: self.id.clone(),
                entries,
                cursor: after.clone(),
            })?;
        }
        if read.cursor.is_some() {
            *cursor = read.cursor;
        }
        Ok(read.more)
    }

    /// The threads: each as it changed, and each gone, after each commit.
    async fn threads(&self) -> Result<(), Ended> {
        let mut committed = self.siphon.runtime.committed();
        let mut shown: HashMap<String, Listed> = HashMap::new();
        loop {
            let listed = self.siphon.threads.list().map_err(|_| Ended::Gone)?;
            let mut now = HashMap::with_capacity(listed.len());
            for thread in listed {
                if shown.get(&thread.id) != Some(&thread) {
                    self.offer(&Event::Thread {
                        subscription: self.id.clone(),
                        thread: Box::new(thread.clone()),
                    })?;
                }
                now.insert(thread.id.clone(), thread);
            }
            for gone in shown.keys().filter(|id| !now.contains_key(*id)) {
                self.offer(&Event::ThreadGone {
                    subscription: self.id.clone(),
                    thread: gone.clone(),
                })?;
            }
            shown = now;
            tokio::select! {
                () = self.stop.cancelled() => return Ok(()),
                changed = committed.changed() => changed.map_err(|_| Ended::Gone)?,
            }
        }
    }

    /// The settings — the environment's, and the session's device's — as they change, after each commit.
    async fn settings(&self) -> Result<(), Ended> {
        let mut committed = self.siphon.runtime.committed();
        let mut told = None;
        loop {
            let snapshot = self.siphon.runtime.snapshot().map_err(|_| Ended::Gone)?;
            let environment = siphon_settings::environment(&snapshot).map_err(|_| Ended::Gone)?;
            let device = siphon_settings::device(&snapshot, &self.session).map_err(|_| Ended::Gone)?;
            let settings = Settings { environment, device };
            if told.as_ref() != Some(&settings) {
                self.offer(&Event::Settings {
                    subscription: self.id.clone(),
                    settings: settings.clone(),
                })?;
                told = Some(settings);
            }
            tokio::select! {
                () = self.stop.cancelled() => return Ok(()),
                changed = committed.changed() => changed.map_err(|_| Ended::Gone)?,
            }
        }
    }
}
