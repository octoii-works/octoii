//! The protocols providers speak, each shared by the drivers whose providers speak it, which differ in its dialect.

pub mod responses;

use base64::Engine;

use crate::driver::Call;
use crate::vocabulary::{Failure, FailureKind, FileRef};

/// A file's bytes as a data URL, as a request carries a file inline.
///
/// # Errors
/// An `invalid_request` failure where the host has no such file.
pub(crate) fn data_url(call: &Call, file: &FileRef) -> Result<String, Failure> {
    let bytes = call
        .files
        .read(&file.reference)
        .map_err(|missing| Failure::new(FailureKind::InvalidRequest, missing.to_string()))?;
    Ok(format!(
        "data:{};base64,{}",
        file.media_type,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Whether a provider's words say a request is too long for the model's context.
#[must_use]
pub fn context_too_long(message: &str) -> bool {
    const SAID: [&str; 7] = [
        "maximum context length",
        "maximum prompt length",
        "context length",
        "context window",
        "too many tokens",
        "too many input tokens",
        "prompt is too long",
    ];
    let lowered = message.to_lowercase();
    SAID.iter().any(|words| lowered.contains(words))
}
