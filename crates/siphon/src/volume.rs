//! The volume as an extension reaches it: read as it stands, ids minted from it, its time — and written only through
//! Siphon's primitives, which hold the rules of what may be written: a delivery, a plan of a call's, the person's trust,
//! a record of runs while they run. An extension is given this, never the runtime nor the kernel, which write anything.

use std::future::Future;

use bytes::Bytes;
use octoii_ai::vocabulary::FileRef;
use siphon_runtime::{Changes, Runtime};

use crate::threads::{files, operations, thread};

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

    /// Keeps file bytes in the existing thread, named by their content, without fitting them for a model. Returns
    /// the reference, detected media type, basename and size only once storage commits.
    ///
    /// # Errors
    /// No such thread, a file past the byte limit, or the volume's failure.
    pub async fn keep_file(&self, thread: &str, bytes: Bytes, name: &str) -> std::result::Result<(FileRef, u64), files::Refused> {
        let snapshot = self.file_snapshot(thread)?;
        if bytes.len() > files::BYTES_MAX {
            return Err(files::Refused::TooLarge);
        }
        let size = bytes.len() as u64;
        let media_type = files::media_type(name, &bytes);
        let mut changes = Changes::new();
        let reference = files::keep(&snapshot, &mut changes, thread, bytes)?;
        if !changes.is_empty() {
            self.runtime.commit(changes).await?;
        }
        Ok((
            FileRef {
                reference,
                media_type: media_type.to_owned(),
                filename: Some(name.to_owned()),
            },
            size,
        ))
    }

    /// Keeps an image in the existing thread's files, fitted to what every model reads and named by its content.
    /// Returns its reference and fitted size only once storage commits.
    ///
    /// # Errors
    /// No such thread, an image past the byte limit or one that cannot be read, or the volume's failure.
    pub async fn keep_image(
        &self,
        thread: &str,
        bytes: &[u8],
        name: Option<&str>,
    ) -> std::result::Result<(FileRef, siphon_image::Size), files::Refused> {
        let snapshot = self.file_snapshot(thread)?;
        if bytes.len() > files::BYTES_MAX {
            return Err(files::Refused::TooLarge);
        }
        let mut changes = Changes::new();
        let kept = files::keep_image(&snapshot, &mut changes, thread, bytes, name)?;
        if !changes.is_empty() {
            self.runtime.commit(changes).await?;
        }
        Ok(kept)
    }

    fn file_snapshot(&self, thread: &str) -> std::result::Result<Snapshot, files::Refused> {
        let snapshot = self.snapshot()?;
        let exists = snapshot
            .stat(&thread::path(thread))?
            .and_then(|entry| entry.directory)
            .is_some_and(|directory| !operations::removing(&directory));
        if !exists {
            return Err(files::Refused::NoSuchThread);
        }
        Ok(snapshot)
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
