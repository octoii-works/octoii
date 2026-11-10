//! A page of a text's lines, read from where its bytes come: each line its number, from 1, its text without its ending,
//! each stretch of bytes that is no UTF-8 shown as one U+FFFD — as Unicode's practice for a decoder replaces a maximal
//! subpart — cut past as many characters as a line shows; a NUL among the bytes telling that they are no text. A line
//! past what it shows is read through, never kept whole.

use std::io::{self, BufRead};

/// A line as shown: its number, from 1; its text; and whether it was cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub number: usize,
    pub text: String,
    pub cut: bool,
}

/// What a page holds: from which line, from 1; how many lines at most; how many bytes they take at most, each as
/// `N: text` and a newline; and how many characters of a line it shows, past which the line is cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub offset: usize,
    pub limit: usize,
    pub bytes_max: usize,
    pub line_chars_max: usize,
}

impl Default for Bounds {
    fn default() -> Self {
        Self {
            offset: 1,
            limit: 2000,
            bytes_max: 48 * 1024,
            line_chars_max: 2000,
        }
    }
}

/// What a read of a page came to: the page of a text, or bytes that are no text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Paged {
    Text(Page),
    Binary,
}

/// A page of a text's lines: the lines, how many lines were seen — those before the page among them — and whether
/// the page reached the text's end, no line past it seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub lines: Vec<Line>,
    pub seen: usize,
    pub whole: bool,
}

impl Page {
    /// The page `bounds` take of the text `source` gives, read till the page is whole and the line after it seen.
    ///
    /// # Errors
    /// What reading `source` failed with.
    pub fn read(mut source: impl BufRead, bounds: &Bounds) -> io::Result<Paged> {
        let mut page = Self {
            lines: Vec::new(),
            seen: 0,
            whole: true,
        };
        let mut bytes = 0;
        // The line being read, kept to as many bytes as can show its characters, and more, to tell it was cut.
        let kept_max = bounds.line_chars_max * 4 + 8;
        let mut line = Vec::new();
        let mut begun = false;
        loop {
            let (used, ended) = {
                let chunk = source.fill_buf()?;
                if chunk.is_empty() {
                    break;
                }
                if chunk.contains(&0) {
                    return Ok(Paged::Binary);
                }
                let (piece, ended) = match chunk.iter().position(|byte| *byte == b'\n') {
                    Some(newline) => (&chunk[..newline], true),
                    None => (chunk, false),
                };
                let room = kept_max.saturating_sub(line.len());
                line.extend_from_slice(&piece[..piece.len().min(room)]);
                begun |= !piece.is_empty();
                (piece.len() + usize::from(ended), ended)
            };
            source.consume(used);
            if ended {
                begun = false;
                if !page.end(&mut line, &mut bytes, bounds) {
                    return Ok(Paged::Text(page));
                }
            }
        }
        if begun {
            page.end(&mut line, &mut bytes, bounds);
        }
        Ok(Paged::Text(page))
    }

    /// Ends the line read: kept where the page has room for it; false once it is past the page.
    fn end(&mut self, line: &mut Vec<u8>, bytes: &mut usize, bounds: &Bounds) -> bool {
        let raw = std::mem::take(line);
        self.seen += 1;
        if self.seen < bounds.offset {
            return true;
        }
        let (text, cut) = shown(&raw, bounds.line_chars_max);
        let taken = digits(self.seen) + 2 + text.len() + 1;
        if self.lines.len() >= bounds.limit || *bytes + taken > bounds.bytes_max {
            self.whole = false;
            return false;
        }
        self.lines.push(Line {
            number: self.seen,
            text,
            cut,
        });
        *bytes += taken;
        true
    }
}

/// A line as shown: its `\r` left out, each stretch of bytes that is no UTF-8 as U+FFFD, cut past `chars_max`
/// characters; and whether it was cut.
fn shown(raw: &[u8], chars_max: usize) -> (String, bool) {
    let line = raw.strip_suffix(b"\r").unwrap_or(raw);
    let mut text = String::new();
    let mut chars = 0;
    for chunk in line.utf8_chunks() {
        let replacement = (!chunk.invalid().is_empty()).then_some(char::REPLACEMENT_CHARACTER);
        for character in chunk.valid().chars().chain(replacement) {
            if chars == chars_max {
                return (text, true);
            }
            text.push(character);
            chars += 1;
        }
    }
    (text, false)
}

fn digits(number: usize) -> usize {
    number.checked_ilog10().map_or(1, |log| log as usize + 1)
}
