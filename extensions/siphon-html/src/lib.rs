//! One HTML preview document for the person's interface and the browser that checks the result. The source is read
//! through Siphon's machine, without changing it; the document prepares its isolated inner frame in one place.

mod images;

use siphon::machine::Machine;
use siphon::threads::files;

pub use images::inline_local_images;

/// The trusted preview host. Load it as its own document, never through a parent document's `srcdoc` or Blob: those
/// inherit the parent's CSP, which may forbid the inline scripts a self-contained HTML file uses.
///
/// The host announces `octoii.html.ready` and accepts `octoii.html.load` with HTML and an optional theme only from
/// its parent. A theme contains `appearance` (light or dark) and CSS `variables`; `octoii.html.theme` updates it without
/// reloading the document. Without a theme, defaults follow the system. The host then announces `octoii.html.loaded` and `{ type: "octoii.html.size", height: number }`. Sizes are display information,
/// not trusted commands. An embedded host must itself use `sandbox="allow-scripts"`, without `allow-same-origin`.
pub const DOCUMENT: &str = include_str!("../assets/preview.html");

/// Why an HTML source could not be prepared.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Read(#[from] siphon::machine::Error),
    #[error("HTML preview needs a UTF-8 .html or .htm file without NUL bytes")]
    NotHtml,
    #[error("local image {path}: {source}")]
    ImageRead { path: String, source: siphon::machine::Error },
    #[error("local image {0} does not contain supported image bytes")]
    NotImage(String),
    #[error("HTML with its local images must fit within 32 MiB; use smaller images")]
    TooLarge,
}

/// Reads a regular HTML file through `machine` and embeds its local images, up to 32 MiB in total. The source file
/// stays unchanged. The shared preview document adds its CSP and measurements when it receives this text.
///
/// # Errors
/// An inaccessible or non-regular file, invalid HTML text or local image, or a page past the byte limit.
pub fn prepare(machine: &Machine, path: &str) -> Result<String, Error> {
    if path.is_empty() || path.contains('\0') {
        return Err(siphon::machine::Error::InvalidPath.into());
    }
    let (name, bytes) = machine.read_file(&machine.path(path), files::BYTES_MAX as u64)?;
    if files::media_type(&name, &bytes) != "text/html" {
        return Err(Error::NotHtml);
    }
    let html = std::str::from_utf8(&bytes).map_err(|_| Error::NotHtml)?;
    inline_local_images(machine, &name, html)
}
