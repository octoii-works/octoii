//! A repository on the machine as the person reads it through a connection, git itself telling: where it begins, its
//! branch and its base; the changes not yet committed — a file changed with its lines counted, one git does not keep —
//! a file's patch and its two versions; a branch's changes and commits since it parted from its base.

use std::path::Path;
use std::process::Command;

use beak::Beak;
use serde_json::{Value, json};
use siphon::Siphon;
use siphon::testing::{self, Client};
use siphon_protocol::Scope;

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
    let siphon = Siphon::builder(&beak, testing::setup(siphon_machine::Host::default()))
        .expect("built")
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
