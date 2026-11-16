//! The MCP servers as Siphon's extension, through a client's connection: the person's own servers listed as they stand,
//! a project's configuration shown with what the person decided on it, and trusted — its servers then among those a
//! thread there would use.

use serde_json::{Value, json};
use siphon::Siphon;
use siphon::protocol::Scope;
use siphon::testing::Beak;
use siphon::testing::{self, Client};
use siphon_mcp::McpExtension;

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
    let extension = McpExtension {
        config: Some(config.path().to_owned()),
    };
    let siphon = Siphon::builder(&beak, testing::setup(siphon::machine::Host::default()))
        .expect("built")
        .extension(extension)
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

struct SpawnExplorer;
impl siphon::tools::Tool for SpawnExplorer {
    const NAME: &'static str = "spawn_explorer";
    const DESCRIPTION: &'static str = "Make an explorer through the thread primitive";
    type Args = Value;
    fn effect(&self, _: &Value, _: &siphon::machine::Base) -> siphon::tools::Effect {
        siphon::tools::Effect::Delegates
    }
    fn exec(
        &self,
        context: &siphon::tools::Context,
        _: Value,
    ) -> impl std::future::Future<Output = Result<siphon::tools::Told, siphon::tools::Refusal>> + Send {
        let mut plan = siphon::tools::Plan::default();
        let child = plan.spawn(
            &context.volume(),
            None,
            siphon::tools::Spawned::SubThread(siphon::threads::SubThreadKind::Explorer),
        );
        std::future::ready(child.map(|child| siphon::tools::Told {
            plan,
            structured_content: Some(json!(child)),
            ..siphon::tools::Told::default()
        }))
    }
}

struct ExplorerCatalog {
    servers: siphon_mcp::Servers,
    place: siphon::tools::Place,
}
impl siphon::tools::Tool for ExplorerCatalog {
    const NAME: &'static str = "explorer_catalog";
    const DESCRIPTION: &'static str = "Gather the real MCP teller for an explorer";
    type Args = String;
    fn effect(&self, _: &String, _: &siphon::machine::Base) -> siphon::tools::Effect {
        siphon::tools::Effect::Reads
    }
    async fn exec(&self, context: &siphon::tools::Context, child: String) -> Result<siphon::tools::Told, siphon::tools::Refusal> {
        use siphon::agent::Teller as _;
        let telling = self
            .servers
            .tell(&context.volume(), &child, Some(&self.place))
            .await
            .map_err(|error| siphon::tools::Refusal(error.to_string()))?;
        let row = siphon::threads::list(&context.volume())
            .unwrap()
            .into_iter()
            .find(|row| row.id == child)
            .unwrap();
        Ok(siphon::tools::Told {
            structured_content: Some(json!({ "sections": telling.sections, "tools": telling.tools, "asks": row.status.asks.len() })),
            ..siphon::tools::Told::default()
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_explorer_neither_connects_mcp_servers_nor_requests_project_trust() {
    let folder = tempfile::tempdir().unwrap();
    let workspace = folder.path().canonicalize().unwrap();
    let marker = workspace.join("connected");
    std::fs::write(
        workspace.join("mcp.json"),
        json!({ "mcpServers": { "own": {
        "command": "sh", "args": ["-c", "touch \"$1\"; exit 1", "mcp-test", marker]
    } } })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        workspace.join(".mcp.json"),
        json!({ "mcpServers": { "project": { "command": "untrusted-project-server" } } }).to_string(),
    )
    .unwrap();
    let servers = siphon_mcp::Servers::start(siphon::machine::Host::default(), Some(&workspace));
    let beak = Beak::new().unwrap();
    let calling = siphon::testing::Calling::start(&beak, |tools| {
        tools.with(SpawnExplorer).with(ExplorerCatalog {
            servers,
            place: siphon::tools::Place::of(&workspace),
        })
    });
    let child = calling
        .called(
            "spawn",
            workspace.clone(),
            Vec::new(),
            vec![("spawn".into(), "spawn_explorer".into(), "{}".into())],
        )
        .await;
    assert_eq!(child[0].status, siphon::tools::CallStatus::Ok, "{}", child[0].text);
    let child = child[0].structured_content.as_ref().unwrap().clone();
    let result = calling
        .called(
            "catalog",
            workspace,
            Vec::new(),
            vec![("catalog".into(), "explorer_catalog".into(), child.to_string())],
        )
        .await;
    assert_eq!(result[0].status, siphon::tools::CallStatus::Ok, "{}", result[0].text);
    assert_eq!(
        result[0].structured_content,
        Some(json!({ "sections": [], "tools": [], "asks": 0 }))
    );
    assert!(!marker.exists(), "an MCP server was never launched");
    calling.close().await;
}
