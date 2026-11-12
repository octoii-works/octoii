//! The machine's files as the person browses them through a connection: a folder's entries with what git leaves out,
//! a file's lines a page at a time, an image told by its type, a folder read refused, the files that match a query,
//! the best first — each to a session that may read files alone.

use beak::Beak;
use serde_json::json;
use siphon::Siphon;
use siphon::protocol::Scope;
use siphon::testing::{self, Client};

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
    let beak = Beak::new().expect("Beak");
    let siphon = Siphon::builder(&beak, testing::setup(siphon::machine::Host::default()))
        .expect("built")
        .start()
        .expect("started");
    let root = root.to_string_lossy().into_owned();
    let mut client = Client::connect(&siphon, vec![Scope::FilesRead], 64);
    client.message().await;

    let id = client.call("files.list", json!({ "path": root, "limit": 10 })).await;
    let listed = client.answer(&id).await["result"].clone();
    let entries = listed["entries"].as_array().expect("entries");
    let target = entries.iter().find(|entry| entry["name"] == "target").expect("listed");
    assert_eq!(
        (target["kind"].as_str(), target["ignored"].as_bool()),
        (Some("directory"), Some(true))
    );
    let src = entries.iter().find(|entry| entry["name"] == "src").expect("listed");
    assert_eq!(src["ignored"], false);
    let id = client
        .call("files.list", json!({ "path": root, "limit": 1, "after": "notes.txt" }))
        .await;
    let page = client.answer(&id).await["result"].clone();
    assert_eq!(
        (page["entries"][0]["name"].as_str(), page["more"].as_bool()),
        (Some("src"), Some(true))
    );

    let id = client
        .call(
            "files.read",
            json!({ "path": format!("{root}/notes.txt"), "offset": 2, "limit": 1 }),
        )
        .await;
    let text = client.answer(&id).await["result"].clone();
    assert_eq!(
        text,
        json!({ "type": "text", "path": format!("{root}/notes.txt"), "size": 14, "from": 2, "lines": ["two"], "more": true })
    );
    let id = client.call("files.read", json!({ "path": format!("{root}/dot.png") })).await;
    assert_eq!(client.answer(&id).await["result"]["media_type"], "image/png");
    let id = client.call("files.read", json!({ "path": root })).await;
    assert_eq!(client.answer(&id).await["error"]["code"], "a_folder");

    let id = client
        .call("files.search", json!({ "path": root, "query": "main", "limit": 1 }))
        .await;
    let found = client.answer(&id).await["result"].clone();
    assert_eq!(
        (found["found"].clone(), found["truncated"].as_bool()),
        (json!([{ "path": "src/main.rs", "kind": "file" }]), Some(true)),
        "the best first"
    );
    let id = client
        .call("files.search", json!({ "path": root, "query": "src", "limit": 5 }))
        .await;
    let found = client.answer(&id).await["result"]["found"].clone();
    assert!(
        found
            .as_array()
            .is_some_and(|found| found.contains(&json!({ "path": "src", "kind": "directory" }))),
        "a folder the files lie in, found as a folder: {found}"
    );

    let mut reader = Client::connect(&siphon, vec![Scope::ThreadsRead], 64);
    reader.message().await;
    let id = reader.call("files.list", json!({ "path": root, "limit": 10 })).await;
    assert_eq!(reader.answer(&id).await["error"]["code"], "forbidden");
    siphon.runtime.close().await;
}
