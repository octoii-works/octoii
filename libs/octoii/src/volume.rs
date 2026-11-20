//! Access to volume snapshots, time, IDs and snapshot-checked product writes.

use octoii_siphon_lib::{Batch, Changes, RuntimeHandle, Status};

/// The types used by volume reads and Siphon operations.
pub use octoii_siphon_lib::{Error, Result, Snapshot};

/// Plans a write from one snapshot and retries when a concurrent commit changes the head.
/// Staging can run again, so callers keep effects outside the volume out of the plan.
pub(crate) async fn commit_checked<T, E: From<Error>>(
    runtime: &RuntimeHandle,
    mut plan: impl FnMut(&Snapshot, &mut Batch<'_>) -> std::result::Result<T, E>,
) -> std::result::Result<T, E> {
    loop {
        let snapshot = runtime.snapshot()?;
        let mut changes = Changes::new()?;
        let result = plan(&snapshot, &mut changes)?;
        if changes.is_empty() {
            return Ok(result);
        }
        changes.expect(&snapshot)?;
        drop(snapshot);
        match runtime.commit(changes).await {
            Ok(_) => return Ok(result),
            Err(error) if error.status() == Status::Conflict => {}
            Err(error) => return Err(error.into()),
        }
    }
}

/// The volume, as an extension reaches it.
#[derive(Debug, Clone)]
pub struct Volume {
    pub(crate) runtime: RuntimeHandle,
}

impl Volume {
    /// The volume as it stands now, to read.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn snapshot(&self) -> octoii_siphon_lib::Result<octoii_siphon_lib::OwnedSnapshot> {
        self.runtime.snapshot()
    }

    /// A new id, minted from the volume: a `UUIDv7`, rising.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn new_id(&self) -> octoii_siphon_lib::Result<String> {
        self.runtime.new_id()
    }

    /// The time, in ms since the epoch, as the runtime counts it.
    ///
    /// # Errors
    /// The runtime has closed.
    pub fn now_ms(&self) -> octoii_siphon_lib::Result<i64> {
        self.runtime.now_ms()
    }
}
