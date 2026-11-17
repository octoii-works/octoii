//! Git preparation uses Siphon's public startup gate and never adopts an uncertain existing worktree.
use serde_json::{Value, json};
use siphon::{
    Siphon,
    protocol::Scope,
    testing::{self, Beak, Client},
};
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

fn node(beak: &Beak) -> Siphon {
    Siphon::builder(beak, testing::setup(siphon::machine::Host::default()))
        .unwrap()
        .extension(siphon_base::BaseExtension::default())
        .extension(siphon_git::GitExtension)
        .start()
        .unwrap()
}

async fn client(node: &Siphon, scopes: Vec<Scope>) -> Client {
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

fn accept(root: &Path, destination: &Path) -> Value {
    json!({"action":"accept", "key":"task", "config":{"title":"task"},
        "content":[{"type":"input_text", "text":"keep my input"}],
        "worktree":{"path":root, "destination":destination, "branch":"task", "base":git(root, &["rev-parse", "HEAD"])}})
}

#[tokio::test(flavor = "multi_thread")]
async fn accepted_input_waits_and_duplicate_preparation_releases_the_same_thread() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    repository(&root);
    let destination = root.join("task");
    let beak = Beak::new().unwrap();
    let node = node(&beak);
    let mut client = client(&node, vec![Scope::GitOperate, Scope::ThreadsOperate, Scope::ThreadsRead]).await;
    let asked = accept(&root, &destination);
    let accepted = call(&mut client, "git.start", asked.clone()).await;
    assert_eq!(accepted["start"]["state"]["state"], "pending");
    assert!(!destination.exists());
    assert_eq!(call(&mut client, "git.start", asked).await, accepted);
    let listed = call(&mut client, "threads.list", json!({})).await;
    assert_eq!(listed["threads"].as_array().unwrap().len(), 1);
    assert_eq!(listed["threads"][0]["status"]["inbox"].as_array().unwrap().len(), 1);
    std::fs::write(root.join("tracked"), "later").unwrap();
    git(&root, &["commit", "-am", "later"]);
    let prepare = json!({"action":"prepare", "thread":accepted["thread"], "token":accepted["start"]["token"]});
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
    node.runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn existing_worktree_is_inspected_but_never_adopted_or_cleaned_up() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    repository(&root);
    let destination = root.join("task");
    git(&root, &["worktree", "add", "-b", "task", destination.to_str().unwrap(), "HEAD"]);
    std::fs::write(destination.join("keep"), "other work").unwrap();
    let beak = Beak::new().unwrap();
    let node = node(&beak);
    let mut client = client(&node, vec![Scope::GitOperate, Scope::ThreadsOperate, Scope::ThreadsRead]).await;
    let accepted = call(&mut client, "git.start", accept(&root, &destination)).await;
    let thread = &accepted["thread"];
    let inspection = call(&mut client, "git.start", json!({"action":"inspect", "thread":thread})).await;
    assert_eq!(inspection["worktrees"].as_array().unwrap().len(), 2);
    assert_eq!(inspection["start"]["state"]["state"], "pending");
    let failed = call(
        &mut client,
        "git.start",
        json!({"action":"prepare", "thread":thread, "token":accepted["start"]["token"]}),
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
    call(&mut client, "threads.stop", json!({"thread":thread})).await;
    let stopped = call(
        &mut client,
        "git.start",
        json!({"action":"prepare", "thread":thread, "token":retried["start"]["token"]}),
    )
    .await;
    assert_eq!(stopped["start"]["state"]["state"], "cancelled");
    node.runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn starting_requires_both_git_and_thread_operation_permissions() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    repository(&root);
    let beak = Beak::new().unwrap();
    let node = node(&beak);
    for scopes in [vec![Scope::ThreadsOperate], vec![Scope::GitOperate]] {
        let mut client = client(&node, scopes).await;
        let id = client.call("git.start", accept(&root, &root.join("task"))).await;
        let denied = client.answer(&id).await;
        assert_eq!(denied["error"]["code"], "forbidden", "{denied}");
    }
    let mut client = client(&node, vec![Scope::ThreadsRead]).await;
    assert_eq!(call(&mut client, "threads.list", json!({})).await["threads"], json!([]));
    assert!(!root.join("task").exists());
    node.runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reopening_keeps_the_request_and_retry_requires_the_saved_token() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    repository(&root);
    git(&root, &["branch", "task"]);
    let destination = root.join("task");
    let storage = tempfile::tempdir().unwrap();
    let beak = Beak::new().unwrap();
    let open = || {
        let mut setup = testing::setup(siphon::machine::Host::default());
        setup.options.directory = Some(storage.path().to_owned());
        Siphon::builder(&beak, setup)
            .unwrap()
            .extension(siphon_base::BaseExtension::default())
            .extension(siphon_git::GitExtension)
            .start()
            .unwrap()
    };
    let first = open();
    let scopes = vec![Scope::GitOperate, Scope::ThreadsOperate, Scope::ThreadsRead];
    // Own the connection task so reopening waits for every old volume handle to be dropped.
    let (calls, incoming) = tokio::sync::mpsc::channel(4);
    let (outgoing, mut replies) = tokio::sync::mpsc::channel(4);
    let connection = tokio::spawn(siphon::connection::serve(
        first.clone(),
        siphon::connection::Access {
            session: "test".into(),
            scopes: scopes.clone(),
        },
        siphon::protocol::Machine::of("here".into(), &first.machine),
        incoming,
        outgoing,
    ));
    replies.recv().await.unwrap();
    let request = accept(&root, &destination);
    calls
        .send(json!({"id":"accept", "method":"git.start", "params":request}).to_string())
        .await
        .unwrap();
    let siphon::connection::Outgoing::Text(reply) = replies.recv().await.unwrap() else {
        panic!("text response")
    };
    let reply: Value = serde_json::from_str(&reply).unwrap();
    assert!(reply["error"].is_null(), "{reply}");
    let accepted = reply["result"].clone();
    drop(calls);
    connection.await.unwrap();
    first.runtime.close().await;
    drop(first);
    let second = open();
    let mut after = client(&second, scopes).await;
    assert_eq!(call(&mut after, "git.start", request).await, accepted);
    assert!(!destination.exists());
    let thread = &accepted["thread"];
    let failed = call(
        &mut after,
        "git.start",
        json!({"action":"prepare", "thread":thread, "token":accepted["start"]["token"]}),
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
    second.runtime.close().await;
}
