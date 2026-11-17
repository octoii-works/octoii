//! A repository on the machine as the person reads it through a connection, git itself telling: where it begins, its
//! branch and its base; the changes not yet committed — a file changed with its lines counted, one git does not keep —
//! a file's patch and its two versions; a branch's changes and commits since it parted from its base.

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use siphon::Siphon;
use siphon::protocol::Scope;
use siphon::testing::Beak;
use siphon::testing::{self, Client};

fn git(root: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(arguments)
        .current_dir(root)
        .output()
        .expect("git");
    assert!(
        status.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}

async fn asked(client: &mut Client, method: &str, params: Value) -> Value {
    let id = client.call(method, params).await;
    let answer = client.answer(&id).await;
    assert!(answer["error"].is_null(), "{method}: {answer}");
    answer["result"].clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn git_tells_a_repository_s_changes_patches_versions_and_commits() {
    let folder = tempfile::tempdir().expect("a folder");
    let root = folder.path().canonicalize().expect("its real path");
    git(&root, &["init"]);
    std::fs::write(root.join("a.txt"), "one\ntwo\n").expect("written");
    git(&root, &["add", "a.txt"]);
    git(&root, &["commit", "-m", "first"]);
    git(&root, &["checkout", "-b", "work"]);
    std::fs::write(root.join("b.txt"), "b\n").expect("written");
    git(&root, &["add", "b.txt"]);
    git(&root, &["commit", "-m", "add b"]);
    std::fs::write(root.join("a.txt"), "one\nTWO\nthree\n").expect("written");
    std::fs::write(root.join("new.txt"), "fresh\n").expect("written");
    let beak = Beak::new().expect("Beak");
    let siphon = Siphon::builder(&beak, testing::setup(siphon::machine::Host::default()))
        .map(|building| building.extension(siphon_git::GitExtension))
        .expect("built")
        .extension(siphon_base::BaseExtension::default())
        .start()
        .expect("started");
    let mut client = Client::connect(&siphon, vec![Scope::FilesRead], 64);
    client.message().await;
    let repo = root.to_string_lossy().into_owned();

    let repository = asked(&mut client, "git.repository", json!({ "path": repo })).await["repository"].clone();
    assert_eq!(
        (
            repository["root"].as_str(),
            repository["branch"].as_str(),
            repository["base"].as_str()
        ),
        (Some(repo.as_str()), Some("work"), Some("main"))
    );

    let changes = asked(
        &mut client,
        "git.changes",
        json!({ "path": repo, "scope": { "type": "uncommitted" } }),
    )
    .await;
    let files = changes["files"].as_array().expect("files");
    let a = files.iter().find(|file| file["path"] == "a.txt").expect("a.txt changed");
    assert_eq!(
        (a["status"].as_str(), a["additions"].as_u64(), a["deletions"].as_u64()),
        (Some("modified"), Some(2), Some(1))
    );
    let fresh = files.iter().find(|file| file["path"] == "new.txt").expect("new.txt untracked");
    assert_eq!(fresh["status"], "untracked");

    let patch = asked(
        &mut client,
        "git.patch",
        json!({ "path": repo, "comparison": changes["comparison"], "file": a }),
    )
    .await;
    let patch = patch["patch"].as_str().expect("a patch");
    assert!(
        patch.contains("-two") && patch.contains("+TWO") && patch.contains("+three"),
        "{patch}"
    );
    let untracked = asked(
        &mut client,
        "git.patch",
        json!({ "path": repo, "comparison": changes["comparison"], "file": fresh }),
    )
    .await;
    assert!(untracked["patch"].as_str().is_some_and(|patch| patch.contains("+fresh")));
    let versions = asked(
        &mut client,
        "git.versions",
        json!({ "path": repo, "comparison": changes["comparison"], "file": a }),
    )
    .await;
    assert_eq!(versions, json!({ "old": "one\ntwo\n", "new": "one\nTWO\nthree\n" }));

    let branch = asked(&mut client, "git.changes", json!({ "path": repo, "scope": { "type": "branch" } })).await;
    let names: Vec<&str> = branch["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect();
    assert!(
        names.contains(&"b.txt") && names.contains(&"a.txt"),
        "the branch's own and the working tree's: {names:?}"
    );
    assert_eq!(branch["base"], "main");
    let commits = asked(&mut client, "git.commits", json!({ "path": repo })).await;
    assert_eq!(
        (
            commits["commits"][0]["subject"].as_str(),
            commits["commits"].as_array().map(Vec::len)
        ),
        (Some("add b"), Some(1))
    );

    let elsewhere = tempfile::tempdir().expect("a folder");
    let id = client
        .call(
            "git.changes",
            json!({ "path": elsewhere.path(), "scope": { "type": "uncommitted" } }),
        )
        .await;
    assert_eq!(client.answer(&id).await["error"]["code"], "no_repository");
    siphon.runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn worktree_rpc_requires_git_permission_and_checks_out_the_requested_base() {
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().canonicalize().unwrap();
    let source = root.join("source");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init"]);
    std::fs::write(source.join("value.txt"), "base\n").unwrap();
    git(&source, &["add", "."]);
    git(&source, &["commit", "-m", "base"]);
    let base = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&source)
        .output()
        .unwrap();
    assert!(base.status.success());
    let base = String::from_utf8(base.stdout).unwrap().trim().to_owned();
    std::fs::write(source.join("value.txt"), "later commit\n").unwrap();
    git(&source, &["commit", "-am", "later"]);
    std::fs::write(source.join("value.txt"), "uncommitted\n").unwrap();
    std::fs::write(source.join("untracked.txt"), "keep here\n").unwrap();

    let beak = Beak::new().unwrap();
    let siphon = Siphon::builder(&beak, testing::setup(siphon::machine::Host::default()))
        .unwrap()
        .extension(siphon_base::BaseExtension::default())
        .extension(siphon_git::GitExtension)
        .start()
        .unwrap();
    let mut reader = Client::connect(&siphon, vec![Scope::FilesRead], 64);
    reader.message().await;
    let listed = asked(&mut reader, "git.worktrees", json!({"path": source})).await;
    assert_eq!(listed["worktrees"].as_array().unwrap().len(), 1);
    let destination = root.join("task");
    let params = json!({"path": source, "destination": destination, "branch": "task", "base": base});
    let id = reader.call("git.create-worktree", params.clone()).await;
    assert_eq!(reader.answer(&id).await["error"]["code"], "forbidden");
    assert!(!destination.exists());
    let branch = Command::new("git")
        .args(["show-ref", "--verify", "refs/heads/task"])
        .current_dir(&source)
        .output()
        .unwrap();
    assert!(!branch.status.success());

    let mut writer = Client::connect(&siphon, vec![Scope::GitOperate], 64);
    writer.message().await;
    let made = asked(&mut writer, "git.create-worktree", params.clone()).await;
    assert_eq!(made, json!({"path": destination, "branch": "task", "base_commit": base}));
    assert_eq!(std::fs::read_to_string(destination.join("value.txt")).unwrap(), "base\n");
    assert!(!destination.join("untracked.txt").exists());
    assert_eq!(std::fs::read_to_string(source.join("value.txt")).unwrap(), "uncommitted\n");
    let listed = asked(&mut reader, "git.worktrees", json!({"path": source})).await;
    assert_eq!(listed["worktrees"].as_array().unwrap().len(), 2);
    assert!(
        listed["worktrees"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["path"] == destination.to_string_lossy().as_ref())
    );
    std::fs::write(destination.join("keep.txt"), "keep me\n").unwrap();
    let id = writer.call("git.create-worktree", params).await;
    assert!(!writer.answer(&id).await["error"].is_null());
    assert_eq!(std::fs::read_to_string(destination.join("keep.txt")).unwrap(), "keep me\n");
    siphon.runtime.close().await;
}
