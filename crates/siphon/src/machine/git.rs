//! The repositories the machine holds, as git tells of them — git itself run on the machine, so what the person's
//! repository, configuration and git say is what is told: where a folder's repository begins and how its branch
//! stands against its upstream and its base; the changes not yet committed, a branch's against where it parted from its
//! base, or one commit's — each file changed with the lines it gained and lost; a file's patch, and its two versions;
//! and the commits of a branch since it parted from its base. Its output is read as git writes it with `-z`: names
//! whole, whatever they hold.

use std::path::Path;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::machine::namespace::{Machine, Program, RunError};

/// The most commits of a branch told at once.
const COMMITS_MAX: usize = 200;

/// A repository, as a folder in it is: where its work tree begins, its branch — none where its head is detached — its
/// head's commit — none before the first — its upstream, how far ahead of it and behind, and the branch its work was
/// taken from, as its changes are told against unless one is asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Repository {
    pub root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base: Option<String>,
}

/// Which changes are asked for: those not yet committed; a branch's against where it parted from its base — the
/// repository's, where none is named — the working tree's own included; or one commit's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChangesScope {
    Uncommitted,
    Branch {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        base: Option<String>,
    },
    Commit {
        commit: String,
    },
}

/// The two trees changes are told between: the older, by its object's id; the newer, a commit's, or the working
/// tree's where none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Comparison {
    pub old: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub new: Option<String>,
}

/// What became of a file between two trees; untracked, one in the working tree git does not keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
}

/// A file changed: its path from the repository's root, the one it had, what became of it, the lines it gained and
/// lost — none where they were not counted — and whether it is binary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ChangedFile {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub old_path: Option<String>,
    pub status: ChangeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub additions: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub deletions: Option<u32>,
    pub binary: bool,
}

/// The changes asked for: the repository's root, the trees they are between, the branch they are told against, and
/// the files changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Changes {
    pub root: String,
    pub comparison: Comparison,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base: Option<String>,
    pub files: Vec<ChangedFile>,
}

/// A commit of a branch: its id, when it was committed, in seconds since the epoch, and its subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Commit {
    pub commit: String,
    pub time: i64,
    pub subject: String,
}

/// Why git could not tell what was asked.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("no repository holds {0}")]
    NoRepository(String),
    #[error("git {command}: {said}")]
    Refused { command: String, said: String },
    #[error(transparent)]
    Run(#[from] RunError),
}

/// Git on the machine: runs recorded under the process `under`, at the time `time`, stopped as `cancelled` says.
#[derive(Debug)]
pub struct Git<'a> {
    pub machine: &'a Machine,
    pub under: &'a [u8],
    pub time: i64,
    pub cancelled: CancellationToken,
}

/// The fields of `-z` output.
fn fields(output: &[u8]) -> impl Iterator<Item = &[u8]> {
    output.split(|&byte| byte == 0).filter(|field| !field.is_empty())
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

impl Git<'_> {
    /// What git writes, run with `arguments` in `cwd`, a machine's absolute path: its output, where it ended as
    /// `succeeded` takes it to have.
    async fn ran(&self, cwd: &Path, arguments: &[&str], succeeded: impl Fn(i32) -> bool) -> Result<Bytes, GitError> {
        let mut argv = vec![
            "git".to_owned(),
            "--no-pager".to_owned(),
            "-c".to_owned(),
            "core.quotepath=off".to_owned(),
        ];
        argv.extend(arguments.iter().map(|argument| (*argument).to_owned()));
        let ran = self
            .machine
            .run(self.time, self.under, &Program::new(argv, cwd), &self.cancelled)
            .await?;
        match ran.exit {
            beak::Exit::Code(code) if succeeded(code) => Ok(ran.output),
            _ => Err(GitError::Refused {
                command: arguments.join(" "),
                said: text(&ran.errors).trim().to_owned(),
            }),
        }
    }

    async fn output(&self, cwd: &Path, arguments: &[&str]) -> Result<Bytes, GitError> {
        self.ran(cwd, arguments, |code| code == 0).await
    }

    /// What git writes run in `cwd`, trimmed; none where it says nothing is there.
    async fn line(&self, cwd: &Path, arguments: &[&str]) -> Result<Option<String>, GitError> {
        let output = self.ran(cwd, arguments, |code| code <= 1).await?;
        let line = text(&output).trim().to_owned();
        Ok((!line.is_empty()).then_some(line))
    }

    /// The repository the folder `folder` — a machine's absolute path — is in; none where it is in none.
    ///
    /// # Errors
    /// Where git could not tell.
    pub async fn repository(&self, folder: &Path) -> Result<Option<Repository>, GitError> {
        let Ok(top) = self.output(folder, &["rev-parse", "--show-toplevel"]).await else {
            return Ok(None);
        };
        let root = text(&top).trim().to_owned();
        let status = self
            .output(
                Path::new(&root),
                &["status", "--porcelain=v2", "--branch", "-z", "--untracked-files=no"],
            )
            .await?;
        let mut repository = Repository {
            root,
            branch: None,
            head: None,
            upstream: None,
            ahead: 0,
            behind: 0,
            base: None,
        };
        for header in fields(&status).filter_map(|field| field.strip_prefix(b"# ")) {
            let header = text(header);
            let (name, value) = header.split_once(' ').unwrap_or((&header, ""));
            match name {
                "branch.oid" if value != "(initial)" => repository.head = Some(value.to_owned()),
                "branch.head" if value != "(detached)" => repository.branch = Some(value.to_owned()),
                "branch.upstream" => repository.upstream = Some(value.to_owned()),
                "branch.ab" => {
                    let mut counts = value
                        .split(' ')
                        .map(|count| count.trim_start_matches(['+', '-']).parse().unwrap_or(0));
                    (repository.ahead, repository.behind) = (counts.next().unwrap_or(0), counts.next().unwrap_or(0));
                }
                _ => {}
            }
        }
        repository.base = self.base(&repository).await?;
        Ok(Some(repository))
    }

    /// The branch a repository's work is taken from: its upstream's remote's default branch; else a local `main` or
    /// `master`, the one it is not on.
    async fn base(&self, repository: &Repository) -> Result<Option<String>, GitError> {
        let root = Path::new(&repository.root);
        let remote = repository
            .upstream
            .as_deref()
            .and_then(|upstream| upstream.split_once('/'))
            .map_or("origin", |(remote, _)| remote);
        let symbolic = format!("refs/remotes/{remote}/HEAD");
        if let Some(head) = self.line(root, &["symbolic-ref", "-q", "--short", &symbolic]).await? {
            return Ok(Some(head));
        }
        for local in ["main", "master"] {
            let reference = format!("refs/heads/{local}");
            if repository.branch.as_deref() != Some(local) && self.line(root, &["rev-parse", "-q", "--verify", &reference]).await?.is_some()
            {
                return Ok(Some(local.to_owned()));
            }
        }
        Ok(None)
    }
}

/// What `-z` raw and numstat output, together, says of each file changed: the raw entries first, then a count of
/// each one's lines, in the same order — a rename's or a copy's naming its two paths after its count.
fn parsed(output: &[u8]) -> Vec<ChangedFile> {
    let parts: Vec<&[u8]> = output.split(|&byte| byte == 0).collect();
    let mut at = 0;
    let mut changed = Vec::new();
    while at < parts.len() && parts[at].starts_with(b":") {
        let meta = text(parts[at]);
        let letter = meta.split(' ').nth(4).and_then(|status| status.chars().next()).unwrap_or('M');
        let status = match letter {
            'A' => ChangeStatus::Added,
            'D' => ChangeStatus::Deleted,
            'R' => ChangeStatus::Renamed,
            'C' => ChangeStatus::Copied,
            'T' => ChangeStatus::TypeChanged,
            _ => ChangeStatus::Modified,
        };
        let two = matches!(status, ChangeStatus::Renamed | ChangeStatus::Copied);
        let (old_path, path) = if two {
            (
                parts.get(at + 1).map(|old| text(old)),
                parts.get(at + 2).map_or_else(String::new, |path| text(path)),
            )
        } else {
            (None, parts.get(at + 1).map_or_else(String::new, |path| text(path)))
        };
        at += if two { 3 } else { 2 };
        changed.push(ChangedFile {
            path,
            old_path,
            status,
            additions: None,
            deletions: None,
            binary: false,
        });
    }
    for file in &mut changed {
        let Some(count) = parts.get(at) else { break };
        let count = text(count);
        let mut counted = count.splitn(3, '\t');
        let (added, deleted, named) = (
            counted.next().unwrap_or(""),
            counted.next().unwrap_or(""),
            counted.next().unwrap_or(""),
        );
        file.binary = added == "-";
        file.additions = added.parse().ok();
        file.deletions = deleted.parse().ok();
        // A rename's count names nothing on its own line: its two paths follow it.
        at += if named.is_empty() { 3 } else { 1 };
    }
    changed
}

impl Git<'_> {
    /// The repository `folder` is in.
    async fn holding(&self, folder: &Path) -> Result<Repository, GitError> {
        self.repository(folder)
            .await?
            .ok_or_else(|| GitError::NoRepository(folder.display().to_string()))
    }

    /// The id of the tree of nothing, of the repository's own kind of object id.
    async fn empty_tree(&self, root: &Path) -> Result<String, GitError> {
        Ok(text(&self.output(root, &["hash-object", "-t", "tree", "/dev/null"]).await?)
            .trim()
            .to_owned())
    }

    /// The trees `scope`'s changes are told between, and the branch they are told against.
    async fn compared(&self, repository: &Repository, scope: &ChangesScope) -> Result<(Comparison, Option<String>), GitError> {
        let root = Path::new(&repository.root);
        match scope {
            ChangesScope::Uncommitted => {
                let old = match &repository.head {
                    Some(head) => head.clone(),
                    None => self.empty_tree(root).await?,
                };
                Ok((Comparison { old, new: None }, None))
            }
            ChangesScope::Branch { base } => {
                let base = base.clone().or_else(|| repository.base.clone()).ok_or_else(|| GitError::Refused {
                    command: "merge-base".into(),
                    said: "the repository names no branch its work was taken from".into(),
                })?;
                let parted = text(&self.output(root, &["merge-base", &base, "HEAD"]).await?).trim().to_owned();
                Ok((Comparison { old: parted, new: None }, Some(base)))
            }
            ChangesScope::Commit { commit } => {
                let new = text(
                    &self
                        .output(root, &["rev-parse", "--verify", &format!("{commit}^{{commit}}")])
                        .await?,
                )
                .trim()
                .to_owned();
                let old = match self.line(root, &["rev-parse", "-q", "--verify", &format!("{new}^")]).await? {
                    Some(parent) => parent,
                    None => self.empty_tree(root).await?,
                };
                Ok((Comparison { old, new: Some(new) }, None))
            }
        }
    }

    /// The changes `scope` asks for, of the repository `folder` is in.
    ///
    /// # Errors
    /// `NoRepository`; where git could not tell.
    pub async fn changes(&self, folder: &Path, scope: &ChangesScope) -> Result<Changes, GitError> {
        let repository = self.holding(folder).await?;
        let root = Path::new(&repository.root);
        let (comparison, base) = self.compared(&repository, scope).await?;
        let mut arguments = vec![
            "diff",
            "--raw",
            "--numstat",
            "-z",
            "--find-renames",
            "--no-color",
            "--no-ext-diff",
            &comparison.old,
        ];
        if let Some(new) = &comparison.new {
            arguments.push(new);
        }
        let mut files = parsed(&self.output(root, &arguments).await?);
        if comparison.new.is_none() {
            let untracked = self.output(root, &["ls-files", "--others", "--exclude-standard", "-z"]).await?;
            files.extend(fields(&untracked).map(|path| ChangedFile {
                path: text(path),
                old_path: None,
                status: ChangeStatus::Untracked,
                additions: None,
                deletions: None,
                binary: false,
            }));
        }
        Ok(Changes {
            root: repository.root,
            comparison,
            base,
            files,
        })
    }

    /// The patch of `file` between `comparison`'s trees, of the repository `folder` is in, with `context` lines about
    /// each change — git's own number, where none.
    ///
    /// # Errors
    /// `NoRepository`; where git could not tell.
    pub async fn patch(
        &self,
        folder: &Path,
        comparison: &Comparison,
        file: &ChangedFile,
        context: Option<u32>,
    ) -> Result<String, GitError> {
        let repository = self.holding(folder).await?;
        let root = Path::new(&repository.root);
        let unified = context.map(|context| format!("-U{context}"));
        let mut arguments = vec!["diff", "--no-color", "--no-ext-diff"];
        arguments.extend(unified.as_deref());
        if file.status == ChangeStatus::Untracked {
            arguments.extend(["--no-index", "--", "/dev/null", &file.path]);
            // A file compared with nothing differs from it: git says so by its exit.
            return Ok(text(&self.ran(root, &arguments, |code| code <= 1).await?));
        }
        arguments.extend(["--find-renames", &comparison.old]);
        arguments.extend(comparison.new.as_deref());
        arguments.push("--");
        arguments.extend(file.old_path.as_deref());
        arguments.push(&file.path);
        Ok(text(&self.output(root, &arguments).await?))
    }

    /// The two versions of `file` between `comparison`'s trees, of the repository `folder` is in: the older, none where
    /// it came; the newer, none where it went — the working tree's read as it is.
    ///
    /// # Errors
    /// `NoRepository`; where git could not tell.
    pub async fn versions(
        &self,
        folder: &Path,
        comparison: &Comparison,
        file: &ChangedFile,
    ) -> Result<(Option<String>, Option<String>), GitError> {
        let repository = self.holding(folder).await?;
        let root = Path::new(&repository.root);
        let came = matches!(file.status, ChangeStatus::Added | ChangeStatus::Untracked);
        let old = if came {
            None
        } else {
            let named = format!("{}:{}", comparison.old, file.old_path.as_deref().unwrap_or(&file.path));
            Some(text(&self.output(root, &["cat-file", "blob", &named]).await?))
        };
        let new = match (&comparison.new, file.status) {
            (_, ChangeStatus::Deleted) => None,
            (Some(new), _) => Some(text(
                &self.output(root, &["cat-file", "blob", &format!("{new}:{}", file.path)]).await?,
            )),
            (None, _) => {
                let path = crate::machine::namespace::inside(&root.join(&file.path));
                let namespace = self.machine.namespace();
                let size = namespace.stat(&path).map_err(RunError::Beak)?.map_or(0, |stat| stat.size);
                Some(text(&namespace.read(&path, 0, size).map_err(RunError::Beak)?))
            }
        };
        Ok((old, new))
    }

    /// The commits of the branch the folder `folder` is on since it parted from `base` — the repository's, where none
    /// — the newest first, and whether more were left out.
    ///
    /// # Errors
    /// `NoRepository`; where git could not tell.
    pub async fn commits(&self, folder: &Path, base: Option<&str>) -> Result<(Vec<Commit>, bool), GitError> {
        let repository = self.holding(folder).await?;
        let root = Path::new(&repository.root);
        let (comparison, _) = self
            .compared(
                &repository,
                &ChangesScope::Branch {
                    base: base.map(str::to_owned),
                },
            )
            .await?;
        let limit = format!("--max-count={}", COMMITS_MAX + 1);
        let range = format!("{}..HEAD", comparison.old);
        let told = self
            .output(root, &["log", "-z", "--format=%H%x1f%ct%x1f%s", &limit, &range])
            .await?;
        let mut commits: Vec<Commit> = fields(&told)
            .filter_map(|record| {
                let record = text(record);
                let mut parts = record.trim_start_matches('\n').splitn(3, '\u{1f}');
                Some(Commit {
                    commit: parts.next()?.to_owned(),
                    time: parts.next()?.parse().ok()?,
                    subject: parts.next().unwrap_or_default().to_owned(),
                })
            })
            .collect();
        let truncated = commits.len() > COMMITS_MAX;
        commits.truncate(COMMITS_MAX);
        Ok((commits, truncated))
    }
}
