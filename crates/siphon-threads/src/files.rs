//! The files a thread's conversation holds — an image pasted, a PDF attached, an image a call read — kept in the
//! thread's `files/` by the SHA-256 of their bytes, as git keeps a blob: written once, never changed, the same bytes
//! kept once however often given. A message names one by its reference, `sha256:` and the hash, and its bytes are read
//! as the message is sent.

use std::fmt::Write as _;

use beak::Snapshot;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use siphon_runtime::Changes;

use crate::thread;

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

/// The bytes the thread `id` keeps by `reference`; none where it keeps none.
///
/// # Errors
/// The volume's failure.
pub fn read(snapshot: &Snapshot, id: &str, reference: &str) -> beak::Result<Option<Bytes>> {
    let Some(at) = path(id, reference) else { return Ok(None) };
    snapshot.read(&at, 0, u64::MAX)
}
