//! The machine's files as the person browses them through a connection: a folder's entries with what git leaves out,
//! a file's lines a page at a time, an image told by its type, a folder read refused, the files that match a query,
//! the best first — each to a session that may read files alone.

use octoii::Octoii;
use octoii::protocol::Scope;
use octoii::testing::{self, Client};
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn the_person_browses_the_machine_s_files_as_git_sees_them() {
    let folder = tempfile::tempdir().expect("a folder");
    let root = folder.path().canonicalize().expect("its real path");
    std::fs::create_dir_all(root.join(".git")).expect("a repository");
    std::fs::create_dir_all(root.join("src")).expect("a folder");
    std::fs::create_dir_all(root.join("target")).expect("a folder");
    std::fs::write(root.join(".gitignore"), "target\n").expect("written");
    std::fs::write(root.join("notes.txt"), "one\ntwo\nthree\n").expect("written");
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("written");
    std::fs::write(root.join("src/maintenance.md"), "later\n").expect("written");
    std::fs::write(root.join("target/built"), "x").expect("written");
    std::fs::write(root.join("dot.png"), b"\x89PNG\r\n\x1a\n rest").expect("written");
    let siphon = Octoii::builder(testing::setup(octoii::machine::Host::default()))
        .expect("built")
        .extension(TestBase)
        .start()
        .expect("started");
    let root = root.to_string_lossy().into_owned();
    let mut client = Client::connect(&siphon, vec![Scope::FilesRead], 64);
    client.message().await;

    let listed = client.command("files.list", json!({ "path": root, "limit": 10 })).await["result"].clone();
    let entries = listed["entries"].as_array().expect("entries");
    let target = entries.iter().find(|entry| entry["name"] == "target").expect("listed");
    assert_eq!(
        (target["kind"].as_str(), target["ignored"].as_bool()),
        (Some("directory"), Some(true))
    );
    let src = entries.iter().find(|entry| entry["name"] == "src").expect("listed");
    assert_eq!(src["ignored"], false);
    let page = client
        .command("files.list", json!({ "path": root, "limit": 1, "after": "notes.txt" }))
        .await["result"]
        .clone();
    assert_eq!(
        (page["entries"][0]["name"].as_str(), page["more"].as_bool()),
        (Some("src"), Some(true))
    );

    let text = client
        .command(
            "files.read",
            json!({ "path": format!("{root}/notes.txt"), "offset": 2, "limit": 1 }),
        )
        .await["result"]
        .clone();
    assert_eq!(
        text,
        json!({ "type": "text", "path": format!("{root}/notes.txt"), "size": 14, "from": 2, "lines": ["two"], "more": true })
    );
    assert_eq!(
        client.command("files.read", json!({ "path": format!("{root}/dot.png") })).await["result"]["media_type"],
        "image/png"
    );
    assert_eq!(
        client.command("files.read", json!({ "path": root })).await["error"]["code"],
        "a_folder"
    );

    let found = client
        .command("files.search", json!({ "path": root, "query": "main", "limit": 1 }))
        .await["result"]
        .clone();
    assert_eq!(
        (found["found"].clone(), found["truncated"].as_bool()),
        (json!([{ "path": "src/main.rs", "kind": "file" }]), Some(true)),
        "the best first"
    );
    let found = client
        .command("files.search", json!({ "path": root, "query": "src", "limit": 5 }))
        .await["result"]["found"]
        .clone();
    assert!(
        found
            .as_array()
            .is_some_and(|found| found.contains(&json!({ "path": "src", "kind": "directory" }))),
        "a folder the files lie in, found as a folder: {found}"
    );

    let mut reader = Client::connect(&siphon, vec![Scope::ThreadsRead], 64);
    reader.message().await;
    assert_eq!(
        reader.command("files.list", json!({ "path": root, "limit": 10 })).await["error"]["code"],
        "forbidden"
    );
    siphon.close().await;
}

struct TestBase;
impl octoii::Extension for TestBase {
    fn name(&self) -> &'static str {
        "base"
    }
    fn title(&self) -> &'static str {
        "Test base"
    }
    fn description(&self) -> &'static str {
        "Connection test fixture"
    }
    fn build(self: Box<Self>, registrar: &mut octoii::Registrar<'_>) {
        registrar.prompts(octoii::testing::PROMPTS);
    }
}
