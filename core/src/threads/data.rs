//! Small, named extension records kept with the thread that owns them.

use bytes::Bytes;
use octoii_siphon_lib::RuntimeHandle;

use crate::volume::commit_checked;

use super::thread;

const MAX_BYTES: usize = 32 * 1024;

/// The records of one registered extension. Deleting a thread removes its extension data with it.
#[derive(Debug, Clone)]
pub struct ThreadData {
    runtime: RuntimeHandle,
    extension: &'static str,
}

impl ThreadData {
    pub(crate) fn new(runtime: RuntimeHandle, extension: &'static str) -> Self {
        Self { runtime, extension }
    }

    /// Reads a record or returns `None` if it is absent.
    ///
    /// # Errors
    /// An invalid key or a volume failure.
    pub fn read(&self, id: &str, key: &str) -> anyhow::Result<Option<Bytes>> {
        self.check_names(id, key)?;
        let path = thread::within(id, &format!("extensions/{}/{key}", self.extension));
        Ok(self.runtime.snapshot()?.read_range(&path, 0, MAX_BYTES as u64)?)
    }

    /// Replaces a record while the owning thread exists.
    ///
    /// # Errors
    /// An invalid key, oversized record, missing thread, or volume failure.
    pub async fn write(&self, id: &str, key: &str, bytes: Bytes) -> anyhow::Result<()> {
        self.check_names(id, key)?;
        anyhow::ensure!(bytes.len() <= MAX_BYTES, "an extension record holds at most {MAX_BYTES} bytes");
        let folder = thread::within(id, &format!("extensions/{}", self.extension));
        let path = thread::within(id, &format!("extensions/{}/{key}", self.extension));
        commit_checked(&self.runtime, |snapshot, changes| {
            anyhow::ensure!(thread::directory(snapshot, id)?.is_some(), "no such thread");
            if snapshot.read_range(&path, 0, MAX_BYTES as u64)?.as_ref() != Some(&bytes) {
                changes
                    .mkdir(thread::within(id, "extensions"))?
                    .mkdir(&folder)?
                    .write(&path, bytes.clone())?;
            }
            Ok(())
        })
        .await
    }

    fn check_names(&self, id: &str, key: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            segment(id) && segment(self.extension) && segment(key),
            "invalid extension record path"
        );
        Ok(())
    }
}

fn segment(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
