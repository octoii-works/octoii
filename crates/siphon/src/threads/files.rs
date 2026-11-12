//! The files a thread's conversation holds — an image pasted, a PDF attached, an image a call read — kept in the
//! thread's `files/` by the SHA-256 of their bytes, as git keeps a blob: written once, never changed, the same bytes
//! kept once however often given. A message names one by its reference, `sha256:` and the hash, and its bytes are read
//! as the message is sent.

use std::fmt::Write as _;

use beak::Snapshot;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use siphon_runtime::Changes;

use crate::threads::thread;

/// The most bytes a file holds.
pub const BYTES_MAX: usize = 32 << 20;

/// What a reference begins with, before the hash in lowercase hex.
const SCHEME: &str = "sha256:";

/// The reference `bytes` are kept by.
#[must_use]
pub fn reference_of(bytes: &[u8]) -> String {
    let mut reference = String::from(SCHEME);
    for byte in Sha256::digest(bytes) {
        let _ = write!(reference, "{byte:02x}");
    }
    reference
}

/// Where the thread `id` keeps the file of `reference`; none for a reference no file is kept by.
#[must_use]
pub fn path(id: &str, reference: &str) -> Option<Bytes> {
    let hex = reference.strip_prefix(SCHEME)?;
    (hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())).then(|| thread::within(id, &format!("files/{hex}")))
}

/// Keeps `bytes` in the thread `id`, with `changes`, where they are not kept already: their reference.
///
/// # Errors
/// The volume's failure, reading what is kept.
pub fn keep(snapshot: &Snapshot, changes: &mut Changes, id: &str, bytes: Bytes) -> beak::Result<String> {
    let reference = reference_of(&bytes);
    let at = path(id, &reference).unwrap_or_default();
    if snapshot.stat(&at)?.is_none() {
        changes.mkdir(thread::within(id, "files")).write(at, bytes);
    }
    Ok(reference)
}

/// Why a file given was not kept.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("a file holds at most {BYTES_MAX} bytes")]
    TooLarge,
    #[error("the image: {0}")]
    Image(#[from] siphon_image::Error),
    #[error(transparent)]
    Beak(#[from] beak::Error),
}

/// Keeps the image `bytes`, named `name`, in the thread `id` with `changes`, fitted to what every model reads: what
/// names it, and the size it was kept at.
///
/// # Errors
/// An image that cannot be read, or fits no bounds; the volume's failure, reading what is kept.
pub fn keep_image(
    snapshot: &Snapshot,
    changes: &mut Changes,
    id: &str,
    bytes: &[u8],
    name: Option<&str>,
) -> Result<(octoii_ai::vocabulary::FileRef, siphon_image::Size), Refused> {
    let fitted = siphon_image::fit(bytes, &siphon_image::Bounds::MODELS)?;
    let reference = keep(snapshot, changes, id, Bytes::copy_from_slice(&fitted.bytes))?;
    let file = octoii_ai::vocabulary::FileRef {
        reference,
        media_type: fitted.format.media_type().to_owned(),
        filename: name.map(str::to_owned),
    };
    Ok((file, fitted.size))
}

/// Keeps `bytes`, of `media_type` and named `name`, in the thread `id` with `changes`, as every model reads what a
/// conversation names: an image fitted to what each reads, any other file as it came. What names it.
///
/// # Errors
/// `TooLarge`; an image that cannot be read; the volume's failure, reading what is kept.
pub fn keep_given(
    snapshot: &Snapshot,
    changes: &mut Changes,
    id: &str,
    bytes: &[u8],
    media_type: &str,
    name: Option<&str>,
) -> Result<octoii_ai::vocabulary::FileRef, Refused> {
    if bytes.len() > BYTES_MAX {
        return Err(Refused::TooLarge);
    }
    if media_type.starts_with("image/") {
        return Ok(keep_image(snapshot, changes, id, bytes, name)?.0);
    }
    let reference = keep(snapshot, changes, id, Bytes::copy_from_slice(bytes))?;
    Ok(octoii_ai::vocabulary::FileRef {
        reference,
        media_type: media_type.to_owned(),
        filename: name.map(str::to_owned),
    })
}

/// The bytes the thread `id` keeps by `reference`; none where it keeps none.
///
/// # Errors
/// The volume's failure.
pub fn read(snapshot: &Snapshot, id: &str, reference: &str) -> beak::Result<Option<Bytes>> {
    let Some(at) = path(id, reference) else { return Ok(None) };
    snapshot.read(&at, 0, u64::MAX)
}

/// The files of the thread `thread`, as a model request of its conversation reads them: each by its reference, from
/// the volume as it is.
#[derive(Debug, Clone)]
pub struct OfThread {
    pub runtime: siphon_runtime::Runtime,
    pub thread: String,
}

impl octoii_ai::host::Files for OfThread {
    fn read(&self, reference: &str) -> Result<Bytes, octoii_ai::host::NoSuchFile> {
        let missing = || octoii_ai::host::NoSuchFile(reference.to_owned());
        let snapshot = self.runtime.snapshot().map_err(|_| missing())?;
        read(&snapshot, &self.thread, reference).ok().flatten().ok_or_else(missing)
    }
}
