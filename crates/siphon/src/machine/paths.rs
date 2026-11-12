//! Paths as a thread's model writes them and reads them back. A path is taken, whatever its spelling, as the place it
//! names: relative to the workspace, as a shell takes one against its working folder; `~` the home folder; `.`, `..`
//! and repeated separators resolved by their names alone, as Plan 9 resolves them. A place is shown as the model would
//! write it again: relative inside the workspace, absolute outside. Nothing is guessed, and nothing is out of bounds:
//! what a thread may reach is where it runs.

use std::path::{Component, Path, PathBuf};

/// Where a thread's paths start: its workspace, by its absolute path, and the home folder `~` names, where known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base {
    pub workspace: PathBuf,
    pub home: Option<PathBuf>,
}

impl Base {
    /// The absolute, clean path `given` names.
    #[must_use]
    pub fn resolve(&self, given: &str) -> PathBuf {
        let expanded = match (&self.home, given) {
            (Some(home), "~") => home.clone(),
            (Some(home), _) if given.starts_with("~/") => home.join(&given[2..]),
            _ => PathBuf::from(given),
        };
        clean(&self.workspace.join(expanded))
    }

    /// The absolute, clean path `place` as the model reads it: relative inside the workspace, `.` the workspace itself,
    /// else as it is.
    #[must_use]
    pub fn shown(&self, place: &Path) -> String {
        let workspace = clean(&self.workspace);
        match place.strip_prefix(&workspace) {
            Ok(rest) if rest.as_os_str().is_empty() => ".".into(),
            Ok(rest) => rest.to_string_lossy().into_owned(),
            Err(_) => place.to_string_lossy().into_owned(),
        }
    }
}

/// `path` with `.`, `..` and repeated separators resolved by their names alone; `..` at the root stays at the root.
#[must_use]
pub fn clean(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}
