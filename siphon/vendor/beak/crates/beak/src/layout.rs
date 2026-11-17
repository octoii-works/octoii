//! The names Beak gives what it keeps in a volume, as the header defines them, and the paths they make: a process
//! keyed `k` under `p` at `p/system.process:k`, mail keyed `k` in its mailbox at `p/system.mail:k`, a request it made at
//! `p/system.request:k`, each message's and request's payload in its `system.payload`.

use beak_sys as sys;
use bytes::Bytes;

/// A name the header defines, without its terminating NUL.
const fn name(defined: &'static [u8]) -> &'static [u8] {
    match defined.split_last() {
        Some((0, name)) => name,
        _ => panic!("a name the header defines ends in NUL"),
    }
}

pub const PROCESS_KIND: &[u8] = name(sys::BEAK_PROCESS_KIND);
pub const INPUT_NAME: &[u8] = name(sys::BEAK_INPUT_NAME);
pub const STAT_NAME: &[u8] = name(sys::BEAK_STAT_NAME);
pub const MAIL_KIND: &[u8] = name(sys::BEAK_MAIL_KIND);
pub const REQUEST_KIND: &[u8] = name(sys::BEAK_REQUEST_KIND);
pub const PAYLOAD_NAME: &[u8] = name(sys::BEAK_PAYLOAD_NAME);
pub const RUNNING_MAX_ANNOTATION: &[u8] = name(sys::BEAK_RUNNING_MAX_ANNOTATION);
pub const MAX_ANNOTATION_PREFIX: &[u8] = name(sys::BEAK_MAX_ANNOTATION_PREFIX);
pub const LIMIT_REASON: &[u8] = name(sys::BEAK_LIMIT_REASON);
pub const REMOVALS: &[u8] = name(sys::BEAK_REMOVALS);
pub const PROGRAM_LABEL: &[u8] = name(sys::BEAK_PROGRAM_LABEL);
pub const FROZEN_LABEL: &[u8] = name(sys::BEAK_FROZEN_LABEL);
pub const HELD_LABEL: &[u8] = name(sys::BEAK_HELD_LABEL);
pub const ATTENTION_LABEL: &[u8] = name(sys::BEAK_ATTENTION_LABEL);
pub const MAIL_LABEL: &[u8] = name(sys::BEAK_MAIL_LABEL);
pub const REQUEST_LABEL: &[u8] = name(sys::BEAK_REQUEST_LABEL);
pub const INTERRUPTED_LABEL: &[u8] = name(sys::BEAK_INTERRUPTED_LABEL);
pub const REMOVING_LABEL: &[u8] = name(sys::BEAK_REMOVING_LABEL);

/// The directory of `kind` keyed `key` under `parent`: `parent/kind:key`, or `kind:key` at the root, `parent` empty.
#[must_use]
pub fn child(parent: &[u8], kind: &[u8], key: &[u8]) -> Bytes {
    let mut path = Vec::with_capacity(parent.len() + kind.len() + key.len() + 2);
    if !parent.is_empty() {
        path.extend_from_slice(parent);
        path.push(b'/');
    }
    path.extend_from_slice(kind);
    path.push(b':');
    path.extend_from_slice(key);
    path.into()
}

/// The process keyed `key` under `parent`.
#[must_use]
pub fn process(parent: &[u8], key: &[u8]) -> Bytes {
    child(parent, PROCESS_KIND, key)
}

/// The mail keyed `key` in the mailbox of the process at `process`.
#[must_use]
pub fn mail(process: &[u8], key: &[u8]) -> Bytes {
    child(process, MAIL_KIND, key)
}

/// The request keyed `key` the process at `process` made.
#[must_use]
pub fn request(process: &[u8], key: &[u8]) -> Bytes {
    child(process, REQUEST_KIND, key)
}

/// The payload of the message or request at `directory`.
#[must_use]
pub fn payload(directory: &[u8]) -> Bytes {
    [directory, b"/", PAYLOAD_NAME].concat().into()
}
