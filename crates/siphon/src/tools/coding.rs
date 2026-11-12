//! The coding tools: read, write, edit, glob and grep on the files of the machine a thread works on, through its
//! namespace — the threads view among them. What each gives is the model's to read as it is: a file's lines, a
//! folder's names, an image as an image; and a line after the text tells what a page leaves out and where to read on.

use std::fmt::Write as _;

use crate::machine::paths::Base;
use crate::machine::{EditOutcome, GrepOptions, Machine, Read, ReadOptions};
use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::Deserialize;
use siphon_text::{Bounds, Edit};

use crate::tools::effect::Effect;
use crate::tools::told::{INSTRUCTIONS_FILE, Instructions, Structured, Told};
use crate::tools::tool::{Context, Refusal, Tool};

/// How many files a page of glob gives, matching lines a page of grep, and the characters of a line grep shows.
const FILES_PAGE: usize = 200;
const MATCHES_PAGE: usize = 250;
const GREP_LINE_CHARS_MAX: usize = 500;

fn failed(error: crate::machine::Error) -> Refusal {
    Refusal(match error {
        crate::machine::Error::NotFound => "nothing is there".into(),
        crate::machine::Error::Directory => "it is a directory".into(),
        crate::machine::Error::NotText => "it holds no text".into(),
        crate::machine::Error::Beak(error) => error.to_string(),
    })
}

/// The refusal of a call on `path` for `error`, where nothing is there with where the machine came nearest: the
/// nearest folder on its way, and the names in it most like the one missing.
fn failed_at(machine: &Machine, path: &str, error: crate::machine::Error) -> Refusal {
    if !matches!(error, crate::machine::Error::NotFound) {
        return failed(error);
    }
    let Ok(missing) = machine.missing(path) else { return failed(error) };
    let mut words = format!(
        "nothing is there\n<system-reminder>the nearest folder there is {}",
        machine.shown(&missing.folder)
    );
    if !missing.like.is_empty() {
        let _ = write!(words, "; in it, names like the one asked: {}", missing.like.join(", "));
    }
    words.push_str("</system-reminder>");
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
    /// File or directory: relative to the workspace, absolute, or starting with ~ for the home folder.
    pub path: String,
    /// First line or entry to return, from 1.
    #[schemars(range(min = 1))]
    pub offset: Option<usize>,
    /// Number of lines or entries to return.
    #[schemars(range(min = 1))]
    pub limit: Option<usize>,
}

/// `read`: a page of a file's lines, a directory's names, an image shown.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReadTool;

impl Tool for ReadTool {
    const NAME: &'static str = "read";
    const DESCRIPTION: &'static str = "Read a file, or list a directory (directory names end in /). Text is returned as is, without line numbers, up to 2000 lines or 48 KB per call; lines over 2000 characters are truncated. A note after the text gives the lines returned and the offset to continue from. Images are shown to you; PDFs and other binary files are named, not shown.";
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
            let mut out =
                crate::reminder::escaped(&page.lines.iter().map(|line| line.text.as_str()).collect::<Vec<_>>().join("\n")).into_owned();
            let (first, last) = (
                page.lines.first().map_or(0, |line| line.number),
                page.lines.last().map_or(0, |line| line.number),
            );
            if page.lines.is_empty() {
                // A page of no lines: the file holds none, or it ends before the offset asked.
                let _ = write!(
                    out,
                    "{}",
                    if page.seen == 0 {
                        "<system-reminder>the file is empty</system-reminder>".to_owned()
                    } else {
                        format!(
                            "<system-reminder>the file has {} lines, all before the offset</system-reminder>",
                            page.seen
                        )
                    }
                );
            } else if !page.whole {
                let _ = write!(
                    out,
                    "\n<system-reminder>lines {first}-{last}; more from offset {}</system-reminder>",
                    last + 1
                );
            } else if first > 1 {
                let _ = write!(out, "\n<system-reminder>lines {first}-{last} of {last}</system-reminder>");
            }
            let cut = page.lines.iter().filter(|line| line.cut).count();
            if cut > 0 {
                let _ = write!(out, "\n<system-reminder>{cut} lines cut for their length</system-reminder>");
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
                let _ = write!(
                    out,
                    "<system-reminder>entries {from}-{last} of {total}; more from offset {}</system-reminder>",
                    last + 1
                );
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
    /// File: relative to the workspace, absolute, or starting with ~ for the home folder.
    pub path: String,
    /// The file's complete new contents.
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
    const DESCRIPTION: &'static str = "Write a whole file, creating it and any missing directories. Returns how many lines were added and removed. To change part of a file, use edit.";
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
    /// File: relative to the workspace, absolute, or starting with ~ for the home folder.
    pub path: String,
    /// Text to replace, exactly as read returned it.
    pub old: String,
    /// Replacement text.
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
    const DESCRIPTION: &'static str = "Replace text in a file. `old` must match exactly one place, or every place with `all`; include just enough lines to make it unique. Matching tries the exact text, then with typographic characters (curly quotes, dashes, special spaces) read as plain ones, then line by line ignoring trailing whitespace; an inexact match is noted. Returns the change as a unified diff. If `old` is not found, the closest lines in the file are shown, or a note says `new` is already there.";
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
                        let _ = write!(out, "\n<system-reminder>matched {}</system-reminder>", edited.matching.name());
                    }
                    if edited.replacements > 1 {
                        let _ = write!(out, "\n<system-reminder>{} places replaced</system-reminder>", edited.replacements);
                    }
                    let mut told = Told::structured(text(out), &changed);
                    told.instructions = on_the_way(machine, &machine.path(&workspace.to_string_lossy()), folder_of(&path));
                    Ok(told)
                }
                EditOutcome::Refused(siphon_text::Refusal::NotFound(nearest)) => Err(Refusal(not_found(&nearest))),
                EditOutcome::Refused(refusal) => Err(Refusal(refusal.to_string())),
            }
        })
        .await
    }
}

/// The refusal of an edit whose `old` is nowhere in its file, with where the file came nearest.
fn not_found(nearest: &siphon_text::Nearest) -> String {
    match nearest {
        siphon_text::Nearest::Made => {
            "`old` is nowhere in the file\n<system-reminder>`new` is in the file already: the edit may have been made</system-reminder>"
                .into()
        }
        siphon_text::Nearest::Lines { first, lines } => {
            format!(
                "`old` is nowhere in the file\n<system-reminder>the nearest lines, from line {first}:</system-reminder>\n{}",
                lines.join("\n")
            )
        }
        siphon_text::Nearest::Nowhere => "`old` is nowhere in the file".into(),
    }
}

/// The arguments of `glob`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GlobArgs {
    /// Glob pattern, such as src/**/*.rs.
    pub pattern: String,
    /// Directory to search: relative to the workspace, absolute, or starting with ~ for the home folder. Defaults to
    /// the workspace.
    #[serde(default)]
    pub path: String,
    /// First result to return, from 1, newest first.
    #[schemars(range(min = 1))]
    pub offset: Option<usize>,
}

/// `glob`: the files whose paths match a glob, the newest first.
#[derive(Debug, Clone, Copy, Default)]
pub struct GlobTool;

impl Tool for GlobTool {
    const NAME: &'static str = "glob";
    const DESCRIPTION: &'static str = "Find files under a directory whose paths match a glob, respecting ignore files. Returns one path per line, newest first, 200 per page; a note gives the offset of the next page.";
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
                out.push_str("<system-reminder>no match</system-reminder>");
            } else if last < found.total {
                let _ = write!(
                    out,
                    "<system-reminder>files {}-{last} of {}, the newest first; more from offset {}</system-reminder>",
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
        let _ = write!(
            out,
            "\n<system-reminder>not searched, as access is denied: {}</system-reminder>",
            shown.join(", ")
        );
    }
}

/// The arguments of `grep`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GrepArgs {
    /// Regular expression in ripgrep syntax, or plain text with `literal`.
    pub pattern: String,
    /// Directory or file to search: relative to the workspace, absolute, or starting with ~ for the home folder.
    /// Defaults to the workspace.
    #[serde(default)]
    pub path: String,
    /// Search only files matching this glob; a glob without / matches file names.
    pub glob: Option<String>,
    /// Treat `pattern` as plain text.
    #[serde(default)]
    pub literal: bool,
    /// Ignore case.
    #[serde(default)]
    pub case_insensitive: bool,
    /// Lines of context to show around each match.
    #[serde(default)]
    pub context: usize,
    /// First match to return, from 1, files ordered by path.
    #[schemars(range(min = 1))]
    pub offset: Option<usize>,
}

/// `grep`: the lines a regular expression matches.
#[derive(Debug, Clone, Copy, Default)]
pub struct GrepTool;

impl Tool for GrepTool {
    const NAME: &'static str = "grep";
    const DESCRIPTION: &'static str = "Search file contents with a regular expression (ripgrep syntax), in the files under a directory or in one file, respecting ignore files. Returns matches as `path:line: text` and context lines as `path:line- text`, 250 matches per page; a note gives the offset of the next page. Lines over 500 characters are truncated. Binary files are skipped and counted.";
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
fn grep_text(machine: &Machine, grepped: &crate::machine::Grepped, offset: usize) -> String {
    let (mut out, mut matched, mut cut) = (String::new(), 0, false);
    for file in &grepped.files {
        let shown = machine.shown(&file.path);
        for line in &file.lines {
            let text = crate::reminder::escaped(&line.text);
            let _ = writeln!(out, "{shown}:{}{} {text}", line.number, if line.matched { ":" } else { "-" });
            matched += usize::from(line.matched);
            cut |= line.cut;
        }
    }
    if matched == 0 {
        out.push_str("<system-reminder>no match</system-reminder>");
    } else if grepped.more {
        let _ = write!(
            out,
            "<system-reminder>matches {}-{}; more from offset {}</system-reminder>",
            offset + 1,
            offset + matched,
            offset + matched + 1
        );
    }
    if cut {
        let _ = write!(
            out,
            "\n<system-reminder>lines cut at {GREP_LINE_CHARS_MAX} characters</system-reminder>"
        );
    }
    if grepped.binary > 0 {
        let _ = write!(
            out,
            "\n<system-reminder>{} binary files passed over</system-reminder>",
            grepped.binary
        );
    }
    denied(machine, &grepped.denied, &mut out);
    out
}
