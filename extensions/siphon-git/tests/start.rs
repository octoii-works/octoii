//! Git preparation uses Octoii's public startup gate and never adopts an uncertain existing worktree.
use octoii::{
    Octoii,
    protocol::Scope,
    testing::{self, Client},
    threads::{Config, StartState},
};
use octoii_ai::vocabulary::InputContent;
use serde_json::{Value, json};
use siphon_git::protocol::CreateWorktree;
use std::path::Path;

fn git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn repository(root: &Path) {
    git(root, &["init"]);
    std::fs::write(root.join("tracked"), "original").unwrap();
    git(root, &["add", "tracked"]);
    git(root, &["commit", "-m", "base"]);
}

fn node() -> Octoii {
    Octoii::builder(testing::setup(octoii::machine::Host::default()))
        .unwrap()
        .extension(siphon_base::BaseExtension::default())
        .extension(siphon_git::GitExtension {
            worktrees: std::env::temp_dir().join("octoii-test-worktrees"),
        })
        .start()
        .unwrap()
}

async fn client(node: &Octoii, scopes: Vec<Scope>) -> Client {
    let mut client = Client::connect(node, scopes, 64);
    client.message().await;
    client
}

async fn call(client: &mut Client, method: &str, params: Value) -> Value {
    let id = client.call(method, params).await;
    let reply = client.answer(&id).await;
    assert!(reply["error"].is_null(), "{reply}");
    reply["result"].clone()
}

fn worktree(root: &Path, destination: &Path, branch: &str) -> CreateWorktree {
    CreateWorktree {
        path: root.to_str().unwrap().to_owned(),
        destination: destination.to_str().unwrap().to_owned(),
        branch: branch.to_owned(),
        base: git(root, &["rev-parse", "HEAD"]),
    }
}

async fn accept(node: &Octoii, worktree: &CreateWorktree) -> anyhow::Result<String> {
    siphon_git::start::accept(
        &node.volume(),
        "session-1",
        &worktree.branch,
        Config {
            title: Some("task".into()),
            ..Config::default()
        },
        vec![InputContent::text("keep my input").into()],
        worktree.clone(),
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn accepted_input_waits_and_duplicate_preparation_releases_the_same_thread() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    repository(&root);
    let destination = root.join("task");
    let node = node();
    let mut client = client(&node, vec![Scope::GitOperate, Scope::ThreadsOperate, Scope::ThreadsRead]).await;
    let asked = worktree(&root, &destination, "task");
    let thread = accept(&node, &asked).await.unwrap();
    let start = node.volume().thread_start(&thread).unwrap().unwrap();
    assert_eq!(start.state, StartState::Pending);
    assert!(!destination.exists());
    assert_eq!(accept(&node, &asked).await.unwrap(), thread);
    let listed = call(&mut client, "threads.list", json!({})).await;
    assert_eq!(listed["threads"].as_array().unwrap().len(), 1);
    assert_eq!(listed["threads"][0]["status"]["inbox"].as_array().unwrap().len(), 1);
    std::fs::write(root.join("tracked"), "later").unwrap();
    git(&root, &["commit", "-am", "later"]);
    let prepare = json!({"action":"prepare", "thread":thread, "token":start.token});
    let a = client.call("git.start", prepare.clone()).await;
    let b = client.call("git.start", prepare).await;
    let first = client.message().await;
    let second = client.message().await;
    assert!(first["id"] == a || first["id"] == b);
    assert!(second["id"] == a || second["id"] == b);
    assert_ne!(first["id"], second["id"]);
    assert!(first["error"].is_null(), "{first}");
    assert_eq!(first["result"], second["result"]);
    assert_eq!(first["result"]["start"]["state"]["state"], "ready");
    assert_eq!(std::fs::read_to_string(destination.join("tracked")).unwrap(), "original");
    let listed = call(&mut client, "threads.list", json!({})).await;
    assert_eq!(listed["threads"][0]["config"]["workspace"], destination.to_str().unwrap());
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn existing_worktree_is_inspected_but_never_adopted_or_cleaned_up() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    repository(&root);
    let destination = root.join("task");
    git(&root, &["worktree", "add", "-b", "task", destination.to_str().unwrap(), "HEAD"]);
    std::fs::write(destination.join("keep"), "other work").unwrap();
    let node = node();
    let mut client = client(&node, vec![Scope::GitOperate, Scope::ThreadsOperate, Scope::ThreadsRead]).await;
    let thread = accept(&node, &worktree(&root, &destination, "task")).await.unwrap();
    let inspection = call(&mut client, "git.start", json!({"action":"inspect", "thread":thread})).await;
    assert_eq!(inspection["worktrees"].as_array().unwrap().len(), 2);
    assert_eq!(inspection["start"]["state"]["state"], "pending");
    let failed = call(
        &mut client,
        "git.start",
        json!({"action":"prepare", "thread":thread, "token":inspection["start"]["token"]}),
    )
    .await;
    assert_eq!(failed["start"]["state"]["state"], "failed");
    let retried = call(
        &mut client,
        "git.start",
        json!({"action":"retry", "thread":thread, "token":failed["start"]["token"]}),
    )
    .await;
    assert_eq!(retried["start"]["state"]["state"], "failed");
    assert_ne!(retried["start"]["token"], failed["start"]["token"]);
    assert_eq!(std::fs::read_to_string(destination.join("keep")).unwrap(), "other work");
    let listed = call(&mut client, "threads.list", json!({})).await;
    assert_eq!(listed["threads"][0]["status"]["inbox"].as_array().unwrap().len(), 1);
    call(
        &mut client,
        "threads.stop",
        json!({"thread":thread, "target":{"kind":"preparation", "token":retried["start"]["token"]}}),
    )
    .await;
    let stopped = call(
        &mut client,
        "git.start",
        json!({"action":"prepare", "thread":thread, "token":retried["start"]["token"]}),
    )
    .await;
    assert_eq!(stopped["start"]["state"]["state"], "cancelled");
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stop_for_a_previous_preparation_leaves_the_retry_and_its_input() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    repository(&root);
    let destination = root.join("task");
    let node = node();
    let thread = accept(&node, &worktree(&root, &destination, "task")).await.unwrap();
    let volume = node.volume();
    let first = volume.thread_start(&thread).unwrap().unwrap();
    let input = node.threads.listed(&thread).unwrap().status.inbox;
    assert_eq!(input.len(), 1);
    volume
        .fail_thread_start(&thread, &first.token, "retry fixture".into())
        .await
        .unwrap();
    let retry = volume.retry_thread_start(&thread, &first.token).await.unwrap();
    assert_ne!(retry.token, first.token);
    assert_eq!(retry.state, StartState::Pending);

    let mut client = client(&node, vec![Scope::ThreadsOperate]).await;
    let stop = client
        .call(
            "threads.stop",
            json!({"thread":thread, "target":{"kind":"preparation", "token":first.token}}),
        )
        .await;
    let refused = client.answer(&stop).await;
    assert_eq!(refused["error"]["code"], "stale_stop", "{refused}");
    let status = node.threads.listed(&thread).unwrap().status;
    assert_eq!(status.start, Some(retry));
    assert_eq!(status.inbox, input);
    assert!(!destination.exists());
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn preparing_requires_both_git_and_thread_operation_permissions() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    repository(&root);
    let node = node();
    let destination = root.join("task");
    let thread = accept(&node, &worktree(&root, &destination, "task")).await.unwrap();
    let start = node.volume().thread_start(&thread).unwrap().unwrap();
    let prepare = json!({"action":"prepare", "thread":thread, "token":start.token});
    for scopes in [vec![Scope::ThreadsOperate], vec![Scope::GitOperate]] {
        let mut client = client(&node, scopes).await;
        let id = client.call("git.start", prepare.clone()).await;
        let denied = client.answer(&id).await;
        assert_eq!(denied["error"]["code"], "forbidden", "{denied}");
    }
    assert_eq!(node.volume().thread_start(&thread).unwrap(), Some(start));
    assert!(!destination.exists());
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reopening_keeps_the_request_and_retry_requires_the_saved_token() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    repository(&root);
    git(&root, &["branch", "task"]);
    let destination = root.join("task");
    let storage = tempfile::tempdir().unwrap();
    let open = || {
        let mut setup = testing::setup(octoii::machine::Host::default());
        setup.directory = Some(storage.path().to_owned());
        Octoii::builder(setup)
            .unwrap()
            .extension(siphon_base::BaseExtension::default())
            .extension(siphon_git::GitExtension {
                worktrees: std::env::temp_dir().join("octoii-test-worktrees"),
            })
            .start()
            .unwrap()
    };
    let scopes = vec![Scope::GitOperate, Scope::ThreadsOperate, Scope::ThreadsRead];
    let request = worktree(&root, &destination, "task");
    // Reopening models a new host: tear down the old executor as well as its node.
    // Connection and runtime completion alone do not join every executor-owned task.
    let (thread, start) = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                tokio::runtime::Runtime::new().unwrap().block_on(async {
                    let first = open();
                    let thread = accept(&first, &request).await.unwrap();
                    let start = first.volume().thread_start(&thread).unwrap().unwrap();
                    first.close().await;
                    (thread, start)
                })
            })
            .join()
            .unwrap()
    });
    let second = open();
    let mut after = client(&second, scopes).await;
    assert_eq!(accept(&second, &request).await.unwrap(), thread);
    assert_eq!(second.volume().thread_start(&thread).unwrap(), Some(start.clone()));
    assert!(!destination.exists());
    let failed = call(
        &mut after,
        "git.start",
        json!({"action":"prepare", "thread":thread, "token":start.token}),
    )
    .await;
    assert_eq!(failed["start"]["state"]["state"], "failed");
    assert!(!destination.exists());
    // Explicit fixture cleanup represents the person resolving the conflict; the service never does this.
    git(&root, &["branch", "-d", "task"]);
    let ready = call(
        &mut after,
        "git.start",
        json!({"action":"retry", "thread":thread, "token":failed["start"]["token"]}),
    )
    .await;
    assert_eq!(ready["start"]["state"]["state"], "ready");
    assert_ne!(ready["start"]["token"], failed["start"]["token"]);
    let stale = after
        .call(
            "git.start",
            json!({"action":"prepare", "thread":thread, "token":failed["start"]["token"]}),
        )
        .await;
    assert!(!after.answer(&stale).await["error"].is_null());
    assert_eq!(std::fs::read_to_string(destination.join("tracked")).unwrap(), "original");
    second.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn independent_tasks_preserve_dirty_source_and_worktrees_after_thread_deletion() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    repository(&root);
    std::fs::write(root.join("tracked"), "uncommitted source").unwrap();
    std::fs::write(root.join("source-only"), "not committed").unwrap();
    let before = git(&root, &["status", "--porcelain"]);
    let node = node();
    let mut client = client(&node, vec![Scope::GitOperate, Scope::ThreadsOperate, Scope::ThreadsRead]).await;
    let mut tasks = Vec::new();
    for name in ["first", "second"] {
        // Sibling paths keep generated worktrees out of the source checkout's status.
        let destination_owner = tempfile::Builder::new().prefix(name).tempdir().unwrap();
        let destination = destination_owner.path().join("checkout");
        let request = worktree(&root, &destination, name);
        let thread = accept(&node, &request).await.unwrap();
        let start = node.volume().thread_start(&thread).unwrap().unwrap();
        let prepare = json!({"action":"prepare", "thread":thread, "token":start.token});
        let ready = call(&mut client, "git.start", prepare.clone()).await;
        assert_eq!(ready["start"]["state"]["state"], "ready");
        assert_eq!(std::fs::read_to_string(destination.join("tracked")).unwrap(), "original");
        assert!(!destination.join("source-only").exists());
        std::fs::write(destination.join("tracked"), name).unwrap();
        assert_eq!(call(&mut client, "git.start", prepare).await, ready);
        tasks.push((destination_owner, destination, thread, request));
    }
    assert_eq!(git(&root, &["status", "--porcelain"]), before);
    assert_eq!(git(&root, &["branch", "--show-current"]), "main");
    for (_, path, thread, request) in &tasks {
        let contents = std::fs::read_to_string(path.join("tracked")).unwrap();
        call(&mut client, "threads.delete", json!({"thread":thread})).await;
        assert_eq!(std::fs::read_to_string(path.join("tracked")).unwrap(), contents);
        assert!(accept(&node, request).await.is_err(), "a deleted task must not be recreated");
    }
    assert_eq!(call(&mut client, "threads.list", json!({})).await["threads"], json!([]));
    assert_eq!(git(&root, &["worktree", "list", "--porcelain"]).matches("worktree ").count(), 3);
    assert_eq!(std::fs::read_to_string(tasks[0].1.join("tracked")).unwrap(), "first");
    assert_eq!(std::fs::read_to_string(tasks[1].1.join("tracked")).unwrap(), "second");
    node.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_pending_task_rejects_late_preparation_and_acceptance_replay() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    repository(&root);
    let destination = root.join("task");
    let node = node();
    let mut client = client(&node, vec![Scope::GitOperate, Scope::ThreadsOperate, Scope::ThreadsRead]).await;
    let request = worktree(&root, &destination, "task");
    let thread = accept(&node, &request).await.unwrap();
    let start = node.volume().thread_start(&thread).unwrap().unwrap();
    call(&mut client, "threads.delete", json!({"thread":thread})).await;
    let late = client
        .call("git.start", json!({"action":"prepare", "thread":thread, "token":start.token}))
        .await;
    assert!(!client.answer(&late).await["error"].is_null());
    assert!(accept(&node, &request).await.is_err());
    assert!(!destination.exists());
    assert_eq!(git(&root, &["branch", "--list", "task"]), "");
    assert_eq!(call(&mut client, "threads.list", json!({})).await["threads"], json!([]));
    node.close().await;
}
