//! The diff between two texts, line by line: the shortest edit between them, as Myers' algorithm finds it — bounded
//! where the search grows too costly — as the unified diff people and models read, `CONTEXT_LINES` unchanged lines
//! around each change, with the lines it adds and removes counted.

use std::fmt::Write as _;
use std::ops::Range;

use similar::{Algorithm, DiffOp};

/// The unchanged lines a unified diff shows around each change.
pub const CONTEXT_LINES: usize = 3;

/// The diff between two texts: the unified diff, empty where they are the same, and the lines it adds and removes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    pub unified: String,
    pub additions: usize,
    pub deletions: usize,
}

impl Diff {
    /// The diff from `before` to `after`, both named `name` — `before` from `/dev/null` where it is none, a file made —
    /// a line without its newline marked, a line's `\r` left out.
    #[must_use]
    pub fn between(before: Option<&str>, after: &str, name: &str) -> Self {
        let old: Vec<&str> = before.unwrap_or("").split_inclusive('\n').collect();
        let new: Vec<&str> = after.split_inclusive('\n').collect();
        let ops = similar::capture_diff_slices(Algorithm::Myers, &old, &new);
        let (mut additions, mut deletions) = (0, 0);
        for op in &ops {
            additions += op.new_range().len() * usize::from(!matches!(op, DiffOp::Equal { .. }));
            deletions += op.old_range().len() * usize::from(!matches!(op, DiffOp::Equal { .. }));
        }
        let groups = similar::group_diff_ops(ops, CONTEXT_LINES);
        if groups.is_empty() {
            return Self {
                unified: String::new(),
                additions,
                deletions,
            };
        }
        let mut unified = format!("--- {}\n+++ {name}\n", if before.is_none() { "/dev/null" } else { name });
        for group in &groups {
            write_hunk(&mut unified, group, &old, &new);
        }
        unified.pop();
        Self {
            unified,
            additions,
            deletions,
        }
    }
}

/// A hunk: its ranges in each text, then each of its lines after its mark.
fn write_hunk(out: &mut String, group: &[DiffOp], old: &[&str], new: &[&str]) {
    let (first, last) = (&group[0], &group[group.len() - 1]);
    let old_range = first.old_range().start..last.old_range().end;
    let new_range = first.new_range().start..last.new_range().end;
    let _ = writeln!(out, "@@ -{} +{} @@", range(&old_range), range(&new_range));
    for op in group {
        match op {
            DiffOp::Equal { .. } => write_lines(out, ' ', &old[op.old_range()]),
            DiffOp::Delete { .. } => write_lines(out, '-', &old[op.old_range()]),
            DiffOp::Insert { .. } => write_lines(out, '+', &new[op.new_range()]),
            DiffOp::Replace { .. } => {
                write_lines(out, '-', &old[op.old_range()]);
                write_lines(out, '+', &new[op.new_range()]);
            }
        }
    }
}

/// A hunk's range as a unified diff writes it: its first line, from 1, and how many; an empty range names the line
/// before it.
fn range(range: &Range<usize>) -> String {
    match range.len() {
        0 => format!("{},0", range.start),
        1 => format!("{}", range.start + 1),
        count => format!("{},{count}", range.start + 1),
    }
}

/// Lines of a diff after their mark, their endings left out; one with no newline followed by a note that it has none.
fn write_lines(out: &mut String, mark: char, lines: &[&str]) {
    for line in lines {
        let text = line
            .strip_suffix('\n')
            .map_or(*line, |line| line.strip_suffix('\r').unwrap_or(line));
        out.push(mark);
        out.push_str(text);
        out.push('\n');
        if !line.ends_with('\n') {
            out.push_str("\\ No newline at end of file\n");
        }
    }
}
