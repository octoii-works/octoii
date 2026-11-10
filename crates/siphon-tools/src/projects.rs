//! A project, as its threads are grouped, granted and trusted alike: a directory `siphon.project:<sha256 of its
//! folder>` at the volume's root, labelled with its folder, holding what the person granted it for good.

use std::fmt::Write as _;

use beak::Snapshot;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use siphon_runtime::Changes;

use crate::permissions::Grants;

/// The kind of a project's directory, and the label that holds its folder.
pub const KIND: &str = "siphon.project";
pub const FOLDER_LABEL: &str = "siphon.folder";
/// The annotation that holds what the person granted the project.
const GRANTS: &str = "siphon.grants";

/// The directory of the project of `folder`.
#[must_use]
pub fn path(folder: &str) -> Bytes {
    let mut hex = String::new();
    for byte in Sha256::digest(folder.as_bytes()) {
        let _ = write!(hex, "{byte:02x}");
    }
    beak::layout::child(b"", KIND.as_bytes(), hex.as_bytes())
}

/// What the person granted the project of `folder`; nothing where they granted nothing.
///
/// # Errors
/// The volume's failure; grants not as Siphon writes them.
pub fn grants(snapshot: &Snapshot, folder: &str) -> anyhow::Result<Grants> {
    let directory = snapshot.stat(&path(folder))?.and_then(|entry| entry.directory);
    let kept = directory.and_then(|directory| directory.annotation(GRANTS.as_bytes()).cloned());
    Ok(kept.map(|kept| serde_json::from_slice(&kept)).transpose()?.unwrap_or_default())
}

/// The changes that grant `more` to the project of `folder`, beside what it was granted, its directory made where it
/// has none.
///
/// # Errors
/// The volume's failure; grants not as Siphon writes them.
pub fn grant(snapshot: &Snapshot, folder: &str, more: Grants) -> anyhow::Result<Changes> {
    let mut changes = Changes::new();
    let project = path(folder);
    let mut all = grants(snapshot, folder)?;
    all.add(more);
    if snapshot.stat(&project)?.is_none() {
        changes.mkdir(project.clone());
    }
    let label = (
        Bytes::from_static(FOLDER_LABEL.as_bytes()),
        Some(Bytes::copy_from_slice(folder.as_bytes())),
    );
    let annotation = (Bytes::from_static(GRANTS.as_bytes()), Some(Bytes::from(serde_json::to_vec(&all)?)));
    changes.meta(project, vec![label], vec![annotation]);
    Ok(changes)
}
