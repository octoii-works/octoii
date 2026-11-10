//! What every call through the ABI shares: bytes lent to a call, optional values, a status read as a result with its
//! reason, and Beak's buffers taken as owned bytes, let go of as they are taken.

use beak_sys as sys;
use bytes::Bytes;

use crate::{Error, Result, Status};

/// `bytes`, lent to a call.
pub(crate) fn lent(bytes: &[u8]) -> sys::beak_bytes {
    sys::beak_bytes {
        ptr: bytes.as_ptr(),
        len: bytes.len(),
    }
}

pub(crate) fn maybe(bytes: Option<&[u8]>) -> sys::beak_maybe_bytes {
    sys::beak_maybe_bytes {
        has: bytes.is_some(),
        bytes: lent(bytes.unwrap_or_default()),
    }
}

pub(crate) fn maybe_i64(value: Option<i64>) -> sys::beak_maybe_i64 {
    sys::beak_maybe_i64 {
        has: value.is_some(),
        value: value.unwrap_or_default(),
    }
}

pub(crate) fn maybe_u64(value: Option<u64>) -> sys::beak_maybe_u64 {
    sys::beak_maybe_u64 {
        has: value.is_some(),
        value: value.unwrap_or_default(),
    }
}

pub(crate) fn of_i64(value: sys::beak_maybe_i64) -> Option<i64> {
    value.has.then_some(value.value)
}

pub(crate) fn of_u64(value: sys::beak_maybe_u64) -> Option<u64> {
    value.has.then_some(value.value)
}

/// The bytes a buffer Beak gave inside another holds, copied; the buffer is let go of with what holds it.
pub(crate) fn copied(buffer: &sys::beak_buffer) -> Bytes {
    if buffer.len == 0 {
        return Bytes::new();
    }
    // SAFETY: a buffer Beak gave holds its length of bytes.
    Bytes::copy_from_slice(unsafe { std::slice::from_raw_parts(buffer.ptr, buffer.len) })
}

pub(crate) fn copied_maybe(buffer: &sys::beak_maybe_buffer) -> Option<Bytes> {
    buffer.has.then(|| copied(&buffer.buffer))
}

/// The bytes a buffer Beak gave the caller holds, the buffer let go of.
pub(crate) fn taken(mut buffer: sys::beak_buffer) -> Bytes {
    let bytes = copied(&buffer);
    // SAFETY: the buffer is Beak's, let go of once.
    unsafe { sys::beak_buffer_free(&raw mut buffer) };
    bytes
}

pub(crate) fn empty() -> sys::beak_buffer {
    sys::beak_buffer {
        ptr: std::ptr::null_mut(),
        len: 0,
    }
}

/// The reason the last call on this thread that failed gave.
fn last_error() -> String {
    let mut out = vec![0; 512];
    // SAFETY: the buffer has room for what it says.
    let len = unsafe { sys::beak_last_error(out.as_mut_ptr(), out.len()) };
    if len > out.len() {
        out.resize(len, 0);
        // SAFETY: as above.
        unsafe { sys::beak_last_error(out.as_mut_ptr(), out.len()) };
    }
    out.truncate(len);
    String::from_utf8_lossy(&out).into_owned()
}

/// A call's status as a result, the reason of a failure with it.
pub(crate) fn check(status: sys::beak_status) -> Result<()> {
    let status = match status {
        sys::beak_status::BEAK_STATUS_OK => return Ok(()),
        sys::beak_status::BEAK_STATUS_NOT_FOUND => Status::NotFound,
        sys::beak_status::BEAK_STATUS_REFUSED => Status::Refused,
        sys::beak_status::BEAK_STATUS_INVALID => Status::Invalid,
        sys::beak_status::BEAK_STATUS_EXISTS => Status::Exists,
        sys::beak_status::BEAK_STATUS_READ_ONLY => Status::ReadOnly,
        sys::beak_status::BEAK_STATUS_DENIED => Status::Denied,
        sys::beak_status::BEAK_STATUS_UNSUPPORTED => Status::Unsupported,
        sys::beak_status::BEAK_STATUS_MISSING_PROGRAM => Status::MissingProgram,
        sys::beak_status::BEAK_STATUS_CLOSED => Status::Closed,
        sys::beak_status::BEAK_STATUS_FAILED => Status::Failed,
        sys::beak_status::BEAK_STATUS_PANICKED => Status::Panicked,
    };
    Err(Error {
        status,
        reason: last_error(),
    })
}

/// What a call gives into an out-parameter, once it succeeded.
pub(crate) fn given<T>(call: impl FnOnce(*mut T) -> sys::beak_status) -> Result<T>
where
    T: Default,
{
    let mut out = T::default();
    check(call(&raw mut out))?;
    Ok(out)
}

/// What a call gives as a pointer to an object of Beak's, once it succeeded.
pub(crate) fn object<T>(call: impl FnOnce(*mut *mut T) -> sys::beak_status) -> Result<*mut T> {
    let mut out = std::ptr::null_mut();
    check(call(&raw mut out))?;
    Ok(out)
}
