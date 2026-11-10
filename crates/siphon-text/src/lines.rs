//! A text's lines as a page shows them, gathered from its bytes a range at a time as they are read, till the page is
//! whole and the line after it seen: each line its number, from 1, its text without its ending, each stretch of bytes
//! that is no UTF-8 shown as one U+FFFD — as Unicode's practice for a decoder replaces a maximal subpart — cut past as
//! many characters as a line shows; a NUL among the bytes telling that they are no text.

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

/// A page of a text's lines, taken a range of its bytes at a time.
#[derive(Debug)]
pub struct Pager {
    bounds: Bounds,
    lines: Vec<Line>,
    /// The bytes the page's lines take.
    bytes: usize,
    /// The lines seen.
    seen: usize,
    /// Whether a line past the page was seen.
    more: bool,
    /// Whether a NUL was among the bytes: no text.
    binary: bool,
    /// The line being gathered, kept to as many bytes as can show its characters, and more, to tell it was cut.
    gathering: Vec<u8>,
    /// Whether a line is begun past the last newline taken.
    begun: bool,
}

impl Pager {
    #[must_use]
    pub fn new(bounds: Bounds) -> Self {
        Self {
            bounds,
            lines: Vec::new(),
            bytes: 0,
            seen: 0,
            more: false,
            binary: false,
            gathering: Vec::new(),
            begun: false,
        }
    }

    /// Takes the next range of the text; whether the page wants more.
    pub fn take(&mut self, range: &[u8]) -> bool {
        if range.contains(&0) {
            self.binary = true;
            return false;
        }
        let mut pieces = range.split(|byte| *byte == b'\n').peekable();
        while let Some(piece) = pieces.next() {
            self.gather(piece);
            if pieces.peek().is_none() {
                // What follows the last newline is a line begun, ended by the next range or the text's end; a range
                // with no newline goes on with the line the ranges before began.
                self.begun |= !piece.is_empty();
                return true;
            }
            if !self.end() {
                return false;
            }
        }
        true
    }

    /// Takes the end of the text: the line begun past its last newline, where one is.
    pub fn finish(&mut self) {
        if self.begun {
            self.end();
        }
    }

    /// Whether the page reached the text's end: no line past it seen.
    #[must_use]
    pub fn whole(&self) -> bool {
        !self.more
    }

    /// Whether a NUL was among the bytes, so they are no text.
    #[must_use]
    pub fn binary(&self) -> bool {
        self.binary
    }

    /// How many lines were seen, those before the page among them.
    #[must_use]
    pub fn seen(&self) -> usize {
        self.seen
    }

    /// The page's lines.
    #[must_use]
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// The page's lines, the pager let go of.
    #[must_use]
    pub fn into_lines(self) -> Vec<Line> {
        self.lines
    }

    fn gather(&mut self, piece: &[u8]) {
        let kept_max = self.bounds.line_chars_max * 4 + 8;
        let room = kept_max.saturating_sub(self.gathering.len());
        self.gathering.extend_from_slice(&piece[..piece.len().min(room)]);
    }

    /// Ends the line gathered: kept where the page has room for it; false once it is past the page.
    fn end(&mut self) -> bool {
        let gathered = std::mem::take(&mut self.gathering);
        self.begun = false;
        self.seen += 1;
        if self.seen < self.bounds.offset {
            return true;
        }
        let (text, cut) = shown(&gathered, self.bounds.line_chars_max);
        let bytes = digits(self.seen) + 2 + text.len() + 1;
        if self.lines.len() >= self.bounds.limit || self.bytes + bytes > self.bounds.bytes_max {
            self.more = true;
            return false;
        }
        self.lines.push(Line {
            number: self.seen,
            text,
            cut,
        });
        self.bytes += bytes;
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
