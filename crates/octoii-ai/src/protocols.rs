//! The protocols providers speak, each shared by the drivers whose providers speak it, which differ in its dialect.

pub mod anthropic;
pub mod chat_completions;
pub mod google;
pub mod responses;

use base64::Engine;

use crate::driver::Call;
use crate::vocabulary::{Failure, FailureKind, FileRef};

/// The bytes of the file of `reference`, as the conversation the call is of holds them.
///
/// # Errors
/// An `invalid_request` failure where it holds no such file, or the call is of no conversation.
pub(crate) fn file_bytes(call: &Call, reference: &str) -> Result<bytes::Bytes, Failure> {
    let files = call
        .files
        .as_ref()
        .ok_or_else(|| Failure::new(FailureKind::InvalidRequest, format!("the file {reference} is of no conversation")))?;
    files
        .read(reference)
        .map_err(|missing| Failure::new(FailureKind::InvalidRequest, missing.to_string()))
}

/// A file's bytes as a data URL, as a request carries a file inline.
///
/// # Errors
/// An `invalid_request` failure where the host has no such file.
pub(crate) fn data_url(call: &Call, file: &FileRef) -> Result<String, Failure> {
    let bytes = file_bytes(call, &file.reference)?;
    Ok(format!(
        "data:{};base64,{}",
        file.media_type,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Whether a file of `media_type` is text, which a provider that takes no text files is sent as text.
#[must_use]
pub fn is_text(media_type: &str) -> bool {
    media_type.starts_with("text/") || matches!(media_type, "application/json" | "application/xml" | "application/x-yaml")
}

/// The text of a text file the host keeps, as the person's text: inside `<file>` with its name.
///
/// # Errors
/// An `invalid_request` failure where the host has no such file.
pub(crate) fn file_as_text(call: &Call, file: &FileRef) -> Result<String, Failure> {
    let bytes = file_bytes(call, &file.reference)?;
    Ok(file_text(&String::from_utf8_lossy(&bytes), file.filename.as_deref()))
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

/// A system reminder — what the host tells the model after the start — inside `<system-reminder>`, for a model that
/// takes it as the person's text: escaped so it cannot close its tag.
#[must_use]
pub fn system_reminder(words: &str) -> String {
    format!("<system-reminder>\n{}\n</system-reminder>", escaped(words, "</system-reminder"))
}

/// A text file's text inside `<file>`, with its name where it has one, for a model that takes no text files: escaped
/// so it cannot close its tag.
#[must_use]
pub fn file_text(text: &str, name: Option<&str>) -> String {
    let named = name.map_or_else(String::new, |name| {
        format!(
            " name=\"{}\"",
            name.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;")
        )
    });
    format!("<file{named}>\n{}\n</file>", escaped(text, "</file"))
}

/// `text` with each closing `tag`, in any case, begun `&lt;/` in place of `</`.
fn escaped(text: &str, tag: &str) -> String {
    let lowered = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lowered[at..].find(tag) {
        let start = at + found;
        out.push_str(&text[at..start]);
        out.push_str("&lt;/");
        out.push_str(&text[start + 2..start + tag.len()]);
        at = start + tag.len();
    }
    out.push_str(&text[at..]);
    out
}
