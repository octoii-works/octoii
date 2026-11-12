//! MCP servers: configured by the person and by a project, a project's run once the person trusts its configuration
//! as it is; each a command the machine runs, connected as first needed and shared; their tools listed for the model
//! and called as its own — directly, or from a script.

use std::path::{Path, PathBuf};
use std::time::Duration;

use beak::{Beak, Outcome, layout};
use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use serde_json::json;
use siphon_machine::Host;
use siphon_machine::paths::Base;
use siphon_mcp::trust::{self, Trust, TrustProgram};
use siphon_mcp::{Launch, Servers, parse};
use siphon_runtime::{Asked, Changes, Ending, Options, Program, Run, Runtime};
use siphon_threads::Mode;
use siphon_tools::projects::Place;
use siphon_tools::{CallStatus, Calls, Exec, ExecuteTool, ReadTool, ScriptProgram, Setting, Tools, answer_exec};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Asking {
    workspace: PathBuf,
    catalog: Vec<String>,
    calls: Vec<(String, String, String)>,
}

/// Takes its calls on in full access, then ends with how each did and what it said, and how many images it showed.
struct Caller {
    tools: Tools,
}

impl Program for Caller {
    const KIND: &'static str = "test.caller";
    type Input = Asking;
    type Checkpoint = Calls;
    type Output = Vec<(CallStatus, String, usize, Option<serde_json::Value>)>;

    fn run(&self, run: &mut Run<'_>, input: Asking, checkpoint: Option<Calls>) -> anyhow::Result<Ending<Calls, Self::Output>> {
        let mut calls = match checkpoint {
            Some(calls) => calls,
            None => Calls::new(run, input.calls.clone())?,
        };
        let base = Base {
            workspace: input.workspace.clone(),
            home: None,
        };
        let setting = Setting {
            thread: "t1",
            tools: &self.tools,
            catalog: &input.catalog,
            mode: Mode::FullAccess,
            base: &base,
            project: None,
            confines: beak::machine_confines(),
        };
        if let Some(wait) = calls.advance(run, &setting)? {
            return Ok(Ending::Wait(calls, wait));
        }
        Ok(Ending::Done(calls.0.into_iter().map(said).collect()))
    }
}

fn said(call: siphon_tools::Call) -> (CallStatus, String, usize, Option<serde_json::Value>) {
    let siphon_tools::State::Ended {
        status,
        output,
        structured,
        ..
    } = call.state
    else {
        return (CallStatus::Interrupted, String::new(), 0, None);
    };
    let mut texts = Vec::new();
    let mut images = 0;
    for part in &output {
        match part {
            InputContent::InputText { text } => texts.push(text.as_str()),
            InputContent::InputImage { .. } => images += 1,
            InputContent::InputFile { .. } => {}
        }
    }
    (status, texts.join("\n"), images, structured)
}

fn tools(servers: &Servers) -> (Tools, Tools) {
    let inner = Tools::new().with(ReadTool).with_toolset(std::sync::Arc::new(servers.clone()));
    let outer = inner.clone().with(ExecuteTool { tools: inner.clone() });
    (inner, outer.with_toolset(std::sync::Arc::new(servers.clone())))
}

fn runtime(beak: &Beak, servers: &Servers) -> Runtime {
    let (inner, outer) = tools(servers);
    let execs = outer.clone();
    Runtime::builder()
        .program(Caller { tools: outer })
        .program(ScriptProgram { tools: inner })
        .program(TrustProgram)
        .ask_person::<Trust>()
        .handle(move |asked: Asked<Exec>| {
            let tools = execs.clone();
            async move { answer_exec(&tools, &Host::default(), asked).await }
        })
        .start(beak, Options::default())
        .expect("started")
}

async fn ended(runtime: &Runtime, path: &[u8]) -> Vec<(CallStatus, String, usize, Option<serde_json::Value>)> {
    let mut committed = runtime.committed();
    let waiting = async {
        loop {
            let record = runtime.snapshot().expect("a snapshot").record(path).expect("a record");
            if let Some(Outcome::Completed(Some(result))) = record.and_then(|record| record.outcome) {
                return serde_json::from_slice(&result).expect("what it said");
            }
            committed.changed().await.expect("the runtime runs");
        }
    };
    tokio::time::timeout(Duration::from_secs(30), waiting).await.expect("ended in time")
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
    let runtime = runtime(&beak, &servers);
    let (_folder, workspace) = folder();
    let catalog = servers.catalog(&runtime, &Place::of(&workspace)).await.expect("a catalog");
    assert_eq!(catalog.problems, Vec::<String>::new());
    let names: Vec<&str> = catalog.tools.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, ["test.say", "test.fail", "test.where", "test.picture"]);
    let told = siphon_tools::declarations(&catalog.tools);
    assert!(told.starts_with("interface Tools {\n  test: {\n"), "{told}");
    assert!(told.contains("    /**\n     * Says the words back.\n     */\n    say(args: {\n      /**\n       * What to say.\n       */\n      words: string;\n    }): Promise<string>;\n"), "{told}");
    let asking = Asking {
        workspace: workspace.clone(),
        catalog: names.iter().map(|name| (*name).to_owned()).collect(),
        calls: vec![
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
    };
    let mut changes = Changes::new();
    changes.mkdir(siphon_threads::thread::path("t1"));
    changes.start::<Caller>(Bytes::new(), "p1", &asking).expect("an input");
    runtime.commit(changes).await.expect("started");
    let said = ended(&runtime, &layout::process(b"", b"p1")).await;
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
    let runtime = runtime(&beak, &servers);
    let (_folder, project) = folder();
    std::fs::create_dir(project.join(".git")).expect("a repository");
    let workspace = project.join("app");
    std::fs::create_dir(&workspace).expect("a workspace");
    let configuration = json!({ "mcpServers": { "test": { "command": "python3", "args": [fixture()] } } }).to_string();
    std::fs::write(project.join(".mcp.json"), &configuration).expect("written");
    let place = Place::of(&workspace);
    assert_eq!(place.project, project);

    let catalog = servers.catalog(&runtime, &place).await.expect("a catalog");
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

    let catalog = servers.catalog(&runtime, &place).await.expect("a catalog");
    assert_eq!(catalog.tools.len(), 4, "{:?}", catalog.problems);
    assert!(catalog.asks.is_none());

    let other = json!({ "mcpServers": { "test": { "command": "python3", "args": [fixture(), "--more"] } } }).to_string();
    std::fs::write(project.join(".mcp.json"), other).expect("written");
    let catalog = servers.catalog(&runtime, &place).await.expect("a catalog");
    assert!(
        catalog.tools.is_empty() && catalog.asks.is_some(),
        "a changed configuration is asked again"
    );
    runtime.close().await;
}
