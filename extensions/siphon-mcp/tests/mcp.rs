//! MCP servers: configured by the person and by a project, a project's run once the person trusts its configuration
//! as it is; each a command the machine runs, connected as first needed and shared; their tools listed for the model
//! and called as its own — directly, or from a script.

use std::path::{Path, PathBuf};
use std::time::Duration;

use beak::{Beak, layout};
use bytes::Bytes;
use serde_json::json;
use siphon::machine::Host;
use siphon::testing::{Called, Calling};
use siphon::tools::CallStatus;
use siphon::tools::Place;
use siphon::trust::{self, TrustProgram};
use siphon_mcp::{Launch, Servers, parse};
use siphon_runtime::Changes;

fn fixture() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mcp_server.py")
        .to_string_lossy()
        .into_owned()
}

fn folder() -> (tempfile::TempDir, PathBuf) {
    let folder = tempfile::tempdir().expect("a folder");
    let path = folder.path().canonicalize().expect("its real path");
    (folder, path)
}

/// A Home whose `mcp.json` names the fixture as the server `test`.
fn home() -> (tempfile::TempDir, PathBuf) {
    let (kept, home) = folder();
    let configuration = json!({ "mcpServers": { "test": {
        "command": "python3",
        "args": [fixture()],
        "env": { "GREETING": "${SIPHON_TEST_UNSET:-hi there}" }
    } } });
    std::fs::write(home.join("mcp.json"), configuration.to_string()).expect("written");
    (kept, home)
}

/// Calls run by Siphon's own tools and the servers' `servers`.
fn calling(beak: &Beak, servers: &Servers) -> Calling {
    let servers = servers.clone();
    Calling::start(beak, move |tools| tools.with_toolset(std::sync::Arc::new(servers)))
}

/// What a call came to, as these tests compare it: how it ended, its text, its images, and what it told typed.
fn said(called: Called) -> (CallStatus, String, usize, Option<serde_json::Value>) {
    (called.status, called.text, called.images, called.structured)
}

fn call(id: &str, tool: &str, arguments: &serde_json::Value) -> (String, String, String) {
    (id.into(), tool.into(), arguments.to_string())
}

#[test]
fn a_configuration_names_commands_and_urls_and_says_what_it_could_not_take() {
    let text = json!({ "mcpServers": {
        "run": { "command": "npx", "args": ["-y", "${PACKAGE}"], "env": { "KEY": "${KEY:-none}" }, "cwd": "tools" },
        "far": { "type": "http", "url": "https://example.com/mcp", "headers": { "Authorization": "Bearer ${TOKEN}" } },
        "near": { "url": "http://localhost:3000/mcp" },
        "old": { "type": "sse", "url": "https://example.com/sse" },
        "dotted.name": { "command": "x" },
        "unset": { "command": "${MISSING}" },
        "empty": {}
    } })
    .to_string();
    let variables = |name: &str| match name {
        "PACKAGE" => Some("server-everything".to_owned()),
        "TOKEN" => Some("secret".to_owned()),
        _ => None,
    };
    let configured = parse(Path::new("/p/.mcp.json"), &text, Path::new("/p"), &variables);
    let launches: Vec<(&str, &Launch)> = configured
        .servers
        .iter()
        .map(|server| (server.name.as_str(), &server.launch))
        .collect();
    assert_eq!(
        launches,
        [
            (
                "far",
                &Launch::Http {
                    url: "https://example.com/mcp".into(),
                    headers: vec![("Authorization".into(), "Bearer secret".into())]
                }
            ),
            (
                "near",
                &Launch::Http {
                    url: "http://localhost:3000/mcp".into(),
                    headers: vec![]
                }
            ),
            (
                "run",
                &Launch::Command {
                    command: "npx".into(),
                    args: vec!["-y".into(), "server-everything".into()],
                    env: vec![("KEY".into(), "none".into())],
                    cwd: "/p/tools".into()
                }
            ),
        ]
    );
    assert_eq!(configured.servers[2].runs(), "npx -y server-everything");
    let problems = configured.problems.join("\n");
    for said in [
        "dotted.name of /p/.mcp.json: its name is empty or holds a dot",
        "empty of /p/.mcp.json: it names neither a command nor a URL",
        "old of /p/.mcp.json: a server over SSE is not taken",
        "unset of /p/.mcp.json: ${MISSING} is not set",
    ] {
        assert!(problems.contains(said), "{problems}");
    }
    let broken = parse(Path::new("/p/.mcp.json"), "{", Path::new("/p"), &variables);
    assert!(
        broken.servers.is_empty() && broken.problems[0].starts_with("/p/.mcp.json: EOF"),
        "{broken:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_person_s_servers_tools_are_listed_and_called_as_the_model_s_own() {
    let (_home_folder, home) = home();
    let servers = Servers::start(Host::default(), Some(&home));
    let beak = Beak::new().expect("Beak");
    let calling = calling(&beak, &servers);
    let runtime = &calling.runtime;
    let volume = calling.volume();
    let (_folder, workspace) = folder();
    let catalog = servers.catalog(&volume, &Place::of(&workspace)).await.expect("a catalog");
    assert_eq!(catalog.problems, Vec::<String>::new());
    let names: Vec<&str> = catalog.tools.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, ["test.say", "test.fail", "test.where", "test.picture"]);
    let told = siphon::tools::declarations(&catalog.tools);
    assert!(told.starts_with("interface Tools {\n  test: {\n"), "{told}");
    assert!(told.contains("    /**\n     * Says the words back.\n     */\n    say(args: {\n      /**\n       * What to say.\n       */\n      words: string;\n    }): Promise<string>;\n"), "{told}");
    let catalog: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
    let said: Vec<_> = calling
        .called(
            "p1",
            workspace.clone(),
            catalog,
            vec![
                call("a", "test.say", &json!({ "words": "hello" })),
                call("b", "test.fail", &json!({})),
                call("c", "test.where", &json!({})),
                call("d", "test.picture", &json!({})),
                call("e", "other.say", &json!({})),
                call(
                    "f",
                    "execute",
                    &json!({ "code": "return await tools.test.say({ words: 'from a script' });" }),
                ),
            ],
        )
        .await
        .into_iter()
        .map(said)
        .collect();
    assert_eq!(
        said[0],
        (CallStatus::Ok, "hello".into(), 0, Some(json!({ "words": "hello" }))),
        "its structured content kept beside what the model reads"
    );
    assert_eq!(said[1], (CallStatus::ToolError, "it failed".into(), 0, None));
    assert_eq!(
        said[2],
        (CallStatus::Ok, format!("{} hi there", home.display()), 0, None),
        "run in its configuration's folder"
    );
    assert_eq!(said[3], (CallStatus::Ok, "an image of 1×1".into(), 1, None));
    assert_eq!(
        said[4],
        (CallStatus::ToolError, "no MCP server other where this thread works".into(), 0, None)
    );
    assert_eq!(said[5], (CallStatus::Ok, r#""from a script""#.into(), 0, None));
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_s_servers_run_once_the_person_trusts_its_configuration_as_it_is() {
    let servers = Servers::start(Host::default(), None);
    let beak = Beak::new().expect("Beak");
    let calling = calling(&beak, &servers);
    let runtime = &calling.runtime;
    let volume = calling.volume();
    let (_folder, project) = folder();
    std::fs::create_dir(project.join(".git")).expect("a repository");
    let workspace = project.join("app");
    std::fs::create_dir(&workspace).expect("a workspace");
    let configuration = json!({ "mcpServers": { "test": { "command": "python3", "args": [fixture()] } } }).to_string();
    std::fs::write(project.join(".mcp.json"), &configuration).expect("written");
    let place = Place::of(&workspace);
    assert_eq!(place.project, project);

    let catalog = servers.catalog(&volume, &place).await.expect("a catalog");
    assert_eq!(catalog.tools, []);
    assert!(
        catalog.problems[0].ends_with("are not run till the person trusts them"),
        "{:?}",
        catalog.problems
    );
    let asks = catalog.asks.expect("asked");
    assert_eq!(asks.servers[0].runs, format!("python3 {}", fixture()));

    let mut changes = Changes::new();
    changes
        .start::<TrustProgram>(Bytes::new(), TrustProgram::KEY, &asks)
        .expect("an input");
    runtime.commit(changes).await.expect("started");
    let process = layout::process(b"", TrustProgram::KEY.as_bytes());
    let request = layout::request(&process, TrustProgram::KEY.as_bytes());
    let mut committed = runtime.committed();
    tokio::time::timeout(Duration::from_secs(10), async {
        while runtime.snapshot().expect("a snapshot").stat(&request).expect("read").is_none() {
            committed.changed().await.expect("the runtime runs");
        }
    })
    .await
    .expect("asked in time");
    runtime
        .answer(&process, TrustProgram::KEY.as_bytes(), &json!({ "trusted": true }))
        .await
        .expect("answered");
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let snapshot = runtime.snapshot().expect("a snapshot");
            if trust::decided(&snapshot, &asks.folder, &asks.digest).expect("read") == Some(true) {
                break;
            }
            committed.changed().await.expect("the runtime runs");
        }
    })
    .await
    .expect("decided in time");

    let catalog = servers.catalog(&volume, &place).await.expect("a catalog");
    assert_eq!(catalog.tools.len(), 4, "{:?}", catalog.problems);
    assert!(catalog.asks.is_none());

    let other = json!({ "mcpServers": { "test": { "command": "python3", "args": [fixture(), "--more"] } } }).to_string();
    std::fs::write(project.join(".mcp.json"), other).expect("written");
    let catalog = servers.catalog(&volume, &place).await.expect("a catalog");
    assert!(
        catalog.tools.is_empty() && catalog.asks.is_some(),
        "a changed configuration is asked again"
    );
    runtime.close().await;
}
