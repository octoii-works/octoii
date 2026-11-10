//! A text file's bytes as text, and text as the bytes of a file: text is UTF-8 with no NUL, its byte-order mark apart
//! from it; new text that replaces a file's is written as the file held its own — its mark kept, and its lines ending
//! `\r\n` where the file's first line did.

use std::borrow::Cow;

pub const BYTE_ORDER_MARK: &[u8] = b"\xef\xbb\xbf";

/// A file's text, and whether its bytes began with a byte-order mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Text<'a> {
    pub text: &'a str,
    pub marked: bool,
}

impl<'a> Text<'a> {
    /// The text `bytes` hold; none where they hold a NUL or are no UTF-8.
    #[must_use]
    pub fn of(bytes: &'a [u8]) -> Option<Self> {
        let (marked, rest) = match bytes.strip_prefix(BYTE_ORDER_MARK) {
            Some(rest) => (true, rest),
            None => (false, bytes),
        };
        let text = std::str::from_utf8(rest).ok().filter(|text| !text.contains('\0'))?;
        Some(Self { text, marked })
    }

    /// The bytes `text` is written as in this file: its mark first where it had one.
    #[must_use]
    pub fn encode(&self, text: &str) -> Vec<u8> {
        let mark: &[u8] = if self.marked { BYTE_ORDER_MARK } else { &[] };
        [mark, text.as_bytes()].concat()
    }

    /// `content` as it replaces this file's text: its lines ending `\r\n` where the file's first line did.
    #[must_use]
    pub fn with_line_endings<'c>(&self, content: &'c str) -> Cow<'c, str> {
        let first_line_crlf = self.text.find('\n').is_some_and(|newline| self.text[..newline].ends_with('\r'));
        if !first_line_crlf {
            return Cow::Borrowed(content);
        }
        let mut out = String::with_capacity(content.len() + content.len() / 32);
        let mut previous = None;
        for character in content.chars() {
            if character == '\n' && previous != Some('\r') {
                out.push('\r');
            }
            out.push(character);
            previous = Some(character);
        }
        Cow::Owned(out)
    }
}
