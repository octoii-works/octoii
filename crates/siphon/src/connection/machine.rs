//! The machine's files as the person browses them, beside the threads: a folder's entries a page at a time, each with
//! whether git leaves it out; a file's lines a page at a time, or what it is where it is no text; and the files under
//! a folder whose paths match what the person types, as a fuzzy finder matches them. The machine is the person's, read
//! alone, what the node keeps for itself hidden from it as from every thread.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};
use siphon_machine::{Is, Machine, Read, ReadOptions};
use siphon_protocol::Error;
use siphon_protocol::siphon::{FileKind, FilePage, FolderEntry, FolderPage, FoundPaths, ListFolder, ReadFile, SearchFiles};

use super::refusal;
use crate::Siphon;

fn machine(siphon: &Siphon) -> Result<Machine, Error> {
    siphon.person_machine().map_err(|error| refusal("failed", &error.to_string()))
}

/// The namespace's `path` as the person names it: whole, from the root.
fn absolute(path: &str) -> String {
    format!("/{path}")
}

fn failed(error: &siphon_machine::Error) -> Error {
    let code = match error {
        siphon_machine::Error::NotFound => "not_found",
        siphon_machine::Error::Directory => "a_folder",
        _ => "failed",
    };
    refusal(code, &error.to_string())
}

/// A page of the folder `asked` names.
pub(super) fn list(siphon: &Siphon, asked: &ListFolder) -> Result<FolderPage, Error> {
    let machine = machine(siphon)?;
    let path = machine.path(&asked.path);
    let listed = machine.listed(&path).map_err(|error| failed(&error))?;
    let limit = usize::try_from(asked.limit).unwrap_or(usize::MAX);
    let mut after: Vec<FolderEntry> = listed
        .into_iter()
        .filter(|entry| asked.after.as_ref().is_none_or(|after| entry.name > *after))
        .map(|entry| FolderEntry {
            name: entry.name,
            kind: match entry.kind {
                beak::FileKind::Directory => FileKind::Directory,
                beak::FileKind::File => FileKind::File,
                beak::FileKind::Link => FileKind::Link,
            },
            ignored: entry.ignored,
        })
        .collect();
    let more = after.len() > limit;
    after.truncate(limit);
    Ok(FolderPage {
        path: absolute(&path),
        entries: after,
        more,
    })
}

/// A page of the file `asked` names, or what it is.
pub(super) fn read(siphon: &Siphon, asked: &ReadFile) -> Result<FilePage, Error> {
    let machine = machine(siphon)?;
    let stat = machine.stat(&machine.path(&asked.path)).map_err(|error| failed(&error))?;
    let path = absolute(&stat.path);
    match stat.is {
        Is::Folder => return Err(refusal("a_folder", &format!("{path} is a folder: list it"))),
        Is::Media(media_type) => {
            return Ok(FilePage::Media {
                path,
                size: stat.size,
                media_type: media_type.to_owned(),
            });
        }
        Is::File => {}
    }
    let mut options = ReadOptions::default();
    if let Some(offset) = asked.offset {
        options.bounds.offset = usize::try_from(offset).unwrap_or(usize::MAX);
    }
    if let Some(limit) = asked.limit {
        options.bounds.limit = usize::try_from(limit).unwrap_or(usize::MAX);
    }
    match machine.read(&stat.path, &options).map_err(|error| failed(&error))? {
        Read::Text { size, page, .. } => Ok(FilePage::Text {
            path,
            size,
            from: page.lines.first().map_or(1, |line| u32::try_from(line.number).unwrap_or(u32::MAX)),
            more: !page.whole,
            lines: page.lines.into_iter().map(|line| line.text).collect(),
        }),
        _ => Ok(FilePage::Binary { path, size: stat.size }),
    }
}

/// The files under the folder `asked` names, and the folders they lie in, whose paths from it match its query, the
/// best first.
pub(super) fn search(siphon: &Siphon, asked: &SearchFiles) -> Result<FoundPaths, Error> {
    let machine = machine(siphon)?;
    let start = machine.path(&asked.path);
    let walked = machine.walk(&start).map_err(|error| failed(&error))?;
    let prefix = if start.is_empty() { String::new() } else { format!("{start}/") };
    let files: Vec<&str> = walked.files.iter().map(|file| file.strip_prefix(&prefix).unwrap_or(file)).collect();
    let paths = files.iter().map(|file| (*file).to_owned()).chain(folders_of(&files));
    let pattern = Pattern::parse(&asked.query, CaseMatching::Smart, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let best = pattern.match_list(paths, &mut matcher);
    let limit = usize::try_from(asked.limit).unwrap_or(usize::MAX);
    Ok(FoundPaths {
        path: absolute(&start),
        truncated: best.len() > limit,
        found: best.into_iter().take(limit).map(|(path, _)| path).collect(),
    })
}

/// The folders `files` lie in, each once, by its path ending in `/`, as `ls -F` marks a folder.
fn folders_of(files: &[&str]) -> std::collections::BTreeSet<String> {
    let mut folders = std::collections::BTreeSet::new();
    for file in files {
        let mut rest = *file;
        while let Some(at) = rest.rfind('/') {
            rest = &rest[..at];
            if !folders.insert(format!("{rest}/")) {
                break;
            }
        }
    }
    folders
}
