//! Reports HTTP loopback origins from a terminal's own output. The terminal keeps the addresses while it runs.

use std::sync::LazyLock;

use regex::Regex;
use url::{Host, Url};

const LINE_LIMIT: usize = 4096;
const ADDRESS_LIMIT: usize = 12;

static LINKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)https?://[^\s\x00-\x1f<>"'`]+"#).expect("a static URL pattern"));

#[derive(Debug, Default)]
pub(crate) struct Addresses {
    pub(crate) origins: Vec<String>,
    line: Vec<u8>,
}

impl Addresses {
    /// Output can split a URL. Read it once the terminal finishes the line.
    pub(crate) fn read(&mut self, output: &[u8]) -> bool {
        let mut changed = false;
        for piece in output.split_inclusive(|byte| matches!(byte, b'\r' | b'\n')) {
            let remaining = (LINE_LIMIT + 1).saturating_sub(self.line.len());
            self.line.extend_from_slice(&piece[..piece.len().min(remaining)]);
            if piece.last().is_some_and(|byte| matches!(byte, b'\r' | b'\n')) {
                changed |= self.report();
                self.line.clear();
            }
        }
        changed
    }

    pub(crate) fn gap(&mut self) {
        self.line.clear();
    }

    fn report(&mut self) -> bool {
        if self.line.len() > LINE_LIMIT {
            return false;
        }
        let text = String::from_utf8_lossy(&self.line);
        let mut changed = false;
        for address in LINKS.find_iter(&text).filter_map(|found| loopback_origin(found.as_str())) {
            if self.origins.contains(&address) {
                continue;
            }
            if self.origins.len() == ADDRESS_LIMIT {
                self.origins.remove(0);
            }
            self.origins.push(address);
            changed = true;
        }
        changed
    }
}

fn loopback_origin(text: &str) -> Option<String> {
    let url = Url::parse(text.trim_end_matches(['.', ',', ';', ')', '!', '}'])).ok()?;
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    match url.host()? {
        Host::Domain("localhost") => {}
        Host::Ipv4(ip) if ip.is_loopback() => {}
        Host::Ipv6(ip) if ip.is_loopback() => {}
        _ => return None,
    }
    Some(url.origin().ascii_serialization())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_only_loopback_origins_even_when_output_splits_a_link() {
        let mut addresses = Addresses::default();
        assert!(!addresses.read(b"Local: \x1b[32mhttp://local"));
        assert!(addresses.read(b"host:5173/path?token=fixture\x1b[0m\r\nhttps://localhost.invalid:1234\r\n"));
        assert!(!addresses.read(b"Duplicate (http://localhost:5173/)\r\n"));
        assert!(addresses.read(b"Listening: http://[::1]:9373/\r\n"));
        assert_eq!(addresses.origins, ["http://localhost:5173", "http://[::1]:9373"]);
    }
}
