//! Search over what the namespace shows — the machine's tree and the threads view alike — as userland walks a file
//! system: no index, every query fresh and whole, its results paged with where the next page starts.

mod grep;
mod walk;

use globset::GlobBuilder;

pub use grep::{GrepError, GrepFile, GrepLine, GrepOptions, Grepped};
pub use walk::{Listed, Walked};

use crate::machine::namespace::Machine;

/// What a glob found: the page of its files, by the namespace's paths, the newest first; how many match in all; and
/// the folders the walk was denied.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Globbed {
    pub files: Vec<String>,
    pub total: usize,
    pub denied: Vec<String>,
}

impl Machine {
    /// The files under the namespace's `start` whose paths below it match `pattern`, the newest first, `limit` of them
    /// from the `offset`th.
    ///
    /// # Errors
    /// `Glob` for no glob; `NotFound` where nothing is there; the namespace's failure.
    pub fn glob(&self, start: &str, pattern: &str, offset: usize, limit: usize) -> Result<Globbed, GrepError> {
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|error| GrepError::Glob(error.to_string()))?
            .compile_matcher();
        let walked = self.walk(start)?;
        let mut found: Vec<(i64, String)> = walked
            .files
            .into_iter()
            .filter(|path| grep::takes(&glob, true, start, path))
            .map(|path| {
                let modified = self.namespace.stat(&path).ok().flatten().map_or(0, |stat| stat.modified_ms);
                (modified, path)
            })
            .collect();
        found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let total = found.len();
        Ok(Globbed {
            files: found.into_iter().skip(offset).take(limit).map(|(_, path)| path).collect(),
            total,
            denied: walked.denied,
        })
    }
}
