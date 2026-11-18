//! Where a text came nearest to holding what an edit looked for in vain: `new` in it already, as where the edit was
//! made before; else the lines that match the most of `old`'s, line for line with the spaces around them aside, or, where
//! none does, the line that begins most like `old`'s first. They are given as many as `old` has, at most twelve, each
//! cut at two hundred characters, as the text holds them, so they can be given back as `old`.

/// The most lines given.
const LINES_MAX: usize = 12;
/// The most characters of a line given.
const LINE_CHARS_MAX: usize = 200;
/// The fewest characters of `new`, its spaces aside, that are taken as the edit made where the text holds them: a
/// shorter one is in many texts by chance.
const MADE_CHARS_MIN: usize = 20;
/// The fewest characters a line must begin like `old`'s first with to be the nearest by its start.
const PREFIX_CHARS_MIN: usize = 8;

/// Where a text came nearest to an edit's `old`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nearest {
    /// `new` is in the text already.
    Made,
    /// The lines from the line `first`, from 1.
    Lines { first: usize, lines: Vec<String> },
    /// Nothing in the text is near.
    Nowhere,
}

/// Where `text` came nearest to holding `old`, which it does not hold, for an edit that would put `new` in its place.
#[must_use]
pub fn nearest(text: &str, old: &str, new: &str) -> Nearest {
    let new = new.replace('\r', "");
    if new.trim().chars().count() >= MADE_CHARS_MIN && text.replace('\r', "").contains(&new) {
        return Nearest::Made;
    }
    let lines: Vec<&str> = text.lines().collect();
    let wanted: Vec<&str> = old.lines().collect();
    let Some(first) = by_lines(&lines, &wanted).or_else(|| by_start(&lines, &wanted)) else {
        return Nearest::Nowhere;
    };
    let count = wanted.len().clamp(1, LINES_MAX);
    Nearest::Lines {
        first: first + 1,
        lines: lines[first..]
            .iter()
            .take(count)
            .map(|line| line.chars().take(LINE_CHARS_MAX).collect())
            .collect(),
    }
}

/// The index of the line from which the most of `wanted`'s lines match, each with the one it stands for, its spaces
/// aside; none where no line but a blank one matches.
fn by_lines(lines: &[&str], wanted: &[&str]) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None;
    for start in 0..lines.len() {
        let matches = wanted
            .iter()
            .zip(&lines[start..])
            .filter(|(want, line)| !want.trim().is_empty() && want.trim() == line.trim())
            .count();
        if matches > best.map_or(0, |(_, most)| most) {
            best = Some((start, matches));
        }
    }
    best.map(|(start, _)| start)
}

/// The index of the line that begins most like `wanted`'s first line that is not blank, its spaces aside; none where
/// none begins so for long enough.
fn by_start(lines: &[&str], wanted: &[&str]) -> Option<usize> {
    let want = wanted.iter().map(|line| line.trim()).find(|line| !line.is_empty())?;
    let shared = |line: &str| line.trim().chars().zip(want.chars()).take_while(|(a, b)| a == b).count();
    let (start, length) = lines
        .iter()
        .enumerate()
        .map(|(at, line)| (at, shared(line)))
        .max_by_key(|&(at, length)| (length, usize::MAX - at))?;
    (length >= PREFIX_CHARS_MIN).then_some(start)
}
