//! One HTML preview document for the person's interface and the browser that checks the result. The source is read
//! through Siphon's machine, without changing it; the document prepares its isolated inner frame in one place.

use siphon::machine::Machine;
use siphon::threads::files;

/// The trusted preview host. Load it as its own document, never through a parent document's `srcdoc` or Blob: those
/// inherit the parent's CSP, which may forbid the inline scripts a self-contained HTML file uses.
///
/// The host announces `octoii.html.ready`, accepts `{ type: "octoii.html.load", html: string }` only from its parent,
/// then announces `octoii.html.loaded` and `{ type: "octoii.html.size", height: number }`. Sizes are display information,
/// not trusted commands. An embedded host must itself use `sandbox="allow-scripts"`, without `allow-same-origin`.
pub const DOCUMENT: &str = include_str!("../assets/preview.html");

/// Why an HTML source could not be prepared.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Read(#[from] siphon::machine::Error),
    #[error("HTML preview needs a UTF-8 .html or .htm file without NUL bytes")]
    NotHtml,
}

/// Reads a regular HTML file through `machine`, up to 32 MiB, preserving the source text exactly. The preview
/// document, shared by both viewers, adds its CSP and measurements when it receives this text.
///
/// # Errors
/// An inaccessible or non-regular file, a file past the byte limit, or a file that is not HTML text.
pub fn prepare(machine: &Machine, path: &str) -> Result<String, Error> {
    if path.is_empty() || path.contains('\0') {
        return Err(siphon::machine::Error::InvalidPath.into());
    }
    let (name, bytes) = machine.read_file(&machine.path(path), files::BYTES_MAX as u64)?;
    if files::media_type(&name, &bytes) != "text/html" {
        return Err(Error::NotHtml);
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| Error::NotHtml)
}
