//! The volume as an extension reaches it: read as it stands, ids minted from it, its time — and written only through
//! Siphon's primitives, which hold the rules of what may be written: a delivery, a plan of a call's, the person's trust,
//! a record of runs while they run. An extension is given this, never the runtime nor the kernel, which write anything.

use std::future::Future;

use bytes::Bytes;
use siphon_runtime::{Changes, Runtime};

/// What the volume's reads give, and how the volume and the machine's commands fail, as Beak tells it.
pub use beak::{Error, Result, Snapshot};

/// The volume, as an extension reaches it.
#[derive(Debug, Clone)]
pub struct Volume {
    pub(crate) runtime: Runtime,
}

impl Volume {
    /// The volume as it stands now, to read.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn snapshot(&self) -> beak::Result<Snapshot> {
        self.runtime.snapshot()
    }

    /// A new id, minted from the volume: a `UUIDv7`, rising.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn new_id(&self) -> beak::Result<String> {
        self.runtime.new_id()
    }

    /// The machine's commands, for what an extension runs on it: they write no volume.
    #[must_use]
    pub fn commands(&self) -> crate::machine::Commands {
        crate::machine::Commands {
            kernel: self.runtime.kernel().clone(),
        }
    }

    /// The time, in ms since the epoch, as the runtime counts it.
    #[must_use]
    pub fn now_ms(&self) -> i64 {
        self.runtime.now_ms()
    }

    /// What `work` comes to, given a record of the kind `kind` for the runs it makes — the commands a run of git makes,
    /// held under it while they run — made for it and taken away after it.
    ///
    /// # Errors
    /// The volume's failure to make the record.
    pub async fn recorded<F, Done, T>(&self, kind: &str, work: F) -> beak::Result<T>
    where
        F: FnOnce(Bytes) -> Done,
        Done: Future<Output = T>,
    {
        let record = beak::layout::child(b"", kind.as_bytes(), self.runtime.new_id()?.as_bytes());
        let mut changes = Changes::new();
        changes.mkdir(record.clone());
        self.runtime.commit(changes).await?;
        let done = work(record.clone()).await;
        let mut changes = Changes::new();
        changes.force_remove(record, None);
        // A record left by a failed removal goes as the volume collects what nothing holds.
        let _ = self.runtime.commit(changes).await;
        Ok(done)
    }
}
