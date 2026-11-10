//! Whether a call goes ahead, and where it may write, as its thread's mode and what the person granted its project say:
//! weighed by its effect alone.
//!
//! - In `default`, a read goes ahead; an edit within the workspace, and a command within the system's sandbox — writing
//!   the workspace and the temporary folders alone — go ahead held there; a command that asks to write beyond the
//!   sandbox, every command where the machine confines none, an edit beyond the workspace and a tool that may do
//!   anything ask the person.
//! - In `auto`, nothing is sandboxed: a read, an edit within the workspace and a line that reads alone go ahead; all
//!   else a second model judges.
//! - In `full-access`, everything goes ahead.
//!
//! In every mode, what the person granted the project goes ahead, writing anywhere; and working the thread's browser,
//! which changes no file of the machine's, goes ahead.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use siphon_threads::Mode;

use crate::effect::{Effect, Line};

/// Where a call that goes ahead may write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Writes {
    /// Its thread's workspace and the machine's temporary folders alone.
    Workspace,
    Anywhere,
}

/// What a weighing came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// It goes ahead, writing as said.
    Go(Writes),
    /// The person decides.
    Ask,
    /// A second model judges.
    Review,
}

/// What the person granted a project for good: each a command's pattern, as `git push *`, a path's glob, as
/// `/work/app/docs/**`, or a tool's name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grants {
    pub commands: Vec<String>,
    pub paths: Vec<String>,
    pub tools: Vec<String>,
}

/// The patterns an "always" grants of `effect`: a line's commands by the words that name what they do, the files an
/// edit makes, a tool by its name; none for a line no reading knows whole, which no grant covers.
#[must_use]
pub fn patterns_of(effect: &Effect) -> Grants {
    match effect {
        Effect::Reads | Effect::Browses | Effect::Calls => Grants::default(),
        Effect::Edits(paths) => Grants {
            paths: paths.iter().map(|path| path.to_string_lossy().into_owned()).collect(),
            ..Grants::default()
        },
        Effect::Runs { line, .. } => Grants {
            commands: Line::read(line).map(|read| read.patterns).unwrap_or_default(),
            ..Grants::default()
        },
        Effect::Acts { tool } => Grants {
            tools: vec![tool.clone()],
            ..Grants::default()
        },
    }
}

impl Grants {
    /// Whether these grants cover all `effect` does.
    #[must_use]
    pub fn cover(&self, effect: &Effect) -> bool {
        match effect {
            Effect::Reads | Effect::Browses | Effect::Calls => true,
            Effect::Edits(paths) => paths.iter().all(|path| self.paths.iter().any(|glob| matches_glob(glob, path))),
            Effect::Runs { line, .. } => Line::read(line).is_ok_and(|read| {
                !read.commands.is_empty()
                    && read
                        .commands
                        .iter()
                        .all(|command| self.commands.iter().any(|pattern| covers(pattern, command)))
            }),
            Effect::Acts { tool } => self.tools.contains(tool),
        }
    }

    /// These grants with `more` added.
    pub fn add(&mut self, more: Grants) {
        for (kept, added) in [
            (&mut self.commands, more.commands),
            (&mut self.paths, more.paths),
            (&mut self.tools, more.tools),
        ] {
            for each in added {
                if !kept.contains(&each) {
                    kept.push(each);
                }
            }
        }
    }
}

/// Whether the command pattern `pattern` — words, then ` *` — covers `command`.
fn covers(pattern: &str, command: &str) -> bool {
    let named = pattern.strip_suffix(" *").unwrap_or(pattern);
    command == named || command.starts_with(&format!("{named} "))
}

fn matches_glob(glob: &str, path: &Path) -> bool {
    globset::Glob::new(glob).is_ok_and(|built| built.compile_matcher().is_match(path))
}

/// The workspace a weighing holds edits and commands to, and whether the machine confines a command to a sandbox.
#[derive(Debug, Clone)]
pub struct Bounds {
    pub workspace: PathBuf,
    pub confines: bool,
}

/// Weighs `effect` as `mode` and `grants` say, within `bounds`.
#[must_use]
pub fn weigh(effect: &Effect, mode: Mode, grants: &Grants, bounds: &Bounds) -> Verdict {
    if mode == Mode::FullAccess || (!matches!(effect, Effect::Reads | Effect::Browses | Effect::Calls) && grants.cover(effect)) {
        return Verdict::Go(Writes::Anywhere);
    }
    let within = |paths: &[PathBuf]| paths.iter().all(|path| path.starts_with(&bounds.workspace));
    match (mode, effect) {
        (_, Effect::Reads | Effect::Browses | Effect::Calls) => Verdict::Go(Writes::Workspace),
        (_, Effect::Edits(paths)) if within(paths) => Verdict::Go(Writes::Workspace),
        (
            Mode::Default,
            Effect::Runs {
                outside_sandbox: false, ..
            },
        ) if bounds.confines => Verdict::Go(Writes::Workspace),
        (Mode::Auto, Effect::Runs { line, .. }) if Line::read(line).is_ok_and(|read| read.reads_alone) => Verdict::Go(Writes::Anywhere),
        (Mode::Default, _) => Verdict::Ask,
        _ => Verdict::Review,
    }
}
