//! The folders an extension gives a thread's work to write as freely as its workspace — the person's memory — beside
//! the thread's scratchpad and the machine's temporary folders: in the sandbox's reach while their extension is on, and
//! named so to the model.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use beak::Snapshot;

use crate::machine::Host;

/// A folder an extension gives: the extension, the words the model knows it by, and the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Writable {
    pub extension: String,
    pub named: &'static str,
    pub folder: PathBuf,
}

/// Of `writables`, those whose extensions are switched on, as the settings in `snapshot` say.
pub fn on<'a>(writables: &'a [Writable], snapshot: &Snapshot) -> impl Iterator<Item = &'a Writable> {
    writables
        .iter()
        .filter(move |writable| crate::settings::switched_on(snapshot, &writable.extension))
}

/// The places beside its workspace that the thread `thread`'s work writes as freely as it, on `host`: its scratchpad,
/// the machine's temporary folders, and the folders `writables` gives whose extensions are on in `snapshot`.
pub fn places(host: &Host, thread: &str, writables: &[Writable], snapshot: &Snapshot) -> Vec<PathBuf> {
    let mut places = host.places_of(thread);
    places.extend(on(writables, snapshot).map(|writable| writable.folder.clone()));
    places
}

/// Read-only view of all registered extra writable folders, finalized before Siphon starts.
#[derive(Debug, Clone)]
pub struct WritableFolders(Arc<OnceLock<Vec<Writable>>>);

impl WritableFolders {
    pub(crate) fn new() -> Self {
        Self(Arc::new(OnceLock::new()))
    }

    pub(crate) fn finish(&self, folders: Vec<Writable>) -> beak::Result<()> {
        self.0.set(folders).map_err(|_| beak::Error {
            status: beak::Status::Invalid,
            reason: "writable folders already finalized".into(),
        })
    }

    /// Extra folders whose extensions are enabled in the volume's current settings.
    ///
    /// # Errors
    /// Siphon has not started, or the volume could not be read.
    pub fn read(&self, volume: &crate::volume::Volume) -> beak::Result<Vec<Writable>> {
        let folders = self.0.get().ok_or_else(|| beak::Error {
            status: beak::Status::Invalid,
            reason: "writable folders are not finalized until Siphon starts".into(),
        })?;
        let snapshot = volume.snapshot()?;
        Ok(on(folders, &snapshot).cloned().collect())
    }
}
