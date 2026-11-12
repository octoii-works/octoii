//! Git's methods, as its extension adds them to the protocol: the repository a folder is in, its changes, a file's patch
//! and its two versions, and a branch's commits — what the Diff view reads.

use serde::{Deserialize, Serialize};
use siphon::protocol::Scope;
use siphon::protocol::siphon::InFolder;
use ts_rs::TS;

use crate::git::{ChangedFile, Changes, ChangesScope, Commit, Comparison, Repository};

siphon::methods! {
    /// The repository a folder of the machine's is in, as git tells of it; none where it is in none.
    "git.repository" GitRepository(InFolder) -> InRepository, Some(Scope::FilesRead);
    /// The changes asked for of the repository a folder is in: each file with the lines it gained and lost.
    "git.changes" GitChanges(GitChanges) -> Changes, Some(Scope::FilesRead);
    /// A file's patch between two trees.
    "git.patch" GitPatch(GitPatch) -> Patch, Some(Scope::FilesRead);
    /// A file's two versions between two trees.
    "git.versions" GitVersions(GitVersions) -> Versions, Some(Scope::FilesRead);
    /// The commits of a branch since it parted from its base.
    "git.commits" GitCommits(GitCommits) -> Commits, Some(Scope::FilesRead);
}

/// The repository a folder is in; none where it is in none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InRepository {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repository: Option<Repository>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GitChanges {
    pub path: String,
    pub scope: ChangesScope,
}

/// A file's patch to read: of the repository `path` is in, between `comparison`'s trees, with `context` lines about each
/// change — git's own number, where none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GitPatch {
    pub path: String,
    pub comparison: Comparison,
    pub file: ChangedFile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub context: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Patch {
    pub patch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GitVersions {
    pub path: String,
    pub comparison: Comparison,
    pub file: ChangedFile,
}

/// A file's two versions: the older, none where it came; the newer, none where it went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Versions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub old: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub new: Option<String>,
}

/// The commits to list: of the branch the folder `path` is on, since it parted from `base` — the repository's, where
/// none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GitCommits {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base: Option<String>,
}

/// A branch's commits, the newest first, and whether more were left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Commits {
    pub commits: Vec<Commit>,
    pub truncated: bool,
}
