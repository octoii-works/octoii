//! The runtime: a kernel of Beak's, the programs its processes run, and the requests they make, each acted on as
//! Beak tells of it. No loop owns it all: Beak's commits are its transactions and its snapshots consistent reads from
//! anywhere, so each part does its own work —
//!
//! - one dispatcher runs the phases due, a batch at a time, off the async threads, woken by every commit and by when
//!   the kernel is next due;
//! - one task handles the kernel's events: each request handled in a task of its own that commits its own answer, cut
//!   off as Beak says, and the volume collected off the async threads as it grows past what it kept after the last;
//! - the streams are kept by their own task;
//! - whoever follows the volume watches the latest commit and reads the journal on from where it left off.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use beak::{Kernel, Snapshot, layout};
use bytes::Bytes;
use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::changes::{Changes, Mail};
use crate::person::{NotGiven, problems};
use crate::program::Runs;
use crate::request::ANSWERED;
use crate::streams::{Following, Streams};

mod builder;
mod dispatcher;
mod events;

pub use builder::Builder;
use builder::{Handles, Schema};

/// The time, in milliseconds since the Unix epoch.
pub trait Clock: Send + Sync + 'static {
    fn now_ms(&self) -> i64;
}

/// The system's clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
    }
}

/// When the volume is collected, as Redis rewrites its append-only file: once what it keeps has grown past what it
/// kept after the last collection by `growth_percent`, and past `min_bytes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Collection {
    pub min_bytes: u64,
    pub growth_percent: u64,
}

impl Default for Collection {
    fn default() -> Self {
        Self {
            min_bytes: 64 << 20,
            growth_percent: 100,
        }
    }
}

impl Collection {
    /// Whether what the volume keeps, `usage`, has grown past where it is collected, from `base`.
    #[must_use]
    pub fn is_due(&self, base: u64, usage: u64) -> bool {
        usage >= self.min_bytes && usage >= base + base * self.growth_percent / 100
    }
}

/// How the runtime opens.
#[derive(Clone)]
pub struct Options {
    /// The directory the volume is kept in; memory, gone with the runtime, where `None`.
    pub directory: Option<PathBuf>,
    pub clock: Arc<dyn Clock>,
    pub collection: Collection,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            directory: None,
            clock: Arc::new(SystemClock),
            collection: Collection::default(),
        }
    }
}

impl std::fmt::Debug for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Options")
            .field("directory", &self.directory)
            .field("collection", &self.collection)
            .finish_non_exhaustive()
    }
}

struct Shared {
    kernel: Kernel,
    clock: Arc<dyn Clock>,
    programs: HashMap<Bytes, Arc<dyn Runs>>,
    handlers: HashMap<Bytes, Arc<dyn Handles>>,
    person: HashMap<Bytes, Schema>,
    streams: Streams,
    /// The latest commit, for whoever follows the volume.
    committed: watch::Sender<u64>,
    /// Wakes the dispatcher.
    dispatch: Notify,
    shutdown: CancellationToken,
}

impl Shared {
    /// Commits `changes` now, off the async threads.
    async fn commit(self: &Arc<Self>, changes: Changes) -> beak::Result<u64> {
        let shared = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            let now = shared.clock.now_ms();
            changes.lent(|lent| shared.kernel.commit(now, lent))
        })
        .await
        .unwrap_or_else(|error| {
            Err(beak::Error {
                status: beak::Status::Panicked,
                reason: error.to_string(),
            })
        })
    }
}

/// The runtime, shared by whoever holds a handle to it.
#[derive(Clone)]
pub struct Runtime {
    shared: Arc<Shared>,
    tasks: TaskTracker,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime").finish_non_exhaustive()
    }
}

impl Runtime {
    #[must_use]
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// The time, on the runtime's clock.
    #[must_use]
    pub fn now_ms(&self) -> i64 {
        self.shared.clock.now_ms()
    }

    /// The kernel, for what reaches it beside the runtime: its namespaces, its commands.
    #[must_use]
    pub fn kernel(&self) -> &Kernel {
        &self.shared.kernel
    }

    /// The volume as the last commit left it.
    ///
    /// # Errors
    /// None that a kernel gives.
    pub fn snapshot(&self) -> beak::Result<Snapshot> {
        self.shared.kernel.snapshot()
    }

    /// Commits `changes` now, all or none, off the async threads: its sequence number.
    ///
    /// # Errors
    /// The first change that cannot apply; the commit's failure.
    pub async fn commit(&self, changes: Changes) -> beak::Result<u64> {
        self.shared.commit(changes).await
    }

    /// A new id, made now.
    ///
    /// # Errors
    /// Past the last id there is.
    pub fn new_id(&self) -> beak::Result<String> {
        self.shared.kernel.new_id(self.now_ms())
    }

    /// The latest commit, as it changes: whoever follows the volume reads its journal on from where it left off.
    #[must_use]
    pub fn committed(&self) -> watch::Receiver<u64> {
        self.shared.committed.subscribe()
    }

    /// Follows what streams on `topic`.
    pub async fn follow(&self, topic: &str) -> Following {
        self.shared.streams.follow(topic).await
    }

    /// Has the dispatcher look for what is due now, as after a clock set forward.
    pub fn wake(&self) {
        self.shared.dispatch.notify_one();
    }

    /// The JSON Schema a person's answer to the request of `key` the process at `process` made fits.
    ///
    /// # Errors
    /// `NoSuchRequest` where none waits; `NotAskedOfPerson` for one a handler answers.
    pub fn answer_schema(&self, process: &[u8], key: &[u8]) -> Result<serde_json::Value, NotGiven> {
        let (name, payload) = self.request(process, key)?;
        let schema = self
            .shared
            .person
            .get(&name)
            .ok_or_else(|| NotGiven::NotAskedOfPerson(String::from_utf8_lossy(&name).into_owned()))?;
        Ok(schema(&payload)?)
    }

    /// Gives a person's `answer` to the request of `key` the process at `process` made, where it fits what is asked.
    ///
    /// # Errors
    /// `Unfit` with each problem where it does not fit; as `answer_schema`; the commit's failure.
    pub async fn answer(&self, process: &[u8], key: &[u8], answer: &serde_json::Value) -> Result<u64, NotGiven> {
        let schema = self.answer_schema(process, key)?;
        let problems = problems(&schema, answer)?;
        if !problems.is_empty() {
            return Err(NotGiven::Unfit(problems));
        }
        let mut changes = Changes::new();
        let mail = Mail {
            key: Bytes::copy_from_slice(key),
            name: Bytes::from_static(ANSWERED.as_bytes()),
            payload: serde_json::to_vec(answer).map_err(anyhow::Error::from)?.into(),
        };
        changes.answer(Bytes::copy_from_slice(process), mail, None);
        Ok(self.commit(changes).await?)
    }

    /// The name and payload of the request of `key` the process at `process` made.
    fn request(&self, process: &[u8], key: &[u8]) -> Result<(Bytes, Bytes), NotGiven> {
        let snapshot = self.snapshot()?;
        let path = layout::request(process, key);
        let entry = snapshot.stat(&path)?.ok_or(NotGiven::NoSuchRequest)?;
        let name = entry
            .directory
            .as_ref()
            .and_then(|directory| directory.label(layout::REQUEST_LABEL))
            .cloned()
            .ok_or(NotGiven::NoSuchRequest)?;
        let payload = snapshot.read(&layout::payload(&path), 0, u64::MAX)?.unwrap_or_default();
        Ok((name, payload))
    }

    /// Stops the runtime's tasks and waits for them: a request being answered is left unanswered, to come back to its
    /// process interrupted as the volume opens again, as though the host had ended.
    pub async fn close(&self) {
        self.shared.shutdown.cancel();
        self.tasks.wait().await;
    }
}
