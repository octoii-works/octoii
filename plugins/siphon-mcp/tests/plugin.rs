//! The MCP servers as Siphon's plugin, through a client's connection: the person's own servers listed as they stand,
//! a project's configuration shown with what the person decided on it, and trusted — its servers then among those a
//! thread there would use.

use beak::Beak;
use serde_json::{Value, json};
use siphon::Siphon;
use siphon::testing::{self, Client};
use siphon_mcp::McpPlugin;
use siphon_protocol::Scope;

async fn asked(client: &mut Client, method: &str, params: Value) -> Value {
    let id = client.call(method, params).await;
    let answer = client.answer(&id).await;
    assert!(answer["error"].is_null(), "{method}: {answer}");
    answer["result"].clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn servers_are_listed_as_they_stand_and_a_project_s_trusted_by_the_person() {
    let config = tempfile::tempdir().expect("a folder");
    let own = json!({ "mcpServers": { "mine": { "command": "my-server", "args": ["--quiet"] } } });
    std::fs::write(config.path().join("mcp.json"), own.to_string()).expect("written");
    let project = tempfile::tempdir().expect("a folder");
    let root = project.path().canonicalize().expect("its real path");
    std::fs::create_dir_all(root.join(".git")).expect("a repository");
    let named = json!({ "mcpServers": { "theirs": { "command": "their-server" } } });
    std::fs::write(root.join(".mcp.json"), named.to_string()).expect("written");
    let beak = Beak::new().expect("Beak");
    let plugin = McpPlugin {
        config: Some(config.path().to_owned()),
    };
    let siphon = Siphon::builder(&beak, testing::setup(siphon_machine::Host::default()))
        .expect("built")
        .plugin(plugin)
        .start()
        .expect("started");
    let mut client = Client::connect(&siphon, vec![Scope::ThreadsRead, Scope::ThreadsOperate], 64);
    client.message().await;

    let listed = asked(&mut client, "servers.list", json!({})).await;
    assert_eq!(
        listed["servers"],
        json!([{ "name": "mine", "source": config.path().join("mcp.json"), "runs": "my-server --quiet", "connected": false, "tools": [] }])
    );
    assert_eq!(listed["person"], json!(config.path().join("mcp.json")));

    let workspace = root.to_string_lossy().into_owned();
    let listed = asked(&mut client, "servers.list", json!({ "workspace": workspace })).await;
    let named = &listed["project"]["named"];
    assert_eq!(
        (named["servers"][0]["name"].as_str(), listed["project"].get("trusted")),
        (Some("theirs"), None),
        "not yet decided"
    );
    assert_eq!(
        listed["servers"].as_array().map(Vec::len),
        Some(1),
        "a project's servers not run till trusted"
    );

    let decided = json!({ "folder": named["folder"], "digest": named["digest"], "trusted": true });
    asked(&mut client, "trust.decide", decided).await;
    let listed = asked(&mut client, "servers.list", json!({ "workspace": workspace })).await;
    assert_eq!(listed["project"]["trusted"], true);
    let running: Vec<&str> = listed["servers"]
        .as_array()
        .expect("servers")
        .iter()
        .filter_map(|server| server["name"].as_str())
        .collect();
    assert_eq!(running, ["mine", "theirs"], "the project's trusted, beside the person's own");
    siphon.runtime.close().await;
}
