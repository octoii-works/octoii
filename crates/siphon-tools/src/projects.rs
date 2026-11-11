//! A project, as its threads are grouped, granted and trusted alike: a directory `siphon.project:<sha256 of its
//! folder>` at the volume's root, labelled with its folder, its annotations holding what the person granted it for good
//! and whether they trust what its configuration runs.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use beak::Snapshot;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use siphon_runtime::Changes;

use crate::permissions::Grants;

/// Where a thread works: its workspace, and its project's folder — the git repository that holds the workspace, else
/// the workspace itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub workspace: PathBuf,
    pub project: PathBuf,
}

impl Place {
    /// Where a thread working in `workspace` works.
    #[must_use]
    pub fn of(workspace: &Path) -> Self {
        let project = workspace
            .ancestors()
            .find(|folder| folder.join(".git").exists())
            .unwrap_or(workspace);
        Self {
            workspace: workspace.to_owned(),
            project: project.to_owned(),
        }
    }
}

/// The kind of a project's directory, and the label that holds its folder.
pub const KIND: &str = "siphon.project";
pub const FOLDER_LABEL: &str = "siphon.folder";
/// The annotation that holds what the person granted the project.
const GRANTS: &str = "siphon.grants";

/// The directory of the project of `folder`.
#[must_use]
pub fn path(folder: &str) -> Bytes {
    beak::layout::child(b"", KIND.as_bytes(), sha256_hex(folder.as_bytes()).as_bytes())
}

/// The SHA-256 of `bytes`, in lowercase hexadecimal.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().fold(String::with_capacity(64), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// The annotation `name` of the project of `folder`; none where it has none.
///
/// # Errors
/// The volume's failure.
pub fn annotation(snapshot: &Snapshot, folder: &str, name: &str) -> beak::Result<Option<Bytes>> {
    let directory = snapshot.stat(&path(folder))?.and_then(|entry| entry.directory);
    Ok(directory.and_then(|directory| directory.annotation(name.as_bytes()).cloned()))
}

/// The changes that set the project of `folder`'s annotation `name` to `value`, its directory made where it has none,
/// labelled with its folder.
///
/// # Errors
/// The volume's failure.
pub fn annotate(snapshot: &Snapshot, folder: &str, name: &str, value: Bytes) -> beak::Result<Changes> {
    let mut changes = Changes::new();
    let project = path(folder);
    if snapshot.stat(&project)?.is_none() {
        changes.mkdir(project.clone());
    }
    let label = (
        Bytes::from_static(FOLDER_LABEL.as_bytes()),
        Some(Bytes::copy_from_slice(folder.as_bytes())),
    );
    changes.meta(project, vec![label], vec![(Bytes::copy_from_slice(name.as_bytes()), Some(value))]);
    Ok(changes)
}

/// What the person granted the project of `folder`; nothing where they granted nothing.
///
/// # Errors
/// The volume's failure; grants not as Siphon writes them.
pub fn grants(snapshot: &Snapshot, folder: &str) -> anyhow::Result<Grants> {
    let kept = annotation(snapshot, folder, GRANTS)?;
    Ok(kept.map(|kept| serde_json::from_slice(&kept)).transpose()?.unwrap_or_default())
}

/// The changes that grant `more` to the project of `folder`, beside what it was granted.
///
/// # Errors
/// The volume's failure; grants not as Siphon writes them.
pub fn grant(snapshot: &Snapshot, folder: &str, more: Grants) -> anyhow::Result<Changes> {
    let mut all = grants(snapshot, folder)?;
    all.add(more);
    Ok(annotate(snapshot, folder, GRANTS, Bytes::from(serde_json::to_vec(&all)?))?)
}
