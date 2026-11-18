//! An edit of a text: `old` found in it and replaced with `new`, once or at every match. `old` is found on the text
//! read with each `\r\n` as `\n`, by the first of three tiers that finds it — exactly; with typographic characters read
//! as their plain ones; line by line, trailing spaces and tabs left aside — and each match is replaced by `new`'s
//! lines, each ending as the line it stands for did, a line the edit keeps written as the file wrote it. Where `old` and
//! `new` differ in their `\r` alone, the edit is of line endings: matched and replaced as written. An edit that cannot be
//! made as asked is refused, with why.

use similar::{Algorithm, DiffOp};

use crate::nearest::{Nearest, nearest};

/// How an edit found what it replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Matching {
    Exact,
    Typographic,
    Lines,
}

impl Matching {
    /// The matching by its name, as a call's result tells it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Typographic => "typographic",
            Self::Lines => "lines",
        }
    }
}

/// An edit: what to find, what to put in its place, and whether at every match.
#[derive(Debug, Clone, Copy)]
pub struct Edit<'a> {
    pub old: &'a str,
    pub new: &'a str,
    pub all: bool,
}

/// Why an edit is refused, in the words a failure tells it in: `old` empty, or the same as `new`; no match, with where
/// the text came nearest; more than one, not asked to replace every one; or a text the edit leaves as it was.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("`old` is empty")]
    Empty,
    #[error("`new` is the same as `old`")]
    Same,
    #[error("`old` is nowhere in the file")]
    NotFound(Nearest),
    #[error("`old` matches {0} places: give more of it, or set `all`")]
    Many(usize),
    #[error("the edit would leave the file as it is")]
    Unchanged,
}

/// What an edit made of a text: the text after, how many matches it replaced and by which matching it found them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    pub text: String,
    pub replacements: usize,
    pub matching: Matching,
}

/// A place in a text: from its start to its end, in bytes.
#[derive(Debug, Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
}

impl Edit<'_> {
    /// `text` with this edit made.
    ///
    /// # Errors
    /// The `Refusal` of an edit that cannot be made as asked.
    pub fn apply(&self, text: &str) -> Result<Edited, Refusal> {
        if self.old.is_empty() {
            return Err(Refusal::Empty);
        }
        if self.old == self.new {
            return Err(Refusal::Same);
        }
        if self.old.replace('\r', "") == self.new.replace('\r', "") {
            let spans = occurrences(text, self.old);
            self.check(text, spans.len())?;
            return made(text, replaced_as_written(text, &spans, self.new), spans.len(), Matching::Exact);
        }
        let old = lf_view(self.old).text;
        let view = lf_view(text);
        let (spans, matching) = matched(&view.text, &old);
        self.check(text, spans.len())?;
        let after = replaced(text, &view, &spans, &old, &lf_view(self.new).text);
        made(text, after, spans.len(), matching)
    }
}

impl Edit<'_> {
    /// Whether `matches` matches of `old` in `text` are what the edit asks: none refused, with where `text` came nearest;
    /// more than one refused where every match is not asked for.
    fn check(&self, text: &str, matches: usize) -> Result<(), Refusal> {
        match matches {
            0 => Err(Refusal::NotFound(nearest(text, self.old, self.new))),
            1 => Ok(()),
            many if !self.all => Err(Refusal::Many(many)),
            _ => Ok(()),
        }
    }
}

fn made(before: &str, after: String, replacements: usize, matching: Matching) -> Result<Edited, Refusal> {
    if before == after {
        return Err(Refusal::Unchanged);
    }
    Ok(Edited {
        text: after,
        replacements,
        matching,
    })
}

/// Where `old` matches `view`, by the first matching that finds it.
fn matched(view: &str, old: &str) -> (Vec<Span>, Matching) {
    let exact = occurrences(view, old);
    if !exact.is_empty() {
        return (exact, Matching::Exact);
    }
    let plain_view = plain_view(view);
    let plain: Vec<Span> = occurrences(&plain_view.text, &plain_view_of(old))
        .into_iter()
        .map(|span| Span {
            start: plain_view.at[span.start],
            end: plain_view.at[span.end],
        })
        .collect();
    if !plain.is_empty() {
        return (plain, Matching::Typographic);
    }
    (line_spans(view, old), Matching::Lines)
}

/// Where `needle` occurs in `text`, left to right, none overlapping the one before.
fn occurrences(text: &str, needle: &str) -> Vec<Span> {
    text.match_indices(needle)
        .map(|(start, found)| Span {
            start,
            end: start + found.len(),
        })
        .collect()
}

fn replaced_as_written(text: &str, spans: &[Span], new: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for span in spans {
        out.push_str(&text[at..span.start]);
        out.push_str(new);
        at = span.end;
    }
    out.push_str(&text[at..]);
    out
}

/// A text read another way, and where each of its bytes, and its end, stands in the text it was read from.
#[derive(Debug)]
struct View {
    text: String,
    at: Vec<usize>,
}

/// `text` read with each `\r\n` as `\n`, that `\n` standing where its `\r` does: a span that starts or ends there takes
/// or leaves the pair whole.
fn lf_view(text: &str) -> View {
    let bytes = text.as_bytes();
    let mut read = Vec::with_capacity(bytes.len());
    let mut at = Vec::with_capacity(bytes.len() + 1);
    let mut index = 0;
    while index < bytes.len() {
        at.push(index);
        if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
            index += 1;
        }
        read.push(bytes[index]);
        index += 1;
    }
    at.push(bytes.len());
    View {
        text: String::from_utf8(read).expect("a pair left out of UTF-8 leaves UTF-8"),
        at,
    }
}

/// `text` read with each typographic character as its plain one: curly quotes as straight ones, Unicode dashes and the
/// minus sign as `-`, special spaces as a space.
fn plain_view(text: &str) -> View {
    let mut read = String::with_capacity(text.len());
    let mut at = Vec::with_capacity(text.len() + 1);
    for (index, character) in text.char_indices() {
        if let Some(plain) = plain_of(character) {
            at.push(index);
            read.push(plain);
        } else {
            at.extend(index..index + character.len_utf8());
            read.push(character);
        }
    }
    at.push(text.len());
    View { text: read, at }
}

fn plain_view_of(text: &str) -> String {
    text.chars().map(|character| plain_of(character).unwrap_or(character)).collect()
}

/// The plain character a typographic one stands for; none for another.
fn plain_of(character: char) -> Option<char> {
    match character {
        '\u{2018}'..='\u{201B}' => Some('\''),
        '\u{201C}'..='\u{201F}' => Some('"'),
        '\u{2010}'..='\u{2015}' | '\u{2212}' => Some('-'),
        '\u{00A0}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' => Some(' '),
        _ => None,
    }
}

/// A line as the matching by lines reads it: typographic characters as their plain ones, trailing spaces and tabs left aside.
fn loose(line: &str) -> String {
    let mut plain = plain_view_of(line);
    plain.truncate(plain.trim_end_matches([' ', '\t']).len());
    plain
}

/// A text split at `\n` into its lines, and whether it ends with one — the empty line after it left out.
fn split(text: &str) -> (Vec<&str>, bool) {
    if text.is_empty() {
        return (Vec::new(), false);
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    let ends_with_newline = lines.last() == Some(&"");
    if ends_with_newline {
        lines.pop();
    }
    (lines, ends_with_newline)
}

/// The spans of whole lines of `view` that match `old`'s lines loosely, left to right, none overlapping the one before;
/// where `old` ends with a newline, a match takes the newline after its last line too.
fn line_spans(view: &str, old: &str) -> Vec<Span> {
    let (wanted, ends_with_newline) = split(old);
    if wanted.is_empty() {
        return Vec::new();
    }
    let wanted: Vec<String> = wanted.into_iter().map(loose).collect();
    let mut lines = Vec::new();
    let mut offset = 0;
    for line in view.split('\n') {
        lines.push((offset, line, loose(line)));
        offset += line.len() + 1;
    }
    // A match that takes the newline after its last line ends before the view's last line, which has none.
    let matchable = if ends_with_newline { lines.len() - 1 } else { lines.len() };
    let mut out = Vec::new();
    let mut at = 0;
    while at + wanted.len() <= matchable {
        let found = &lines[at..at + wanted.len()];
        if !found.iter().zip(&wanted).all(|((_, _, line), wanted)| line == wanted) {
            at += 1;
            continue;
        }
        let (last_start, last, _) = found[found.len() - 1];
        out.push(Span {
            start: found[0].0,
            end: last_start + last.len() + usize::from(ends_with_newline),
        });
        at += wanted.len();
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    None,
    Lf,
    Crlf,
}

impl Ending {
    fn text(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
        }
    }
}

/// A line as a text wrote it: its text and its own ending.
#[derive(Debug, Clone, Copy)]
struct Written<'a> {
    text: &'a str,
    ending: Ending,
}

/// A text's lines with their own endings; a last line with none only where text follows the last newline.
fn written_lines(text: &str) -> Vec<Written<'_>> {
    text.split_inclusive('\n')
        .map(|line| match line.strip_suffix('\n') {
            Some(line) => match line.strip_suffix('\r') {
                Some(line) => Written {
                    text: line,
                    ending: Ending::Crlf,
                },
                None => Written {
                    text: line,
                    ending: Ending::Lf,
                },
            },
            None => Written {
                text: line,
                ending: Ending::None,
            },
        })
        .collect()
}

/// The ending more of `lines` have than the other; none where neither has one, or as many have each.
fn prevailing(lines: &[Written<'_>]) -> Option<Ending> {
    let crlf = lines.iter().filter(|line| line.ending == Ending::Crlf).count();
    let lf = lines.iter().filter(|line| line.ending == Ending::Lf).count();
    match crlf.cmp(&lf) {
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => Some(Ending::Crlf),
        std::cmp::Ordering::Less => Some(Ending::Lf),
    }
}

/// A text's own ending: the one most of its lines have, else its first line's, else `\n`.
fn text_ending(lines: &[Written<'_>]) -> Ending {
    prevailing(lines)
        .or_else(|| lines.iter().map(|line| line.ending).find(|ending| *ending != Ending::None))
        .unwrap_or(Ending::Lf)
}

/// `text` with the place each span of `view` stands for replaced by `new`'s lines, each ending as the place's lines do.
fn replaced(text: &str, view: &View, spans: &[Span], old: &str, new: &str) -> String {
    let ending = text_ending(&written_lines(text));
    let (old_lines, _) = split(old);
    let (lines, ends_with_newline) = split(new);
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for span in spans {
        let (start, end) = (view.at[span.start], view.at[span.end]);
        out.push_str(&text[at..start]);
        let place = written_lines(&text[start..end]);
        write_replacement(&mut out, &place, &old_lines, (&lines, ends_with_newline), ending);
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

/// Where a line of the replacement comes from: a line of `old` it keeps, one it changes in place, or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    Kept(usize),
    Changed(usize),
    Added,
}

/// Writes the lines that replace a place, each with its ending from the place: a line kept keeps its own ending — and
/// the place's bytes where the replacement gives it as `old` did; a line changed in place takes the ending of the line
/// it stands for; a line added takes the ending most of the place's lines have, else the text's own.
fn write_replacement(
    out: &mut String,
    place: &[Written<'_>],
    old: &[&str],
    (lines, ends_with_newline): (&[&str], bool),
    text_ending: Ending,
) {
    let prevailing = prevailing(place).unwrap_or(text_ending);
    for (index, (line, origin)) in lines.iter().zip(origins(old, lines)).enumerate() {
        let from = match origin {
            Origin::Added => None,
            Origin::Kept(at) | Origin::Changed(at) => place.get(at),
        };
        let as_written = matches!(origin, Origin::Kept(at) if old[at] == *line);
        out.push_str(match (as_written, from) {
            (true, Some(from)) => from.text,
            _ => line,
        });
        if index == lines.len() - 1 && !ends_with_newline {
            continue;
        }
        let own = from.map_or(Ending::None, |from| from.ending);
        out.push_str(if own == Ending::None { prevailing.text() } else { own.text() });
    }
}

/// Where each of `updated`'s lines comes from: the lines of `old` it keeps, read loosely, and in each change those it
/// puts in place of others, one for one.
fn origins(old: &[&str], updated: &[&str]) -> Vec<Origin> {
    let mut out = Vec::with_capacity(updated.len());
    let (mut change_start, mut removed, mut added) = (0, 0, 0);
    for op in similar::capture_diff_slices_by_key(Algorithm::Myers, old, updated, |line| loose(line)) {
        match op {
            DiffOp::Equal { old_index, len, .. } => {
                flush(&mut out, change_start, removed, added);
                out.extend((old_index..old_index + len).map(Origin::Kept));
                (change_start, removed, added) = (old_index + len, 0, 0);
            }
            DiffOp::Delete { old_len, .. } => removed += old_len,
            DiffOp::Insert { new_len, .. } => added += new_len,
            DiffOp::Replace { old_len, new_len, .. } => {
                removed += old_len;
                added += new_len;
            }
        }
    }
    flush(&mut out, change_start, removed, added);
    out
}

/// The origins of the lines a change added: those in place of the lines it removed changed, one for one, from
/// `change_start`, the rest added.
fn flush(out: &mut Vec<Origin>, change_start: usize, removed: usize, added: usize) {
    out.extend((0..added).map(|at| {
        if at < removed {
            Origin::Changed(change_start + at)
        } else {
            Origin::Added
        }
    }));
}
