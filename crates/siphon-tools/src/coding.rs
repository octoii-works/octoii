//! The coding tools: read, write, edit, glob and grep on the files of the machine a thread works on, through its
//! namespace — the threads view among them. What each gives is the model's to read as it is: a file's lines, a
//! folder's names, an image as an image; and a line after the text tells what a page leaves out and where to read on.

use std::fmt::Write as _;

use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::Deserialize;
use siphon_machine::paths::Base;
use siphon_machine::{EditOutcome, GrepOptions, Machine, Read, ReadOptions};
use siphon_text::{Bounds, Edit};

use crate::effect::Effect;
use crate::told::{INSTRUCTIONS_FILE, Instructions, Structured, Told};
use crate::tool::{Context, Refusal, Tool};

/// How many files a page of glob gives, matching lines a page of grep, and the characters of a line grep shows.
const FILES_PAGE: usize = 200;
const MATCHES_PAGE: usize = 250;
const GREP_LINE_CHARS_MAX: usize = 500;

fn failed(error: siphon_machine::Error) -> Refusal {
    Refusal(match error {
        siphon_machine::Error::NotFound => "not-found: nothing is there".into(),
        siphon_machine::Error::Directory => "it is a directory".into(),
        siphon_machine::Error::NotText => "it holds no text".into(),
        siphon_machine::Error::Beak(error) => error.to_string(),
    })
}

/// The refusal of a call on `path` for `error`, where nothing is there with where the machine came nearest: the
/// nearest folder on its way, and the names in it most like the one missing.
fn failed_at(machine: &Machine, path: &str, error: siphon_machine::Error) -> Refusal {
    if !matches!(error, siphon_machine::Error::NotFound) {
        return failed(error);
    }
    let Ok(missing) = machine.missing(path) else { return failed(error) };
    let mut words = format!(
        "not-found: nothing is there\n[the nearest folder there is {}",
        machine.shown(&missing.folder)
    );
    if !missing.like.is_empty() {
        let _ = write!(words, "; in it, names like the one asked: {}", missing.like.join(", "));
    }
    words.push(']');
    Refusal(words)
}

/// The instructions of each folder below `workspace` down to `folder`, both namespace paths, the farthest first: those
/// of the workspace and the folders above it the model is told before each turn.
fn on_the_way(machine: &Machine, workspace: &str, folder: &str) -> Vec<Instructions> {
    let below = if workspace.is_empty() {
        Some(folder)
    } else {
        folder.strip_prefix(workspace).and_then(|rest| rest.strip_prefix('/'))
    };
    let Some(below) = below else { return Vec::new() };
    let mut at = workspace.to_owned();
    let mut found = Vec::new();
    for name in below.split('/').filter(|name| !name.is_empty()) {
        if !at.is_empty() {
            at.push('/');
        }
        at.push_str(name);
        let file = format!("{at}/{INSTRUCTIONS_FILE}");
        if let Ok(Read::Text { page, .. }) = machine.read(&file, &ReadOptions::default()) {
            let lines: Vec<&str> = page.lines.iter().map(|line| line.text.as_str()).collect();
            found.push(Instructions {
                path: format!("/{file}"),
                text: lines.join("\n"),
            });
        }
    }
    found
}

/// The folder of the namespace's `path`.
fn folder_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(folder, _)| folder)
}

/// Runs `work` on the machine off the async threads, as the namespace's reads and writes may wait on the disk.
async fn blocking<T: Send + 'static>(
    context: &Context,
    work: impl FnOnce(&Machine) -> Result<T, Refusal> + Send + 'static,
) -> Result<T, Refusal> {
    let context = context.clone();
    tokio::task::spawn_blocking(move || work(&context.machine()?))
        .await
        .map_err(|error| Refusal(format!("the work failed: {error}")))?
}

fn text(words: impl Into<String>) -> Vec<InputContent> {
    vec![InputContent::text(words)]
}

/// The arguments of `read`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadArgs {
    /// The file or directory, relative to the workspace, or absolute; ~ the home folder.
    pub path: String,
    /// The first line, or entry, from 1.
    #[schemars(range(min = 1))]
    pub offset: Option<usize>,
    /// How many lines, or entries.
    #[schemars(range(min = 1))]
    pub limit: Option<usize>,
}

/// `read`: a page of a file's lines, a directory's names, an image shown.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReadTool;

impl Tool for ReadTool {
    const NAME: &'static str = "read";
    const DESCRIPTION: &'static str = "Reads a file, or lists a directory, a / after each directory's name. Text comes as it is, without line numbers, at most 2000 lines or 48 KB a page; a line longer than 2000 characters is cut. A note after the page gives the lines it holds and the offset to read on from. An image is shown to you; a PDF or other binary file is named, not shown.";
    type Args = ReadArgs;

    fn effect(&self, _: &ReadArgs, _: &Base) -> Effect {
        Effect::Reads
    }

    async fn exec(&self, context: &Context, args: ReadArgs) -> Result<Told, Refusal> {
        let mut bounds = Bounds::default();
        bounds.offset = args.offset.unwrap_or(1);
        bounds.limit = args.limit.unwrap_or(bounds.limit);
        let options = ReadOptions {
            bounds,
            ..ReadOptions::default()
        };
        let workspace = context.base.workspace.clone();
        let (read, name, found) = blocking(context, move |machine| {
            let path = machine.path(&args.path);
            let read = machine.read(&path, &options).map_err(|error| failed_at(machine, &path, error))?;
            let (Read::Text { path, .. }
            | Read::Directory { path, .. }
            | Read::Media { path, .. }
            | Read::Binary { path, .. }
            | Read::TooLarge { path, .. }) = &read;
            let folder = if matches!(read, Read::Directory { .. }) {
                path.as_str()
            } else {
                folder_of(path)
            };
            let found = on_the_way(machine, &machine.path(&workspace.to_string_lossy()), folder);
            let name = machine.shown(path);
            Ok((read, name, found))
        })
        .await?;
        let lines = match &read {
            Read::Text { page, .. } => {
                let (from, to) = (
                    page.lines.first().map_or(0, |line| line.number),
                    page.lines.last().map_or(0, |line| line.number),
                );
                let total = page.whole.then_some(to);
                Some(Structured::Read {
                    path: name.clone(),
                    from,
                    to,
                    total,
                })
            }
            _ => None,
        };
        let output = shown(context, &name, read).await?;
        let mut told = match lines {
            Some(lines) => Told::structured(output, &lines),
            None => Told::from(output),
        };
        told.instructions = found;
        Ok(told)
    }
}

/// What a read gives the model.
async fn shown(context: &Context, name: &str, read: Read) -> Result<Vec<InputContent>, Refusal> {
    Ok(match read {
        Read::Text { page, .. } => {
            let mut out = page.lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n");
            let (first, last) = (
                page.lines.first().map_or(0, |line| line.number),
                page.lines.last().map_or(0, |line| line.number),
            );
            if !page.whole {
                let _ = write!(out, "\n[lines {first}-{last}; more from offset {}]", last + 1);
            } else if first > 1 {
                let _ = write!(out, "\n[lines {first}-{last} of {last}]");
            }
            let cut = page.lines.iter().filter(|line| line.cut).count();
            if cut > 0 {
                let _ = write!(out, "\n[{cut} lines cut for their length]");
            }
            text(out)
        }
        Read::Directory { entries, from, total, .. } => {
            let mut out = String::new();
            for entry in &entries {
                let slash = if entry.kind == beak::FileKind::Directory { "/" } else { "" };
                let _ = writeln!(out, "{}{slash}", String::from_utf8_lossy(&entry.name));
            }
            let last = from + entries.len().saturating_sub(1);
            if entries.len() < total {
                let _ = write!(out, "[entries {from}-{last} of {total}; more from offset {}]", last + 1);
            }
            text(out)
        }
        Read::Media { media_type, bytes, .. } => media(context, name, media_type, &bytes).await?,
        Read::Binary { size, .. } => text(format!("{name} is a binary file of {size} bytes, not shown")),
        Read::TooLarge { size, limit, .. } => text(format!("{name} is {size} bytes, past the {limit} a read takes")),
    })
}

/// An image shown to the model, fitted to what every model reads and kept in the thread; a PDF named, not shown.
async fn media(context: &Context, name: &str, media_type: &str, bytes: &[u8]) -> Result<Vec<InputContent>, Refusal> {
    if media_type == "application/pdf" {
        return Ok(text(format!("{name} is a PDF of {} bytes, not shown", bytes.len())));
    }
    context.show_image(bytes, Some(name)).await
}

/// The arguments of `write`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteArgs {
    /// The file, relative to the workspace, or absolute; ~ the home folder.
    pub path: String,
    /// What the file holds after.
    pub content: String,
}

/// `write`: a file written whole.
#[derive(Debug, Clone, Copy, Default)]
pub struct WriteTool;

fn edits(base: &Base, path: &str) -> Effect {
    Effect::Edits(vec![base.resolve(path)])
}

impl Tool for WriteTool {
    const NAME: &'static str = "write";
    const DESCRIPTION: &'static str = "Writes a file whole, making it and its directories where they are missing. Gives how many lines it added and took out. To change part of a file, use edit.";
    type Args = WriteArgs;

    fn effect(&self, args: &WriteArgs, base: &Base) -> Effect {
        edits(base, &args.path)
    }

    async fn exec(&self, context: &Context, args: WriteArgs) -> Result<Told, Refusal> {
        let workspace = context.base.workspace.clone();
        blocking(context, move |machine| {
            let path = machine.path(&args.path);
            let shown = machine.shown(&path);
            let change = machine.write(&path, &args.content, &shown).map_err(failed)?;
            let made = if change.created { "made" } else { "wrote" };
            let words = text(format!("{made} {shown}: +{} -{}", change.diff.additions, change.diff.deletions));
            let changed = Structured::Changed {
                path: shown,
                created: change.created,
                additions: change.diff.additions,
                deletions: change.diff.deletions,
            };
            let mut told = Told::structured(words, &changed);
            told.instructions = on_the_way(machine, &machine.path(&workspace.to_string_lossy()), folder_of(&path));
            Ok(told)
        })
        .await
    }
}

/// The arguments of `edit`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditArgs {
    /// The file, relative to the workspace, or absolute; ~ the home folder.
    pub path: String,
    /// The text to replace, as read gives it.
    pub old: String,
    /// The text to put in its place.
    pub new: String,
    /// Replace every match.
    #[serde(default)]
    pub all: bool,
}

/// `edit`: text in a file replaced.
#[derive(Debug, Clone, Copy, Default)]
pub struct EditTool;

impl Tool for EditTool {
    const NAME: &'static str = "edit";
    const DESCRIPTION: &'static str = "Replaces text in a file: `old` must match one place, or every place with `all`. Give in `old` just the lines that make it unique. It is matched exactly, else with typographic characters (curly quotes, dashes, special spaces) read as plain ones, else line by line ignoring trailing whitespace; a match not exact is noted with how it matched. Gives the change as a unified diff. A refusal gives its reason: empty, same (`new` is `old`), not-found (`old` is nowhere in the file; the lines nearest to it follow, as the file holds them, or a note that `new` is there already), unchanged (the edit would leave the file as it is), many (how many places matched: give more of `old`, or set `all`).";
    type Args = EditArgs;

    fn effect(&self, args: &EditArgs, base: &Base) -> Effect {
        edits(base, &args.path)
    }

    async fn exec(&self, context: &Context, args: EditArgs) -> Result<Told, Refusal> {
        let workspace = context.base.workspace.clone();
        blocking(context, move |machine| {
            let path = machine.path(&args.path);
            let edit = Edit {
                old: &args.old,
                new: &args.new,
                all: args.all,
            };
            match machine
                .edit(&path, &edit, &machine.shown(&path))
                .map_err(|error| failed_at(machine, &path, error))?
            {
                EditOutcome::Made { change, edited } => {
                    let changed = Structured::Changed {
                        path: machine.shown(&path),
                        created: change.created,
                        additions: change.diff.additions,
                        deletions: change.diff.deletions,
                    };
                    let mut out = change.diff.unified;
                    if edited.matching.name() != "exact" {
                        let _ = write!(out, "\n[matched {}]", edited.matching.name());
                    }
                    if edited.replacements > 1 {
                        let _ = write!(out, "\n[{} places replaced]", edited.replacements);
                    }
                    let mut told = Told::structured(text(out), &changed);
                    told.instructions = on_the_way(machine, &machine.path(&workspace.to_string_lossy()), folder_of(&path));
                    Ok(told)
                }
                EditOutcome::Refused(siphon_text::Refusal::NotFound(nearest)) => Err(Refusal(not_found(&nearest))),
                EditOutcome::Refused(siphon_text::Refusal::Many(count)) => {
                    Err(Refusal(format!("many: {count} places match; give more of old, or set all")))
                }
                EditOutcome::Refused(refusal) => Err(Refusal(refusal.to_string())),
            }
        })
        .await
    }
}

/// The refusal of an edit whose `old` is nowhere in its file, with where the file came nearest.
fn not_found(nearest: &siphon_text::Nearest) -> String {
    match nearest {
        siphon_text::Nearest::Made => "not-found\n[`new` is in the file already: the edit may have been made]".into(),
        siphon_text::Nearest::Lines { first, lines } => {
            format!("not-found\n[the nearest lines, from line {first}:]\n{}", lines.join("\n"))
        }
        siphon_text::Nearest::Nowhere => "not-found".into(),
    }
}

/// The arguments of `glob`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GlobArgs {
    /// A glob, as src/**/*.rs.
    pub pattern: String,
    /// The directory to look under, relative to the workspace, or absolute; ~ the home folder; the workspace unless
    /// given.
    #[serde(default)]
    pub path: String,
    /// The first file, from 1, the newest first.
    #[schemars(range(min = 1))]
    pub offset: Option<usize>,
}

/// `glob`: the files whose paths match a glob, the newest first.
#[derive(Debug, Clone, Copy, Default)]
pub struct GlobTool;

impl Tool for GlobTool {
    const NAME: &'static str = "glob";
    const DESCRIPTION: &'static str = "Finds the files under a directory whose paths match a glob, honouring ignore files: their paths one a line, the newest first, 200 a page; a note gives the offset to go on from.";
    type Args = GlobArgs;

    fn effect(&self, _: &GlobArgs, _: &Base) -> Effect {
        Effect::Reads
    }

    async fn exec(&self, context: &Context, args: GlobArgs) -> Result<Told, Refusal> {
        blocking(context, move |machine| {
            let offset = args.offset.unwrap_or(1).max(1) - 1;
            let found = machine
                .glob(&machine.path(&args.path), &args.pattern, offset, FILES_PAGE)
                .map_err(|error| Refusal(error.to_string()))?;
            if found.total > 0 && found.files.is_empty() {
                return Err(Refusal(format!("past-end: {} files match", found.total)));
            }
            let mut out = String::new();
            for file in &found.files {
                let _ = writeln!(out, "{}", machine.shown(file));
            }
            let last = offset + found.files.len();
            if found.total == 0 {
                out.push_str("[no match]");
            } else if last < found.total {
                let _ = write!(
                    out,
                    "[files {}-{last} of {}, the newest first; more from offset {}]",
                    offset + 1,
                    found.total,
                    last + 1
                );
            }
            denied(machine, &found.denied, &mut out);
            Ok(text(out))
        })
        .await
        .map(Told::from)
    }
}

/// Notes the folders a walk was denied, as the model reads their paths.
fn denied(machine: &Machine, folders: &[String], out: &mut String) {
    if !folders.is_empty() {
        let shown: Vec<String> = folders.iter().map(|folder| machine.shown(folder)).collect();
        let _ = write!(out, "\n[not searched, as access is denied: {}]", shown.join(", "));
    }
}

/// The arguments of `grep`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GrepArgs {
    /// A regular expression, as ripgrep reads one, or plain text with literal.
    pub pattern: String,
    /// The directory or file to look in, relative to the workspace, or absolute; ~ the home folder; the workspace
    /// unless given.
    #[serde(default)]
    pub path: String,
    /// Only files whose paths match it: their names, for a glob of no /.
    pub glob: Option<String>,
    /// Take `pattern` as plain text.
    #[serde(default)]
    pub literal: bool,
    /// Match letters of either case.
    #[serde(default)]
    pub case_insensitive: bool,
    /// Lines shown around each match.
    #[serde(default)]
    pub context: usize,
    /// The first matching line, from 1, the files in order by path.
    #[schemars(range(min = 1))]
    pub offset: Option<usize>,
}

/// `grep`: the lines a regular expression matches.
#[derive(Debug, Clone, Copy, Default)]
pub struct GrepTool;

impl Tool for GrepTool {
    const NAME: &'static str = "grep";
    const DESCRIPTION: &'static str = "Finds the lines a regular expression matches, in ripgrep's syntax, in the files under a directory or in one file, honouring ignore files: each as `path:line: text`, a line around it as `path:line- text`, 250 matches a page, a note giving the offset to go on from. A line longer than 500 characters is cut. Binary files are passed over, and counted.";
    type Args = GrepArgs;

    fn effect(&self, _: &GrepArgs, _: &Base) -> Effect {
        Effect::Reads
    }

    async fn exec(&self, context: &Context, args: GrepArgs) -> Result<Told, Refusal> {
        blocking(context, move |machine| {
            let offset = args.offset.unwrap_or(1).max(1) - 1;
            let options = GrepOptions {
                literal: args.literal,
                case_insensitive: args.case_insensitive,
                context: args.context,
                glob: args.glob,
                offset,
                limit: MATCHES_PAGE,
                line_chars_max: GREP_LINE_CHARS_MAX,
            };
            let grepped = machine
                .grep(&machine.path(&args.path), &args.pattern, &options)
                .map_err(|error| Refusal(error.to_string()))?;
            Ok(text(grep_text(machine, &grepped, offset)))
        })
        .await
        .map(Told::from)
    }
}

/// What a grep found, as the model reads it.
fn grep_text(machine: &Machine, grepped: &siphon_machine::Grepped, offset: usize) -> String {
    let (mut out, mut matched, mut cut) = (String::new(), 0, false);
    for file in &grepped.files {
        let shown = machine.shown(&file.path);
        for line in &file.lines {
            let _ = writeln!(out, "{shown}:{}{} {}", line.number, if line.matched { ":" } else { "-" }, line.text);
            matched += usize::from(line.matched);
            cut |= line.cut;
        }
    }
    if matched == 0 {
        out.push_str("[no match]");
    } else if grepped.more {
        let _ = write!(
            out,
            "[matches {}-{}; more from offset {}]",
            offset + 1,
            offset + matched,
            offset + matched + 1
        );
    }
    if cut {
        let _ = write!(out, "\n[lines cut at {GREP_LINE_CHARS_MAX} characters]");
    }
    if grepped.binary > 0 {
        let _ = write!(out, "\n[{} binary files passed over]", grepped.binary);
    }
    denied(machine, &grepped.denied, &mut out);
    out
}
