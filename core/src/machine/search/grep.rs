//! grep over a walk, as ripgrep searches — its own searcher and its regex syntax, which models know — each file read
//! through the namespace, a batch of files at a time on every core, the matches given in order by path a page at a
//! time: every page says whether more follow, so nothing is cut short unsaid.

use std::io;

use globset::{Glob, GlobMatcher};
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkContext, SinkMatch};
use rayon::prelude::*;

use crate::machine::files::Error;
use crate::machine::namespace::Machine;

/// How many files are searched at once, before the matches so far are counted.
const BATCH: usize = 256;

/// How a grep goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepOptions {
    /// The pattern as plain text rather than a regex.
    pub literal: bool,
    pub case_insensitive: bool,
    /// The lines shown around each match.
    pub context: usize,
    /// Only files whose paths match it: their names, for a glob of no `/`, as ripgrep's `-g` takes one.
    pub glob: Option<String>,
    /// The first matching line given, from 0, the files in order by path; and how many at most.
    pub offset: usize,
    pub limit: usize,
    /// How many characters of a line are shown; past them it is cut.
    pub line_chars_max: usize,
}

/// A line grep shows: its number, its text, whether it matched or stands around a match, and whether it was cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepLine {
    pub number: u64,
    pub text: String,
    pub matched: bool,
    pub cut: bool,
}

/// A file's lines grep shows, by the namespace's path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepFile {
    pub path: String,
    pub lines: Vec<GrepLine>,
}

/// What grep found: the files of the page, whether matches follow it, the files passed over as no text, and the
/// folders the walk was denied.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grepped {
    pub files: Vec<GrepFile>,
    pub more: bool,
    pub binary: usize,
    pub denied: Vec<String>,
}

/// Why a grep was refused.
#[derive(Debug, thiserror::Error)]
pub enum GrepError {
    #[error("the pattern: {0}")]
    Pattern(String),
    #[error("the glob: {0}")]
    Glob(String),
    #[error(transparent)]
    Files(#[from] Error),
}

/// What a file's search found: its lines, or that it holds no text.
enum Searched {
    Lines(Vec<GrepLine>),
    Binary,
}

/// What takes a file's lines as the searcher finds them.
struct Collected {
    lines: Vec<GrepLine>,
    chars_max: usize,
    binary: bool,
}

impl Collected {
    fn push(&mut self, number: Option<u64>, bytes: &[u8], matched: bool) {
        let text = String::from_utf8_lossy(bytes);
        let text = text.trim_end_matches(['\n', '\r']);
        let cut = text.chars().count() > self.chars_max;
        self.lines.push(GrepLine {
            number: number.unwrap_or_default(),
            text: if cut {
                text.chars().take(self.chars_max).collect()
            } else {
                text.to_owned()
            },
            matched,
            cut,
        });
    }
}

impl Sink for Collected {
    type Error = io::Error;

    fn matched(&mut self, _: &Searcher, found: &SinkMatch<'_>) -> Result<bool, io::Error> {
        self.push(found.line_number(), found.bytes(), true);
        Ok(true)
    }

    fn context(&mut self, _: &Searcher, around: &SinkContext<'_>) -> Result<bool, io::Error> {
        self.push(around.line_number(), around.bytes(), false);
        Ok(true)
    }

    fn binary_data(&mut self, _: &Searcher, _: u64) -> Result<bool, io::Error> {
        self.binary = true;
        Ok(false)
    }
}

/// The matcher of `pattern` as `options` say.
fn matcher(pattern: &str, options: &GrepOptions) -> Result<RegexMatcher, GrepError> {
    RegexMatcherBuilder::new()
        .case_insensitive(options.case_insensitive)
        .fixed_strings(options.literal)
        .build(pattern)
        .map_err(|error| GrepError::Pattern(error.to_string()))
}

/// Whether the file at `path`, under `start`, is one `glob` takes: its name, for a glob of no `/`, else its path below
/// `start`.
pub(crate) fn takes(glob: &GlobMatcher, whole: bool, start: &str, path: &str) -> bool {
    if whole {
        let below = path.strip_prefix(start).map_or(path, |rest| rest.trim_start_matches('/'));
        glob.is_match(below)
    } else {
        glob.is_match(path.rsplit('/').next().unwrap_or(path))
    }
}

impl Machine {
    /// Searches the files under the namespace's `start` — or the one file it names — for `pattern`, as `options` say.
    ///
    /// # Errors
    /// `Pattern` or `Glob` for one that is no pattern or glob; `NotFound` where nothing is there; the namespace's
    /// failure.
    pub fn grep(&self, start: &str, pattern: &str, options: &GrepOptions) -> Result<Grepped, GrepError> {
        let matcher = matcher(pattern, options)?;
        let glob = options
            .glob
            .as_deref()
            .map(|glob| Glob::new(glob).map(|built| (built.compile_matcher(), glob.contains('/'))))
            .transpose()
            .map_err(|error| GrepError::Glob(error.to_string()))?;
        let walked = self.walk(start)?;
        let files: Vec<&String> = walked
            .files
            .iter()
            .filter(|path| glob.as_ref().is_none_or(|(glob, whole)| takes(glob, *whole, start, path)))
            .collect();
        let mut grepped = Grepped {
            denied: walked.denied.clone(),
            ..Grepped::default()
        };
        let mut counted = 0;
        for batch in files.chunks(BATCH) {
            let searched: Vec<(&String, Searched)> = batch.par_iter().map(|path| (*path, self.search(path, &matcher, options))).collect();
            for (path, found) in searched {
                let lines = match found {
                    Searched::Binary => {
                        grepped.binary += 1;
                        continue;
                    }
                    Searched::Lines(lines) => lines,
                };
                let (taken, past) = page_of(lines, &mut counted, options);
                if !taken.is_empty() {
                    grepped.files.push(GrepFile {
                        path: path.clone(),
                        lines: taken,
                    });
                }
                if past {
                    grepped.more = true;
                    return Ok(grepped);
                }
            }
        }
        Ok(grepped)
    }

    /// The lines of the file at `path` that `matcher` matches, with their context.
    fn search(&self, path: &str, matcher: &RegexMatcher, options: &GrepOptions) -> Searched {
        let Ok(bytes) = self.namespace.read(path, 0, u64::MAX) else {
            return Searched::Lines(Vec::new());
        };
        let mut searcher = SearcherBuilder::new()
            .line_number(true)
            .before_context(options.context)
            .after_context(options.context)
            .binary_detection(BinaryDetection::quit(0))
            .build();
        let mut lines = Collected {
            lines: Vec::new(),
            chars_max: options.line_chars_max,
            binary: false,
        };
        if searcher.search_slice(matcher, &bytes, &mut lines).is_err() || lines.binary {
            return if lines.binary {
                Searched::Binary
            } else {
                Searched::Lines(Vec::new())
            };
        }
        Searched::Lines(lines.lines)
    }
}

/// The lines of a file's that fall in the page — its matches past those counted before `matches`, up to the page's
/// end, and the context lines within reach of them — and whether a match lies past the page; `counted` the matches
/// before this file's, and after it, this file's with them.
fn page_of(lines: Vec<GrepLine>, counted: &mut usize, options: &GrepOptions) -> (Vec<GrepLine>, bool) {
    let end = options.offset + options.limit;
    let mut taken_matches = Vec::new();
    let mut past = false;
    for line in lines.iter().filter(|line| line.matched) {
        if *counted >= end {
            past = true;
            break;
        }
        if *counted >= options.offset {
            taken_matches.push(line.number);
        }
        *counted += 1;
    }
    let (Some(&first), Some(&last)) = (taken_matches.first(), taken_matches.last()) else {
        return (Vec::new(), past);
    };
    let reach = options.context as u64;
    let taken = lines
        .into_iter()
        .filter(|line| {
            if line.matched {
                taken_matches.binary_search(&line.number).is_ok()
            } else {
                line.number + reach >= first && line.number <= last + reach
            }
        })
        .collect();
    (taken, past)
}
