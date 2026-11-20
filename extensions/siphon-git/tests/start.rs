//! Git preparation advances through the Thread's durable startup gate.

use std::path::{Path, PathBuf};
use std::time::Duration;

use octoii::{
    Octoii,
    protocol::Scope,
    testing::{self, Client},
    threads::{Config, Start, StartState},
};
use octoii_ai::vocabulary::InputContent;
use serde_json::{Value, json};
use siphon_git::start::WorktreeIntent;

const SESSION: &str = "session-1";
const KEY: &str = "first-send";

fn git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgSign=false",
        ])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn repository(root: &Path) {
    git(root, &["init", "--template="]);
    std::fs::write(root.join("tracked"), "original").unwrap();
    git(root, &["add", "tracked"]);
    git(root, &["commit", "-m", "base"]);
}

fn checkout_paths(folder: &Path) -> (PathBuf, PathBuf) {
    let folder = folder.canonicalize().unwrap();
    let source = folder.join("source");
    std::fs::create_dir(&source).unwrap();
    repository(&source);
    (source, folder.join("worktrees"))
}

fn node(storage: Option<&Path>) -> Octoii {
    let mut setup = testing::setup(octoii::machine::Host::default());
    setup.directory = storage.map(Path::to_path_buf);
    Octoii::builder(setup)
        .unwrap()
        .extension(siphon_base::BaseExtension::default())
        .extension(siphon_git::GitExtension)
        .start()
        .unwrap()
}

async fn client(node: &Octoii, scopes: Vec<Scope>) -> Client {
    let mut client = Client::connect(node, scopes, 64);
    client.message().await;
    client
}

async fn reply(client: &mut Client, method: &str, params: Value) -> Value {
    client.command(method, params).await
}

async fn call(client: &mut Client, method: &str, params: Value) -> Value {
    let answer = reply(client, method, params).await;
    assert!(answer["error"].is_null(), "{answer}");
    answer["result"].clone()
}

fn worktree(root: &Path, base: Option<String>) -> WorktreeIntent {
    WorktreeIntent {
        path: root.to_str().unwrap().to_owned(),
        base,
    }
}

async fn accept(node: &Octoii, worktrees: &Path, caller: &str, worktree: &WorktreeIntent) -> anyhow::Result<String> {
    siphon_git::start::accept(
        node,
        || Ok(worktrees.to_path_buf()),
        caller,
        KEY,
        Config {
            title: Some("task".into()),
            ..Config::default()
        },
        vec![InputContent::text("keep my input").into()],
        worktree.clone(),
    )
    .await
}

async fn settled(node: &Octoii, thread: &str) -> Start {
    let volume = node.volume();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let start = volume.thread_start(thread).unwrap().unwrap();
            if !matches!(start.state, StartState::Pending | StartState::Preparing | StartState::Checking) {
                break start;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("preparation settled")
}

#[cfg(unix)]
fn checkout_hook(root: &Path, script: &str) {
    use std::os::unix::fs::PermissionsExt;

    let hooks = root.join(".git").join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    git(root, &["config", "core.hooksPath", hooks.to_str().unwrap()]);
    let hook = hooks.join("post-checkout");
    std::fs::write(&hook, script).unwrap();
    std::fs::set_permissions(hook, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[cfg(unix)]
fn hold_checkout(root: &Path) {
    checkout_hook(
        root,
        "#!/bin/sh\nprintf checkout > .checkout-started\nwhile [ ! -f .release-checkout ]; do sleep 0.01; done\n",
    );
}

#[cfg(unix)]
async fn checkout_started(destination: &Path) {
    tokio::time::timeout(Duration::from_secs(15), async {
        while !destination.join(".checkout-started").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Git ran the post-checkout hook");
}

#[tokio::test(flavor = "multi_thread")]
async fn acceptance_prepares_at_the_frozen_commit_without_a_client() {
    let folder = tempfile::tempdir().unwrap();
    let (root, worktrees) = checkout_paths(folder.path());
    std::fs::create_dir(&worktrees).unwrap();
    let base = git(&root, &["rev-parse", "HEAD"]);
    let request = worktree(&root, Some(base.clone()));
    std::fs::write(root.join("tracked"), "later").unwrap();
    git(&root, &["commit", "-am", "later"]);

    let node = node(None);
    for moving_base in ["HEAD", "refs/heads/main", "--help"] {
        let invalid = worktree(&root, Some(moving_base.into()));
        let error = accept(&node, &worktrees, SESSION, &invalid).await.unwrap_err();
        assert!(error.to_string().contains("full commit ID"), "{error}");
    }
    let unknown = worktree(&root, Some("0".repeat(base.len())));
    assert!(accept(&node, &worktrees, SESSION, &unknown).await.is_err());
    assert_eq!(worktrees.read_dir().unwrap().count(), 0);

    let (accepted, replayed) = tokio::join!(
        accept(&node, &worktrees, SESSION, &request),
        accept(&node, &worktrees, SESSION, &request)
    );
    let thread = accepted.unwrap();
    assert_eq!(replayed.unwrap(), thread);
    let destination = worktrees.join(&thread);
    let start = settled(&node, &thread).await;
    let StartState::Ready { directory } = &start.state else {
        panic!("{start:?}");
    };
    assert_eq!(directory.workspace, destination.to_str().unwrap());
    assert_eq!(directory.project.as_deref(), root.to_str());
    assert_eq!(
        node.threads.listed(&thread).unwrap().config.workspace.as_deref(),
        destination.to_str()
    );
    assert_eq!(git(&destination, &["rev-parse", "HEAD"]), base);
    assert_eq!(git(&destination, &["branch", "--show-current"]), format!("octoii/{thread}"));
    assert_eq!(std::fs::read_to_string(destination.join("tracked")).unwrap(), "original");

    std::fs::write(destination.join("tracked"), "keep this edit").unwrap();
    std::fs::rename(&root, root.with_file_name("unavailable")).unwrap();
    assert_eq!(accept(&node, &worktrees, SESSION, &request).await.unwrap(), thread);
    assert_eq!(node.volume().thread_start(&thread).unwrap(), Some(start));
    assert_eq!(std::fs::read_to_string(destination.join("tracked")).unwrap(), "keep this edit");
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn existing_worktree_keeps_its_files_and_failed_preparation_keeps_its_input() {
    let folder = tempfile::tempdir().unwrap();
    let (root, worktrees) = checkout_paths(folder.path());

    let node = node(None);
    let thread = accept(&node, &worktrees, SESSION, &worktree(&root, None)).await.unwrap();
    let failed = settled(&node, &thread).await;
    assert!(matches!(failed.state, StartState::Failed { .. }), "{failed:?}");
    let listed = node.threads.listed(&thread).unwrap();
    assert!(listed.config.workspace.is_none());
    assert_eq!(listed.status.inbox.len(), 1);

    std::fs::create_dir(&worktrees).unwrap();
    let destination = worktrees.join(&thread);
    git(
        &root,
        &[
            "worktree",
            "add",
            "-b",
            &format!("octoii/{thread}"),
            destination.to_str().unwrap(),
            "HEAD",
        ],
    );
    std::fs::write(destination.join("keep"), "other work").unwrap();
    let mut client = client(&node, vec![Scope::GitOperate, Scope::ThreadsOperate]).await;
    let retry = call(&mut client, "git.retry-start", json!({"thread":thread, "token":failed.token})).await;
    let retried = settled(&node, &thread).await;
    let StartState::Failed { reason } = &retried.state else {
        panic!("{retried:?}");
    };
    assert!(reason.contains("already exists"), "{reason}");
    assert_eq!(retry["token"], retried.token);
    assert_ne!(retried.token, failed.token);
    assert_eq!(retried.request, failed.request);
    assert_eq!(node.threads.listed(&thread).unwrap().status.inbox, listed.status.inbox);
    assert_eq!(std::fs::read_to_string(destination.join("keep")).unwrap(), "other work");
    node.close().await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn stopping_the_current_attempt_after_checkout_preserves_the_gate() {
    let folder = tempfile::tempdir().unwrap();
    let (root, worktrees) = checkout_paths(folder.path());
    let node = node(None);
    let thread = accept(&node, &worktrees, SESSION, &worktree(&root, None)).await.unwrap();
    let destination = worktrees.join(&thread);
    let failed = settled(&node, &thread).await;
    assert!(matches!(failed.state, StartState::Failed { .. }), "{failed:?}");
    let inbox = node.threads.listed(&thread).unwrap().status.inbox;

    std::fs::create_dir(&worktrees).unwrap();
    hold_checkout(&root);
    let mut client = client(&node, vec![Scope::GitOperate, Scope::ThreadsOperate]).await;
    let retry = call(&mut client, "git.retry-start", json!({"thread":thread, "token":failed.token})).await;
    checkout_started(&destination).await;
    assert_eq!(node.volume().thread_start(&thread).unwrap().unwrap().state, StartState::Preparing);

    let refused = reply(
        &mut client,
        "threads.stop",
        json!({"thread":thread, "target":{"kind":"preparation", "token":failed.token}}),
    )
    .await;
    assert_eq!(refused["error"]["code"], "stale_stop", "{refused}");
    call(
        &mut client,
        "threads.stop",
        json!({"thread":thread, "target":{"kind":"preparation", "token":retry["token"]}}),
    )
    .await;
    std::fs::write(destination.join(".release-checkout"), "release").unwrap();

    let listed = node.threads.listed(&thread).unwrap();
    assert_eq!(listed.status.start.unwrap().state, StartState::Cancelled);
    assert!(listed.config.workspace.is_none());
    assert_eq!(listed.status.inbox, inbox);
    assert_eq!(std::fs::read_to_string(destination.join("tracked")).unwrap(), "original");
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn retry_requires_both_git_and_thread_operation_permissions() {
    let folder = tempfile::tempdir().unwrap();
    let (root, worktrees) = checkout_paths(folder.path());
    let node = node(None);
    let thread = accept(&node, &worktrees, SESSION, &worktree(&root, None)).await.unwrap();
    let failed = settled(&node, &thread).await;
    assert!(matches!(failed.state, StartState::Failed { .. }), "{failed:?}");
    for scopes in [vec![Scope::ThreadsOperate], vec![Scope::GitOperate]] {
        let mut client = client(&node, scopes).await;
        let denied = reply(&mut client, "git.retry-start", json!({"thread":thread, "token":failed.token})).await;
        assert_eq!(denied["error"]["code"], "forbidden", "{denied}");
    }
    assert_eq!(node.volume().thread_start(&thread).unwrap(), Some(failed));
    assert!(!worktrees.join(&thread).exists());
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reopening_preserves_the_failed_request_for_an_explicit_retry() {
    let folder = tempfile::tempdir().unwrap();
    let (root, worktrees) = checkout_paths(folder.path());
    let base = git(&root, &["rev-parse", "HEAD"]);
    let storage = tempfile::tempdir().unwrap();
    let request = worktree(&root, None);

    // Reopening models a new node and executor.
    let (thread, failed) = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                tokio::runtime::Runtime::new().unwrap().block_on(async {
                    let first = node(Some(storage.path()));
                    let thread = accept(&first, &worktrees, SESSION, &request).await.unwrap();
                    let failed = settled(&first, &thread).await;
                    assert!(matches!(failed.state, StartState::Failed { .. }), "{failed:?}");
                    first.close().await;
                    (thread, failed)
                })
            })
            .join()
            .unwrap()
    });
    let destination = worktrees.join(&thread);
    assert_eq!(failed.request.payload["base"], base);
    std::fs::write(root.join("tracked"), "later").unwrap();
    git(&root, &["commit", "-am", "later"]);
    let second = node(Some(storage.path()));
    assert_eq!(accept(&second, &worktrees, SESSION, &request).await.unwrap(), thread);
    assert_eq!(second.volume().thread_start(&thread).unwrap(), Some(failed.clone()));
    assert!(!destination.exists());

    std::fs::create_dir(&worktrees).unwrap();
    let mut after = client(&second, vec![Scope::GitOperate, Scope::ThreadsOperate]).await;
    let retried = call(&mut after, "git.retry-start", json!({"thread":thread, "token":failed.token})).await;
    let ready = settled(&second, &thread).await;
    assert!(matches!(ready.state, StartState::Ready { .. }), "{ready:?}");
    assert_ne!(ready.token, failed.token);
    assert_eq!(retried["token"], ready.token);
    let replayed = call(&mut after, "git.retry-start", json!({"thread":thread, "token":failed.token})).await;
    assert_eq!(serde_json::from_value::<Start>(replayed).unwrap(), ready);
    assert_eq!(git(&destination, &["rev-parse", "HEAD"]), base);
    assert_eq!(std::fs::read_to_string(destination.join("tracked")).unwrap(), "original");
    second.close().await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn reopening_inspects_an_interrupted_checkout_and_preserves_the_gate() {
    let folder = tempfile::tempdir().unwrap();
    let (root, worktrees) = checkout_paths(folder.path());
    std::fs::create_dir(&worktrees).unwrap();
    hold_checkout(&root);
    let base = git(&root, &["rev-parse", "HEAD"]);
    let request = worktree(&root, None);
    let storage = tempfile::tempdir().unwrap();

    let (thread, preparing) = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                tokio::runtime::Runtime::new().unwrap().block_on(async {
                    let first = node(Some(storage.path()));
                    let thread = accept(&first, &worktrees, SESSION, &request).await.unwrap();
                    checkout_started(&worktrees.join(&thread)).await;
                    let preparing = first.volume().thread_start(&thread).unwrap().unwrap();
                    assert_eq!(preparing.state, StartState::Preparing);
                    first.close().await;
                    (thread, preparing)
                })
            })
            .join()
            .unwrap()
    });
    let destination = worktrees.join(&thread);
    std::fs::write(destination.join(".release-checkout"), "release").unwrap();
    std::fs::write(destination.join("tracked"), "keep this edit").unwrap();

    let reopened = node(Some(storage.path()));
    let interrupted = settled(&reopened, &thread).await;
    let StartState::Interrupted { reason } = &interrupted.state else {
        panic!("{interrupted:?}");
    };
    assert!(reason.contains("exists") && reason.contains("Git registers"), "{reason}");
    assert!(reason.contains(&base), "{reason}");
    assert_eq!(interrupted.token, preparing.token);
    assert_eq!(interrupted.request, preparing.request);
    assert_eq!(git(&root, &["worktree", "list", "--porcelain"]).matches("worktree ").count(), 2);
    assert_eq!(std::fs::read_to_string(destination.join("tracked")).unwrap(), "keep this edit");

    let listed = reopened.threads.listed(&thread).unwrap();
    assert!(listed.config.workspace.is_none());
    assert_eq!(listed.status.inbox.len(), 1);
    let mut client = client(&reopened, vec![Scope::GitOperate, Scope::ThreadsOperate]).await;
    let refused = reply(&mut client, "git.retry-start", json!({"thread":thread, "token":interrupted.token})).await;
    assert!(!refused["error"].is_null(), "{refused}");
    assert_eq!(reopened.volume().thread_start(&thread).unwrap(), Some(interrupted));
    reopened.close().await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_linked_source_uses_the_common_git_directory_and_confines_hook_writes() {
    let folder = tempfile::tempdir().unwrap();
    let (source, worktrees) = checkout_paths(folder.path());
    let root = source.parent().unwrap();
    let linked = root.join("linked");
    std::fs::create_dir(&worktrees).unwrap();
    git(&source, &["worktree", "add", "-b", "linked", linked.to_str().unwrap(), "HEAD"]);
    checkout_hook(
        &source,
        "#!/bin/sh\nif (printf escaped > ../../escaped) 2>/dev/null; then exit 1; fi\nprintf scoped > .hook-ran\n",
    );

    let node = node(None);
    let thread = accept(&node, &worktrees, SESSION, &worktree(&linked, None)).await.unwrap();
    let destination = worktrees.join(&thread);
    let start = settled(&node, &thread).await;
    let StartState::Ready { directory } = start.state else {
        panic!("{start:?}");
    };
    assert_eq!(directory.project.as_deref(), linked.to_str());
    assert_eq!(std::fs::read_to_string(destination.join(".hook-ran")).unwrap(), "scoped");
    assert!(!root.join("escaped").exists());
    assert_eq!(git(&destination, &["branch", "--show-current"]), format!("octoii/{thread}"));
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn different_sessions_share_a_key_but_keep_separate_worktrees() {
    let folder = tempfile::tempdir().unwrap();
    let (root, worktrees) = checkout_paths(folder.path());
    std::fs::create_dir(&worktrees).unwrap();
    std::fs::write(root.join("tracked"), "uncommitted source").unwrap();
    std::fs::write(root.join("source-only"), "not committed").unwrap();
    let before = git(&root, &["status", "--porcelain"]);
    let node = node(None);
    let mut client = client(&node, vec![Scope::ThreadsOperate, Scope::ThreadsRead]).await;
    let request = worktree(&root, None);
    let first = accept(&node, &worktrees, SESSION, &request).await.unwrap();
    let second = accept(&node, &worktrees, "session-2", &request).await.unwrap();
    assert_ne!(first, second);
    let tasks = [(SESSION, first), ("session-2", second)];
    for (caller, thread) in &tasks {
        let destination = worktrees.join(thread);
        let start = settled(&node, thread).await;
        assert!(matches!(start.state, StartState::Ready { .. }), "{start:?}");
        assert_eq!(std::fs::read_to_string(destination.join("tracked")).unwrap(), "original");
        assert!(!destination.join("source-only").exists());
        assert_eq!(git(&destination, &["branch", "--show-current"]), format!("octoii/{thread}"));
        std::fs::write(destination.join("tracked"), caller).unwrap();
    }
    assert_eq!(git(&root, &["status", "--porcelain"]), before);
    assert_eq!(git(&root, &["branch", "--show-current"]), "main");
    for (caller, thread) in &tasks {
        call(&mut client, "threads.delete", json!({"thread":thread})).await;
        assert_eq!(std::fs::read_to_string(worktrees.join(thread).join("tracked")).unwrap(), *caller);
        assert!(
            accept(&node, &worktrees, caller, &request).await.is_err(),
            "a deleted task must not be recreated"
        );
    }
    assert_eq!(call(&mut client, "threads.list", json!({})).await["threads"], json!([]));
    assert_eq!(git(&root, &["worktree", "list", "--porcelain"]).matches("worktree ").count(), 3);
    node.close().await;
}
