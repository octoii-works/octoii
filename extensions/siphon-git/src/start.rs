//! Durable preparation of a Thread's worktree using the start request's command capability.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use anyhow::Context as _;
use octoii::machine::{Base, Exit, Machine, Program, Writes, inside};
use octoii::threads::{
    Config, InputPart, NewThread, PreparationContext, Start, StartDirectory, StartRequest, StartWorkspace, WorkspacePreparer,
};
use octoii::volume::Volume;

use crate::git::Git;
use crate::protocol::{CreateWorktree, RetryStart};

pub(crate) const OWNER: &str = "git";

pub(crate) struct WorktreePreparation;

impl WorkspacePreparer for WorktreePreparation {
    fn prepare<'a>(
        &'a self,
        context: &'a PreparationContext,
        request: &'a StartRequest,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<StartWorkspace>> + Send + 'a>> {
        Box::pin(prepare(context, request))
    }

    fn inspect<'a>(
        &'a self,
        context: &'a PreparationContext,
        request: &'a StartRequest,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<String>> + Send + 'a>> {
        Box::pin(inspect(context, request))
    }
}

fn base(context: &PreparationContext) -> Base {
    Base {
        workspace: "/".into(),
        home: context.host().home.clone(),
    }
}

fn git<'a>(context: &'a PreparationContext, machine: &'a Machine) -> Git<'a> {
    Git {
        machine,
        commands: context.commands(),
        cancelled: context.cancelled().clone(),
    }
}

/// Resolve the parent's physical path in the request's read-only view before using it as a write scope.
async fn physical_directory(git: &Git<'_>, directory: &Path) -> anyhow::Result<PathBuf> {
    let ran = git
        .machine
        .run(
            git.commands,
            &Program::new(vec!["pwd".into(), "-P".into()], directory),
            &git.cancelled,
        )
        .await
        .with_context(|| format!("cannot open directory {}", directory.display()))?;
    anyhow::ensure!(ran.exit == Exit::Code(0), "cannot resolve directory {}", directory.display());
    let output = std::str::from_utf8(&ran.output).context("directory path must be UTF-8")?;
    Ok(output.trim_suffix('\n').into())
}

async fn prepare(context: &PreparationContext, request: &StartRequest) -> anyhow::Result<StartWorkspace> {
    let worktree: CreateWorktree = serde_json::from_value(request.payload.clone())?;
    let source = Path::new(&worktree.path);
    let destination = Path::new(&worktree.destination);

    let base = base(context);
    let reader = context.open(base.clone(), &Writes::Within(Vec::new()))?;
    let probe = git(context, &reader);
    let root = probe.repository_root(source).await?.context("source is not a repository")?;
    let root_path = Path::new(&root);
    let commit = probe.verify_commit(root_path, &worktree.base).await?;
    let common = probe.common_directory(root_path).await?;
    anyhow::ensure!(
        common.is_absolute() && common.parent().is_some(),
        "invalid Git common directory {}",
        common.display()
    );

    let (parent, name) = destination
        .parent()
        .zip(destination.file_name())
        .context("destination must name a worktree directory")?;
    let parent = physical_directory(&probe, parent).await?;
    anyhow::ensure!(
        parent.is_absolute() && parent.parent().is_some(),
        "invalid worktree parent {}",
        parent.display()
    );
    let target = parent.join(name);
    anyhow::ensure!(
        reader.namespace().stat(&inside(&target))?.is_none(),
        "destination {} already exists",
        target.display()
    );
    anyhow::ensure!(
        !probe
            .worktrees(root_path)
            .await?
            .iter()
            .any(|entry| Path::new(&entry.path) == target.as_path()),
        "destination {} is already registered with Git",
        target.display()
    );

    let writer = context.open(base, &Writes::Within(vec![common, parent]))?;
    let made = git(context, &writer)
        .create_worktree(root_path, &target, &worktree.branch, &commit)
        .await?;
    anyhow::ensure!(
        made.base_commit == commit,
        "Git checked out {} instead of {commit}",
        made.base_commit
    );
    Ok(StartWorkspace {
        workspace: made.path,
        project: Some(root),
    })
}

async fn inspect(context: &PreparationContext, request: &StartRequest) -> anyhow::Result<String> {
    let worktree: CreateWorktree = serde_json::from_value(request.payload.clone())?;
    let source = Path::new(&worktree.path);
    let destination = Path::new(&worktree.destination);
    let reader = context.open(base(context), &Writes::Within(Vec::new()))?;
    let git = git(context, &reader);
    let presence = match reader.namespace().stat(&inside(destination)) {
        Ok(Some(_)) => "exists".to_owned(),
        Ok(None) => "was not found".to_owned(),
        Err(error) => format!("could not be read ({error})"),
    };
    let destination_state = format!("Destination {} {presence}.", destination.display());
    let worktrees = match git.worktrees(source).await {
        Ok(worktrees) => worktrees,
        Err(error) => return Ok(format!("{destination_state} Git could not read worktree registrations ({error}).")),
    };
    let physical_target = match destination.parent().zip(destination.file_name()) {
        Some((parent, name)) => physical_directory(&git, parent).await.ok().map(|parent| parent.join(name)),
        None => None,
    };
    let relevant: Vec<_> = worktrees
        .iter()
        .filter(|entry| {
            let path = Path::new(&entry.path);
            path == destination || physical_target.as_deref() == Some(path) || entry.branch.as_deref() == Some(worktree.branch.as_str())
        })
        .map(|entry| {
            let branch = entry.branch.as_deref().unwrap_or("none");
            let head = entry.head.as_deref().unwrap_or("unknown");
            format!("{:?} (branch {branch:?}, HEAD {head})", entry.path)
        })
        .collect();
    let registration = if relevant.is_empty() {
        "Git has no worktree registration for this destination or branch.".into()
    } else {
        format!("Git registers {}.", relevant.join(", "))
    };
    let branch = &worktree.branch;
    let branch_state = match git.branch_tip(source, branch).await {
        Ok(Some(commit)) => format!("Branch {branch:?} points to {commit}."),
        Ok(None) => format!("Branch {branch:?} was not found."),
        Err(error) => format!("Git could not read branch {branch:?} ({error})."),
    };
    Ok(format!("{destination_state} {registration} {branch_state}"))
}

pub(crate) async fn retry(volume: &Volume, asked: &RetryStart) -> anyhow::Result<Start> {
    let start = volume.thread_start(&asked.thread)?.context("thread has no preparation")?;
    anyhow::ensure!(start.request.owner == OWNER, "thread preparation belongs to another extension");
    volume.retry_thread_start(&asked.thread, &asked.token).await
}

/// Accept a worktree task and its input behind the Thread's startup gate.
/// The host must authorize Git and Thread operations before calling this trusted interface.
///
/// # Errors
/// Invalid task parameters, conflicting idempotency key, attachment processing or storage failure.
pub async fn accept(
    volume: &Volume,
    caller: &str,
    key: &str,
    config: Config,
    content: Vec<InputPart>,
    worktree: CreateWorktree,
) -> anyhow::Result<String> {
    anyhow::ensure!(Path::new(&worktree.path).is_absolute(), "source must be absolute");
    anyhow::ensure!(Path::new(&worktree.destination).is_absolute(), "destination must be absolute");
    anyhow::ensure!(
        crate::git::is_commit_id(&worktree.base),
        "preparation requires a full commit ID, not a moving revision"
    );
    anyhow::ensure!(config.workspace.is_none(), "the workspace is supplied by worktree preparation");
    let new = NewThread {
        actor: "person".into(),
        config,
        project: Some(worktree.path.clone()),
        ..NewThread::default()
    };
    volume
        .start_thread(
            caller,
            key,
            &new,
            content,
            StartDirectory::Prepare(StartRequest {
                owner: OWNER.into(),
                payload: serde_json::to_value(worktree)?,
            }),
        )
        .await
}
