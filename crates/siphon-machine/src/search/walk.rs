//! A walk of a tree through the namespace, as git sees what it holds: each folder's `.gitignore` and `.ignore` leaving
//! out what they name, those of the repository the walk starts in from its root down, and beneath them the ignore files
//! from outside the tree — the person's own and the repository's `info/exclude`. `.git` itself is never walked. A folder
//! whose access is denied is passed over and named, so the walk says what it could not see.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use beak::{FileKind, Status};
use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::files::Error;
use crate::machine::{Machine, inside};

/// The names in a folder that hold the rules of what is left out beneath it.
const IGNORE_FILES: [&str; 2] = [".gitignore", ".ignore"];

/// What a walk found: its files, by the namespace's paths, in order by path; and the folders it was denied.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Walked {
    pub files: Vec<String>,
    pub denied: Vec<String>,
}

/// The machine's absolute path of the namespace's `path`.
fn absolute(path: &str) -> PathBuf {
    Path::new("/").join(path)
}

/// The rules of `contents`, a file of ignore rules, for the folder at the namespace's `folder`.
fn rules(folder: &str, contents: &[u8]) -> Option<Arc<Gitignore>> {
    let mut builder = GitignoreBuilder::new(absolute(folder));
    for line in String::from_utf8_lossy(contents).lines() {
        let _ = builder.add_line(None, line);
    }
    builder.build().ok().filter(|rules| !rules.is_empty()).map(Arc::new)
}

/// Whether `rules`, the nearest last, leave out the name at the namespace's `path`.
fn ignored(rules: &[Arc<Gitignore>], path: &str, is_dir: bool) -> bool {
    let place = absolute(path);
    for each in rules.iter().rev() {
        match each.matched(&place, is_dir) {
            Match::Ignore(_) => return true,
            Match::Whitelist(_) => return false,
            Match::None => {}
        }
    }
    false
}

fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}

impl Machine {
    /// The rules in the folder at the namespace's `folder`, its ignore files', in their order.
    fn rules_in(&self, folder: &str) -> Vec<Arc<Gitignore>> {
        IGNORE_FILES
            .iter()
            .filter_map(|name| {
                let path = join(folder, name);
                let stat = self.namespace.stat(&path).ok()??;
                let bytes = self.namespace.read(&path, 0, stat.size).ok()?;
                rules(folder, &bytes)
            })
            .collect()
    }

    /// The repository the namespace's `path` is in, by its root's path: the nearest folder from it up that holds
    /// `.git` — a folder, or in a worktree a file.
    #[must_use]
    pub fn repository_of(&self, path: &str) -> Option<String> {
        let mut folder = path;
        loop {
            if self.namespace.stat(&join(folder, ".git")).ok().flatten().is_some() {
                return Some(folder.to_owned());
            }
            if folder.is_empty() {
                return None;
            }
            folder = folder.rsplit_once('/').map_or("", |(parent, _)| parent);
        }
    }

    /// The git directory the repository at `root` shares with its worktrees: its `.git` where that is a folder; else
    /// the folder its `.git` file names by `gitdir:`, or the one that folder's `commondir` names.
    fn common_git_directory(&self, root: &str) -> Option<PathBuf> {
        let dot_git = join(root, ".git");
        let stat = self.namespace.stat(&dot_git).ok()??;
        if stat.kind == FileKind::Directory {
            return Some(absolute(&dot_git));
        }
        let text = String::from_utf8_lossy(&self.namespace.read(&dot_git, 0, stat.size).ok()?).into_owned();
        let named = text.lines().find_map(|line| line.strip_prefix("gitdir:"))?.trim().to_owned();
        let git_directory = absolute(root).join(named);
        let common = inside(&git_directory.join("commondir"));
        match self.namespace.read(&common, 0, 4096) {
            Ok(bytes) => Some(git_directory.join(String::from_utf8_lossy(&bytes).trim())),
            Err(_) => Some(git_directory),
        }
    }

    /// The rules a walk from the namespace's `start` begins with: from outside the tree, the person's own and the
    /// repository's `info/exclude`; then the ignore files of each folder from the repository's root down to `start`'s.
    fn rules_above(&self, start: &str) -> Vec<Arc<Gitignore>> {
        let repository = self.repository_of(start);
        let root = repository.clone().unwrap_or_else(|| start.to_owned());
        let mut outside: Vec<PathBuf> = self.excludes.iter().cloned().collect();
        if let Some(repository) = &repository
            && let Some(git_directory) = self.common_git_directory(repository)
        {
            outside.push(git_directory.join("info/exclude"));
        }
        let mut out: Vec<Arc<Gitignore>> = outside
            .iter()
            .filter_map(|file| {
                let path = inside(file);
                let found = self.namespace.stat(&path).ok()??;
                rules(&root, &self.namespace.read(&path, 0, found.size).ok()?)
            })
            .collect();
        // The folders from the root down to `start`'s parent; `start`'s own are the walk's.
        let mut folder = root.clone();
        for name in start
            .strip_prefix(&root)
            .unwrap_or_default()
            .split('/')
            .filter(|name| !name.is_empty())
        {
            out.extend(self.rules_in(&folder));
            folder = join(&folder, name);
        }
        out
    }

    /// Walks the tree at the namespace's `start` — or takes the one file it names — as git sees what it holds.
    ///
    /// # Errors
    /// `NotFound` where nothing is there; the namespace's failure.
    pub fn walk(&self, start: &str) -> Result<Walked, Error> {
        let found = self.namespace.stat(start)?.ok_or(Error::NotFound)?;
        if found.kind != FileKind::Directory {
            return Ok(Walked {
                files: vec![start.to_owned()],
                denied: Vec::new(),
            });
        }
        let mut walked = Walked::default();
        self.walk_folder(start, self.rules_above(start), &mut walked)?;
        walked.files.sort();
        walked.denied.sort();
        Ok(walked)
    }

    fn walk_folder(&self, folder: &str, mut rules: Vec<Arc<Gitignore>>, walked: &mut Walked) -> Result<(), Error> {
        let entries = match self.entries(folder) {
            Ok(entries) => entries,
            Err(Error::Beak(error)) if error.status == Status::Denied => {
                walked.denied.push(folder.to_owned());
                return Ok(());
            }
            Err(Error::NotFound) => return Ok(()),
            Err(error) => return Err(error),
        };
        rules.extend(self.rules_in(folder));
        for entry in entries {
            let name = String::from_utf8_lossy(&entry.name);
            if name == ".git" {
                continue;
            }
            let path = join(folder, &name);
            let is_dir = entry.kind == FileKind::Directory;
            if ignored(&rules, &path, is_dir) {
                continue;
            }
            if is_dir {
                self.walk_folder(&path, rules.clone(), walked)?;
            } else {
                walked.files.push(path);
            }
        }
        Ok(())
    }
}
