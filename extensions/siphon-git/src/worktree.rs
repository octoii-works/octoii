//! Node-owned paths and names. Planning reads Git but never allocates filesystem resources.
use std::path::Path;

use crate::git::Git;
use crate::protocol::{CreateWorktree, WorktreePlan};

pub(crate) async fn plan(git: &Git<'_>, directory: &Path, asked: WorktreePlan) -> anyhow::Result<CreateWorktree> {
    let key = &asked.key;
    anyhow::ensure!(
        key.len() == 36
            && key.bytes().enumerate().all(|(index, byte)| {
                if matches!(index, 8 | 13 | 18 | 23) {
                    byte == b'-'
                } else {
                    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
                }
            }),
        "task key must be a lowercase UUID"
    );
    anyhow::ensure!(directory.is_absolute(), "managed worktree directory must be absolute");
    let source = Path::new(&asked.path);
    anyhow::ensure!(source.is_absolute(), "source must be absolute");
    let repository = git
        .repository(source)
        .await?
        .ok_or_else(|| anyhow::anyhow!("source is not a repository"))?;
    let base = git.verify_commit(source, &asked.base).await?;
    let destination = directory
        .join(key)
        .into_os_string()
        .into_string()
        .map_err(|_| anyhow::anyhow!("managed worktree path must be UTF-8"))?;
    Ok(CreateWorktree {
        path: repository.root,
        destination,
        branch: format!("octoii/{key}"),
        base,
    })
}
