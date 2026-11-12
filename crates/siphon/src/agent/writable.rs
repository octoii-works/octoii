//! The folders an extension gives a thread's work to write as freely as its workspace — the person's memory — beside
//! the thread's scratchpad and the machine's temporary folders: in the sandbox's reach while their extension is on, and
//! named so to the model.

use std::path::PathBuf;

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
