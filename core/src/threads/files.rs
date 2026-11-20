//! The files a thread's conversation holds — an image pasted, a PDF attached, an image a call read — kept in the
//! thread's `files/` by the SHA-256 of their bytes, as git keeps a blob: written once, never changed, the same bytes
//! kept once however often given. A message names one by its reference, `sha256:` and the hash, and its bytes are read
//! as the message is sent.

use std::fmt::Write as _;

use bytes::Bytes;
use octoii_ai::vocabulary::FileRef;
use octoii_siphon_lib::{Batch, Changes, Snapshot};
use sha2::{Digest, Sha256};

use crate::threads::thread;

/// The most bytes a file holds.
pub const BYTES_MAX: usize = 32 << 20;

/// A kept original's media type, without making archives or text into media the model's `read` tool would decode.
/// HTML uses its case-insensitive `.html` or `.htm` extension only for UTF-8 text without NUL, preserving every byte.
#[must_use]
pub fn media_type(name: &str, bytes: &[u8]) -> &'static str {
    if let Some(media_type) = crate::machine::media_type(bytes) {
        media_type
    } else if [b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08"]
        .iter()
        .any(|head| bytes.starts_with(*head))
    {
        "application/zip"
    } else if siphon_text::Text::of(bytes).is_none() {
        "application/octet-stream"
    } else if name
        .rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm"))
    {
        "text/html"
    } else {
        "text/plain"
    }
}

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
pub(crate) fn keep(snapshot: &Snapshot, changes: &mut Batch<'_>, id: &str, bytes: Bytes) -> octoii_siphon_lib::Result<String> {
    let reference = reference_of(&bytes);
    let at = path(id, &reference).unwrap_or_default();
    if snapshot.stat(&at)?.is_none() {
        changes.mkdir(thread::within(id, "files"))?.write(at, bytes)?;
    }
    Ok(reference)
}

/// Why a file given was not kept.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("no such thread")]
    NoSuchThread,
    #[error("a file holds at most {BYTES_MAX} bytes")]
    TooLarge,
    #[error("the image: {0}")]
    Image(#[from] siphon_image::Error),
    #[error(transparent)]
    Beak(#[from] octoii_siphon_lib::Error),
}

/// Keeps the image `bytes`, named `name`, in the thread `id` with `changes`, fitted to what every model reads: what
/// names it, and the size it was kept at.
///
/// # Errors
/// An image that cannot be read, or fits no bounds; the volume's failure, reading what is kept.
pub(crate) fn keep_image(
    snapshot: &Snapshot,
    changes: &mut Batch<'_>,
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
pub(crate) fn keep_given(
    snapshot: &Snapshot,
    changes: &mut Batch<'_>,
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
pub fn read(snapshot: &Snapshot, id: &str, reference: &str) -> octoii_siphon_lib::Result<Option<Bytes>> {
    let Some(at) = path(id, reference) else { return Ok(None) };
    snapshot.read_range(&at, 0, u64::MAX)
}

/// The files of the thread `thread`, as a model request of its conversation reads them: each by its reference, from
/// the volume as it is.
#[derive(Debug, Clone)]
pub struct OfThread {
    pub runtime: octoii_siphon_lib::RuntimeHandle,
    pub thread: String,
}

impl octoii_ai::host::Files for OfThread {
    fn read(&self, reference: &str) -> Result<Bytes, octoii_ai::host::NoSuchFile> {
        let missing = || octoii_ai::host::NoSuchFile(reference.to_owned());
        let snapshot = self.runtime.snapshot().map_err(|_| missing())?;
        read(&snapshot, &self.thread, reference).ok().flatten().ok_or_else(missing)
    }
}

impl crate::volume::Volume {
    /// Keeps file bytes in the existing thread, named by their content, without fitting them for a model. Returns
    /// the reference, detected media type, basename and size only once storage commits.
    ///
    /// # Errors
    /// No such thread, a file past the byte limit, or the volume's failure.
    pub async fn keep_file(&self, thread: &str, bytes: Bytes, name: &str) -> std::result::Result<(FileRef, u64), Refused> {
        let snapshot = self.file_snapshot(thread)?;
        if bytes.len() > BYTES_MAX {
            return Err(Refused::TooLarge);
        }
        let size = bytes.len() as u64;
        let media_type = media_type(name, &bytes);
        let mut changes = Changes::new()?;
        let reference = keep(&snapshot, &mut changes, thread, bytes)?;
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
    ) -> std::result::Result<(FileRef, siphon_image::Size), Refused> {
        let snapshot = self.file_snapshot(thread)?;
        if bytes.len() > BYTES_MAX {
            return Err(Refused::TooLarge);
        }
        let mut changes = Changes::new()?;
        let kept = keep_image(&snapshot, &mut changes, thread, bytes, name)?;
        if !changes.is_empty() {
            self.runtime.commit(changes).await?;
        }
        Ok(kept)
    }

    /// Keeps an attachment in a thread. Images are fitted for the model and other files keep their original bytes.
    ///
    /// # Errors
    /// A missing thread, an invalid or oversized attachment, or a volume failure.
    pub async fn keep_attachment(
        &self,
        thread: &str,
        bytes: &[u8],
        media_type: &str,
        name: Option<&str>,
    ) -> std::result::Result<FileRef, Refused> {
        let snapshot = self.file_snapshot(thread)?;
        let mut changes = Changes::new()?;
        let file = keep_given(&snapshot, &mut changes, thread, bytes, media_type, name)?;
        if !changes.is_empty() {
            self.runtime.commit(changes).await?;
        }
        Ok(file)
    }

    /// Reads file bytes by the reference kept in this thread.
    ///
    /// # Errors
    /// A missing thread or a volume failure.
    pub fn read_file(&self, thread: &str, reference: &str) -> std::result::Result<Option<Bytes>, Refused> {
        Ok(read(&*self.file_snapshot(thread)?, thread, reference)?)
    }

    fn file_snapshot(&self, thread: &str) -> std::result::Result<octoii_siphon_lib::OwnedSnapshot, Refused> {
        let snapshot = self.snapshot()?;
        if thread::directory(&snapshot, thread)?.is_none() {
            return Err(Refused::NoSuchThread);
        }
        Ok(snapshot)
    }
}
