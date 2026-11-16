//! Local image references become part of the page, rather than paths the isolated browser would have to read.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use regex::Regex;
use siphon::machine::Machine;
use siphon::threads::files;

use crate::Error;

// Like t3code, recognize quoted image filenames (including JS strings) and unquoted CSS url(...), without
// reserializing the HTML or trying to bundle arbitrary scripts and stylesheets.
static REFERENCES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r#"(?i)"([^"\r\n]*\.(?:png|jpe?g|gif|webp|avif|svg|bmp|ico)(?:[?#][^"\r\n]*)?)""#,
        r#"|'([^'\r\n]*\.(?:png|jpe?g|gif|webp|avif|svg|bmp|ico)(?:[?#][^'\r\n]*)?)'"#,
        r#"|`([^`\r\n]*\.(?:png|jpe?g|gif|webp|avif|svg|bmp|ico)(?:[?#][^`\r\n]*)?)`"#,
        r#"|url\(\s*([^\s"'`()]+\.(?:png|jpe?g|gif|webp|avif|svg|bmp|ico)(?:[?#][^\s"'`()]+)?)\s*\)"#,
    ))
    .expect("the local image reference pattern")
});

/// Embeds quoted local image filenames and CSS `url(...)` references into HTML. `path` is the HTML file's namespace
/// path, as `Machine::read_file` returns it; relative images start in its folder. HTTP, HTTPS, data and other URL
/// schemes stay unchanged. Every image is read through the same machine and checked by its bytes.
///
/// # Errors
/// An inaccessible, non-regular or invalid local image, or a page larger than 32 MiB with its images embedded.
pub fn inline_local_images(machine: &Machine, path: &str, html: &str) -> Result<String, Error> {
    if html.len() > files::BYTES_MAX {
        return Err(Error::TooLarge);
    }
    let source = Path::new("/").join(path);
    let folder = source.parent().unwrap_or(Path::new("/"));
    let mut images = BTreeMap::new();
    let mut prepared = String::with_capacity(html.len());
    let mut size = html.len();
    let mut cursor = 0;
    for captures in REFERENCES.captures_iter(html) {
        let Some(reference) = captures.iter().skip(1).flatten().next() else {
            continue;
        };
        let Some((local, fragment)) = local_image(reference.as_str()) else {
            continue;
        };
        let path = if local.starts_with("~/") {
            machine.path(local)
        } else {
            machine.path(&folder.join(local).to_string_lossy())
        };
        if !images.contains_key(&path) {
            let (_, bytes) = machine
                .read_file(&path, files::BYTES_MAX as u64)
                .map_err(|source| Error::ImageRead {
                    path: local.to_owned(),
                    source,
                })?;
            let media_type = image_type(&bytes).ok_or_else(|| Error::NotImage(local.to_owned()))?;
            let encoded_len = bytes.len().div_ceil(3) * 4;
            let prefix = format!("data:{media_type};base64,");
            if size - reference.len() + prefix.len() + encoded_len + fragment.len() > files::BYTES_MAX {
                return Err(Error::TooLarge);
            }
            images.insert(path.clone(), prefix + &STANDARD.encode(bytes));
        }
        let data_uri = &images[&path];
        size = size - reference.len() + data_uri.len() + fragment.len();
        if size > files::BYTES_MAX {
            return Err(Error::TooLarge);
        }
        prepared.push_str(&html[cursor..reference.start()]);
        prepared.push_str(data_uri);
        prepared.push_str(fragment);
        cursor = reference.end();
    }
    prepared.push_str(&html[cursor..]);
    Ok(prepared)
}

fn local_image(reference: &str) -> Option<(&str, &str)> {
    if reference.starts_with("//") {
        return None;
    }
    let (path, fragment) = reference.find('#').map_or((reference, ""), |at| reference.split_at(at));
    let path = path.split('?').next()?;
    (!path.is_empty() && !path.contains(':')).then_some((path, fragment))
}

// Never trust an extension: a renamed credential file must not become a data URI. The MIME also follows the bytes.
fn image_type(bytes: &[u8]) -> Option<&'static str> {
    if let Some(kind) = siphon::machine::media_type(bytes).filter(|kind| kind.starts_with("image/")) {
        Some(kind)
    } else if bytes.starts_with(b"\0\0\x01\0") {
        Some("image/x-icon")
    } else if bytes.starts_with(b"BM") && bytes.get(6..10) == Some(b"\0\0\0\0") {
        Some("image/bmp")
    } else if bytes.get(4..8) == Some(b"ftyp") && matches!(bytes.get(8..12), Some(b"avif" | b"avis")) {
        Some("image/avif")
    } else if has_svg_root(bytes) {
        Some("image/svg+xml")
    } else {
        None
    }
}

fn has_svg_root(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else { return false };
    let mut rest = text.trim_prefix('\u{feff}').trim_start();
    loop {
        if let Some(instruction) = rest.strip_prefix("<?") {
            let Some((_, after)) = instruction.split_once("?>") else {
                return false;
            };
            rest = after.trim_start();
        } else if let Some(comment) = rest.strip_prefix("<!--") {
            let Some((_, after)) = comment.split_once("-->") else {
                return false;
            };
            rest = after.trim_start();
        } else if rest.get(..9).is_some_and(|prefix| prefix.eq_ignore_ascii_case("<!doctype")) {
            let Some(after) = after_doctype(&rest[9..]) else { return false };
            rest = after.trim_start();
        } else {
            return rest
                .strip_prefix("<svg")
                .and_then(|root| root.as_bytes().first())
                .is_some_and(|byte| byte.is_ascii_whitespace() || matches!(byte, b'/' | b'>'));
        }
    }
}

fn after_doctype(text: &str) -> Option<&str> {
    let mut quote = None;
    let mut subset = 0_u32;
    let mut at = 0;
    while let Some(&byte) = text.as_bytes().get(at) {
        if quote.is_none() && subset > 0 {
            let rest = text.get(at..).unwrap_or_default();
            if let Some(comment) = rest.strip_prefix("<!--") {
                at += 4 + comment.find("-->")? + 3;
                continue;
            }
            if let Some(instruction) = rest.strip_prefix("<?") {
                at += 2 + instruction.find("?>")? + 2;
                continue;
            }
        }
        if quote == Some(byte) {
            quote = None;
        } else if quote.is_none() {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'[' => subset += 1,
                b']' => subset = subset.saturating_sub(1),
                b'>' if subset == 0 => return Some(&text[at + 1..]),
                _ => {}
            }
        }
        at += 1;
    }
    None
}
