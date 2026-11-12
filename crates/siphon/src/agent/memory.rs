//! The person's memory of a project: what its threads' models keep across threads, a Markdown file a memory in the
//! project's folder of memory, each starting with front matter — its name, a line of what it holds, its type — and
//! `MEMORY.md` listing them, a line each. The model is told how it is kept and given the index; it writes the files with
//! its tools, and the person reads the list and deletes what they want gone.

use std::fmt::Write as _;
use std::io::Read as _;
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The index of a project's memory, the file that lists its memories.
pub const INDEX: &str = "MEMORY.md";
/// The most of the index the model is given, and of a memory's file read for its front matter.
const INDEX_BYTES_MAX: u64 = 16 << 10;

/// A memory, as the person sees it: its file's name in the folder, and its front matter's name, description and type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Memory {
    pub file: String,
    pub name: String,
    pub description: String,
    pub r#type: String,
}

/// Why a memory was not deleted.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} names no memory: a memory is a .md file of the folder, not its index")]
    NotAMemory(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The memory kept in `folder`, as the model is told it: how it is kept, then its index.
#[must_use]
pub fn told(folder: &Path) -> String {
    let index = folder.join(INDEX);
    let listed = head(&index, INDEX_BYTES_MAX).filter(|text| !text.trim().is_empty());
    let cut = std::fs::metadata(&index).is_ok_and(|found| found.len() > INDEX_BYTES_MAX);
    let mut out = format!(
        "<memory path=\"{}\">
What you remember of this project across threads, a Markdown file a memory, kept as you see fit, without asking.
- Keep what a later thread will need and cannot read from the code, its history or the instructions: who the person is and how they work (user); what they told you to do or not to do, and why (feedback); the goals, decisions and constraints of the work (project); where things are outside it (reference).
- A memory's file starts with front matter — name, description (one line, by which a later thread judges whether to read it), type — then the fact; for feedback and project, why and how to apply it. Dates absolute.
- {INDEX} lists them, a line each: `- [name](file.md) — what it holds`. That list is all you are given at the start: read a memory's file when it bears on the task.
- Update a memory rather than add one that says the same; delete one that turned out wrong — its file with `rm`, its line from {INDEX}. When the person asks you to remember or to forget something, do so.
- A memory is what held when it was written: check a file, function or flag it names before you rely on it.
<index>
",
        folder.display()
    );
    out.push_str(listed.as_deref().map_or("(none yet)", str::trim_end));
    if cut {
        let _ = write!(out, "\n[{INDEX} goes on past what is shown; keep it short]");
    }
    out.push_str("\n</index>\n</memory>");
    out
}

/// The memories kept in `folder`, by their files' names; none where it is not there.
#[must_use]
pub fn list(folder: &Path) -> Vec<Memory> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut memories: Vec<Memory> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| memory(&entry.path(), entry.file_name().to_str()?))
        .collect();
    memories.sort_by(|a, b| a.file.cmp(&b.file));
    memories
}

/// The memory whose file is `path`, named `file`; none where it is not a memory's file.
fn memory(path: &Path, file: &str) -> Option<Memory> {
    if !is_memory(file) || !path.is_file() {
        return None;
    }
    let text = head(path, INDEX_BYTES_MAX).unwrap_or_default();
    let mut memory = Memory {
        file: file.to_owned(),
        name: file.trim_end_matches(".md").to_owned(),
        description: String::new(),
        r#type: String::new(),
    };
    for (key, value) in crate::agent::front_matter::of(&text) {
        match key {
            "name" if !value.is_empty() => value.clone_into(&mut memory.name),
            "description" => value.clone_into(&mut memory.description),
            "type" => value.clone_into(&mut memory.r#type),
            _ => {}
        }
    }
    Some(memory)
}

/// Deletes the memory `file` of `folder`: its file, and its line of the index.
///
/// # Errors
/// `NotAMemory` where `file` is not a memory's file's name; the system's failure, as no such file.
pub fn delete(folder: &Path, file: &str) -> Result<(), Error> {
    if !is_memory(file) {
        return Err(Error::NotAMemory(file.to_owned()));
    }
    std::fs::remove_file(folder.join(file))?;
    let index = folder.join(INDEX);
    let Ok(text) = std::fs::read_to_string(&index) else {
        return Ok(());
    };
    let link = format!("]({file})");
    let kept: String = text.split_inclusive('\n').filter(|line| !line.contains(&link)).collect();
    if kept != text {
        std::fs::write(&index, kept)?;
    }
    Ok(())
}

/// Whether `file` names a memory's file: a Markdown file of the folder itself, not its index.
fn is_memory(file: &str) -> bool {
    let markdown = Path::new(file)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"));
    markdown && file != INDEX && !file.starts_with('.') && !file.contains(['/', '\\'])
}

/// The text of the file at `path`, its first `bytes_max` bytes; none where it cannot be read.
fn head(path: &Path, bytes_max: u64) -> Option<String> {
    let mut text = String::new();
    std::fs::File::open(path).ok()?.take(bytes_max).read_to_string(&mut text).ok()?;
    Some(text)
}
