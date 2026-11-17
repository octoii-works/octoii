//! Repository and worktree operations on the calling thread's machine.

use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use siphon::machine::Base;
use siphon::tools::{Context, Effect, Refusal, Told, Tool};

use crate::git::{Changes, ChangesScope, Git, Repository, Worktree, WorktreeEntry};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    /// Repository, branch and uncommitted changes in the thread's workspace.
    Status,
    /// Registered worktrees, including lock and pruning information.
    ListWorktrees,
    /// Create a new linked worktree and branch from a committed revision. Does not switch the thread.
    CreateWorktree {
        /// New directory; relative to the workspace, absolute, or starting with ~/. Its parent must exist.
        path: String,
        /// New literal branch name.
        branch: String,
        /// Commit, branch or tag to start from; HEAD if omitted.
        base: Option<String>,
    },
    /// Remove a clean, unlocked linked worktree, retaining its branch. Never forces removal.
    RemoveWorktree { path: String },
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GitResult {
    Repository { repository: Repository, changes: Changes },
    NoRepository,
    Worktrees { worktrees: Vec<WorktreeEntry> },
    Created { worktree: Worktree },
    Removed { path: String },
}

#[derive(Debug, Clone, Copy)]
pub struct GitTool;

impl Tool for GitTool {
    const NAME: &'static str = "git";
    const DESCRIPTION: &'static str = "Inspect the workspace repository and manage linked worktrees. status returns the repository and uncommitted changes; list_worktrees lists registered paths, branches and locks. create_worktree creates a new directory and branch from a committed base (HEAD by default); uncommitted work stays in its original directory. remove_worktree removes only a clean, unlocked linked worktree and keeps its branch. Neither operation switches this thread's workspace or context. Use shell for commit, push, merge and other Git commands. Creation failures may leave a directory or branch: inspect before retrying; nothing is automatically deleted.";
    type Args = Action;

    fn output_schema() -> Option<serde_json::Value> {
        Some(schemars::schema_for!(GitResult).to_value())
    }

    fn effect(&self, action: &Action, _: &Base) -> Effect {
        match action {
            Action::Status | Action::ListWorktrees => Effect::Reads,
            Action::CreateWorktree { .. } => Effect::Acts {
                tool: "git.create_worktree".into(),
            },
            Action::RemoveWorktree { .. } => Effect::Acts {
                tool: "git.remove_worktree".into(),
            },
        }
    }

    async fn exec(&self, context: &Context, action: Action) -> Result<Told, Refusal> {
        let machine = context.machine()?;
        let git = Git {
            machine: &machine,
            under: &context.process,
            time: context.volume().now_ms(),
            cancelled: context.cancelled.clone(),
        };
        let folder = &context.base.workspace;
        let result = match action {
            Action::Status => match git.repository(folder).await.map_err(refused)? {
                Some(repository) => GitResult::Repository {
                    repository,
                    changes: git.changes(folder, &ChangesScope::Uncommitted).await.map_err(refused)?,
                },
                None => GitResult::NoRepository,
            },
            Action::ListWorktrees => GitResult::Worktrees {
                worktrees: git.worktrees(folder).await.map_err(refused)?,
            },
            Action::CreateWorktree { path, branch, base } => GitResult::Created {
                worktree: git
                    .create_worktree(folder, &context.base.resolve(&path), &branch, base.as_deref().unwrap_or("HEAD"))
                    .await
                    .map_err(refused)?,
            },
            Action::RemoveWorktree { path } => {
                let destination = context.base.resolve(&path);
                git.remove_worktree(folder, &destination).await.map_err(refused)?;
                GitResult::Removed {
                    path: destination.to_string_lossy().into_owned(),
                }
            }
        };
        let data = serde_json::to_value(result).map_err(refused)?;
        Ok(Told {
            content: vec![InputContent::text(siphon::reminder::escaped(&data.to_string()).into_owned())],
            structured_content: Some(data),
            ..Told::default()
        })
    }
}

fn refused(error: impl std::fmt::Display) -> Refusal {
    Refusal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn actions_have_strict_parameters_and_distinct_mutation_permissions() {
        let schema = schemars::schema_for!(Action).to_value();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let base = Base {
            workspace: "/work".into(),
            home: None,
        };
        for value in [
            json!({"action": "status"}),
            json!({"action": "list_worktrees"}),
            json!({"action": "create_worktree", "path": "../new", "branch": "topic"}),
            json!({"action": "remove_worktree", "path": "../new"}),
        ] {
            assert!(validator.is_valid(&value));
            let action: Action = serde_json::from_value(value).unwrap();
            match &action {
                Action::Status | Action::ListWorktrees => assert_eq!(GitTool.effect(&action, &base), Effect::Reads),
                Action::CreateWorktree { .. } => assert_eq!(
                    GitTool.effect(&action, &base),
                    Effect::Acts {
                        tool: "git.create_worktree".into()
                    }
                ),
                Action::RemoveWorktree { .. } => assert_eq!(
                    GitTool.effect(&action, &base),
                    Effect::Acts {
                        tool: "git.remove_worktree".into()
                    }
                ),
            }
        }
        for value in [
            json!({"action": "create_worktree", "path": "../new"}),
            json!({"action": "remove_worktree", "path": "../new", "force": true}),
            json!({"action": "create_worktree", "path": "../new", "branch": "topic", "switch": true}),
        ] {
            assert!(!validator.is_valid(&value));
            assert!(serde_json::from_value::<Action>(value).is_err());
        }
    }
}
