//! Requests done once: a client's request that makes or adds something carries a key of its own, as an
//! `Idempotency-Key` does, and what it did — its method, its thread, what it gave — is kept a day under
//! `siphon.idempotency/<key>`, written in the commit that does it and gone as Beak expires it. The same request sent
//! again within the day, as a client sends one again after its connection was lost, is answered with what it gave,
//! not done again.

use beak::Snapshot;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use siphon_runtime::Changes;

/// The directory the keys of the last day are kept in.
pub const DIRECTORY: &str = "siphon.idempotency";

/// How long a key is kept.
pub const RETENTION_MS: i64 = 24 * 60 * 60 * 1000;

/// The most bytes a key holds.
pub const KEY_BYTES_MAX: usize = 128;

/// What a request done under a key did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct Done {
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    pub result: String,
}

/// Why a key was refused.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the idempotency key was used by another request")]
    Reused,
    #[error("an idempotency key is 1 to {KEY_BYTES_MAX} letters, digits, '.', '_' or '-'")]
    InvalidKey,
    #[error(transparent)]
    Beak(#[from] beak::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

fn file(key: &str) -> Bytes {
    [DIRECTORY.as_bytes(), b"/", key.as_bytes()].concat().into()
}

/// What the request of `method` on `thread` did under `key` before, where it was done within the day.
///
/// # Errors
/// `InvalidKey` for a key no file can be named by; `Reused` where another request was done under it.
pub fn done_before(snapshot: &Snapshot, key: &str, method: &str, thread: Option<&str>) -> Result<Option<Done>, Error> {
    let valid = (1..=KEY_BYTES_MAX).contains(&key.len())
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if !valid {
        return Err(Error::InvalidKey);
    }
    let Some(bytes) = snapshot.read(&file(key), 0, u64::MAX)? else {
        return Ok(None);
    };
    let done: Done = serde_json::from_slice(&bytes)?;
    if done.method != method || done.thread.as_deref() != thread {
        return Err(Error::Reused);
    }
    Ok(Some(done))
}

/// Keeps what `done` did under `key` for a day from `now`, with `changes`.
///
/// # Errors
/// What cannot be written as JSON.
pub fn record(changes: &mut Changes, key: &str, done: &Done, now: i64) -> serde_json::Result<()> {
    changes
        .mkdir(Bytes::from_static(DIRECTORY.as_bytes()))
        .write(file(key), serde_json::to_vec(done)?)
        .expire(file(key), Some(now + RETENTION_MS));
    Ok(())
}
