//! A model's calls taken on by their caller, a process of their own: checked, weighed, the person or a second model
//! asked where the weighing says, exec'd as their tools' requests — those that may act one after another, a read after
//! them seeing what they did — and each ended once.

use std::path::PathBuf;
use std::time::Duration;

use crate::machine::Host;
use crate::machine::paths::Base;
use crate::threads::{Mode, SubThreadKind};
use crate::tools::{
    CallStatus, Calls, Context, Effect, Exec, ExecuteTool, GlobTool, GrepTool, Judgment, Permission, ReadTool, Refusal, Review,
    ScriptFiber, Setting, ShellTool, State, Told, Tool, Tools, WriteTool, answer_exec, projects,
};
use beak::{Beak, Outcome, layout};
use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use serde_json::json;
use siphon_runtime::{Asked, Changes, Ending, Fiber, NotGiven, Options, Run, Runtime, Wait};

#[test]
fn a_waiting_tool_fiber_reads_the_existing_call_checkpoint() {
    let checkpoint = json!([{
        "id": "call-1",
        "key": "1",
        "tool": "execute",
        "arguments": "{}",
        "acts": true,
        "state": {"state": "program"}
    }]);
    let calls: Calls = serde_json::from_value(checkpoint.clone()).unwrap();
    assert_eq!(calls.0[0].state, State::Fiber);
    assert_eq!(serde_json::to_value(calls).unwrap(), checkpoint);
}

fn tools() -> Tools {
    Tools::new()
        .with(ReadTool)
        .with(WriteTool)
        .with(ShellTool)
        .with(Pick)
        .with(GlobTool)
        .with(GrepTool)
        .with(ExecuteTool)
        .with(Browser)
        .with(Delegate)
        .with_toolset(std::sync::Arc::new(Server))
}

/// A browser action claiming to read, proving that explorer admission is based on names rather than effects.
struct Browser;
impl Tool for Browser {
    const NAME: &'static str = "browser";
    const DESCRIPTION: &'static str = "A browser action";
    type Args = serde_json::Value;
    fn effect(&self, _: &Self::Args, _: &Base) -> Effect {
        Effect::Reads
    }
    fn exec(&self, _: &Context, _: Self::Args) -> impl std::future::Future<Output = Result<Told, Refusal>> + Send {
        std::future::poll_fn(|_| panic!("an unavailable browser action must never execute"))
    }
}

struct Delegate;
impl Tool for Delegate {
    const NAME: &'static str = "delegate";
    const DESCRIPTION: &'static str = "A delegation";
    const REACHES_THREADS: bool = true;
    type Args = serde_json::Value;
    fn effect(&self, _: &Self::Args, _: &Base) -> Effect {
        Effect::Delegates
    }
    fn exec(&self, _: &Context, _: Self::Args) -> impl std::future::Future<Output = Result<Told, Refusal>> + Send {
        std::future::poll_fn(|_| panic!("a sub-thread must never delegate"))
    }
}

struct Server;
impl crate::tools::Toolset for Server {
    fn has(&self, name: &str) -> bool {
        name == "server.write"
    }
    fn call<'a>(
        &'a self,
        _: &'a Context,
        _: &'a str,
        _: serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Told, Refusal>> + Send + 'a>> {
        Box::pin(async { panic!("an explorer must never reach an MCP server") })
    }
}

/// The arguments of `pick`: the colours the person picks one of.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
struct PickArgs {
    colours: Vec<String>,
}

/// `pick`: a tool the person answers, by picking one of the colours it offers.
struct Pick;

impl Tool for Pick {
    const NAME: &'static str = "pick";
    const DESCRIPTION: &'static str = "Asks the person to pick a colour.";
    type Args = PickArgs;

    fn effect(&self, _: &PickArgs, _: &Base) -> Effect {
        Effect::Reads
    }

    fn exec(&self, _: &Context, _: PickArgs) -> impl std::future::Future<Output = Result<Told, Refusal>> + Send {
        std::future::ready(Err(Refusal::new("the person picks")))
    }

    fn asks(&self, args: &PickArgs) -> Option<serde_json::Value> {
        Some(json!({ "enum": args.colours }))
    }

    fn answered(&self, _: &PickArgs, answer: serde_json::Value) -> Told {
        Told {
            content: vec![InputContent::text(format!("picked {}", answer.as_str().unwrap_or_default()))],
            structured_content: Some(answer),
            metadata: Some(json!({ "title": "Picked colour" })),
            ..Told::default()
        }
    }
}

/// What a caller is given: the thread, how it runs, where it works, and the calls its model made.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Asking {
    thread: String,
    mode: Mode,
    workspace: PathBuf,
    calls: Vec<(String, String, String)>,
}

/// Takes its calls on till each ended, then ends with how each did and what it said.
struct Caller {
    tools: Tools,
}

impl Fiber for Caller {
    const KIND: &'static str = "test.caller";
    type Input = Asking;
    type Checkpoint = Calls;
    type Output = Vec<(CallStatus, String)>;

    fn run(&self, run: &mut Run<'_>, input: Asking, checkpoint: Option<Calls>) -> anyhow::Result<Ending<Calls, Vec<(CallStatus, String)>>> {
        let mut calls = match checkpoint {
            Some(calls) => calls,
            None => Calls::new(run, input.calls)?,
        };
        let base = Base {
            workspace: input.workspace.clone(),
            home: None,
        };
        let folder = input.workspace.to_string_lossy().into_owned();
        let catalog = vec!["server.write".to_owned()];
        let setting = Setting {
            thread: &input.thread,
            threads: None,
            tools: &self.tools,
            primitives: &crate::agent::ThreadPrimitives,
            catalog: &catalog,
            mode: input.mode,
            base: &base,
            project: Some(&folder),
            places: &[],
            confines: beak::machine_confines(),
        };
        match calls.advance(run, &setting)? {
            Some(wait) => Ok(Ending::Wait(calls, wait)),
            None => Ok(Ending::Done(calls.0.into_iter().map(said).collect())),
        }
    }
}

fn said(call: crate::tools::Call) -> (CallStatus, String) {
    let crate::tools::State::Ended {
        status,
        output,
        structured_content,
        metadata,
        ..
    } = call.state
    else {
        return (CallStatus::Interrupted, String::new());
    };
    if call.tool == "pick" && status == CallStatus::Ok {
        assert_eq!(
            structured_content,
            Some(json!("blue")),
            "the completed call keeps the person's answer"
        );
        assert_eq!(
            metadata,
            Some(json!({ "title": "Picked colour" })),
            "answer presentation survives completion"
        );
    }
    let text = output
        .iter()
        .filter_map(|part| match part {
            InputContent::InputText { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    (status, text)
}

/// A runtime whose caller's tools are exec'd, whose permissions the person answers, and whose second model lets every
/// command go ahead but one that removes.
fn runtime(beak: &Beak) -> Runtime {
    Runtime::builder()
        .fiber(Caller { tools: tools() })
        .fiber(RecoveredCaller)
        .fiber(ScriptFiber {
            tools: tools(),
            primitives: std::sync::Arc::new(crate::agent::ThreadPrimitives),
        })
        .handle(|asked: Asked<Exec>| async move { answer_exec(&tools(), &Host::default(), asked).await })
        .ask_person::<Permission>()
        .ask_person::<crate::tools::Asks>()
        .handle(|asked: Asked<Review>| async move {
            let removes = asked.payload.shown.does.iter().any(|does| does.contains(" rm "));
            Ok(Judgment {
                allow: !removes,
                reason: if removes { "it removes files".into() } else { "it builds".into() },
                usage: None,
            })
        })
        .start(beak, Options::default())
        .expect("started")
}

async fn caller(runtime: &Runtime, key: &str, asking: &Asking) -> Bytes {
    let mut changes = Changes::new();
    changes.start::<Caller>(Bytes::new(), key, asking).expect("an input");
    runtime.commit(changes).await.expect("started");
    layout::process(b"", key.as_bytes())
}

async fn ended(runtime: &Runtime, path: &[u8]) -> Vec<(CallStatus, String)> {
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
    tokio::time::timeout(Duration::from_secs(10), waiting).await.expect("ended in time")
}

/// The key of the request named `name` the process at `path` waits on, once it does.
async fn asked(runtime: &Runtime, path: &[u8], name: &[u8]) -> String {
    let waiting = async {
        loop {
            let snapshot = runtime.snapshot().expect("a snapshot");
            let query = beak::Query {
                kind: layout::REQUEST_KIND,
                name: layout::REQUEST_LABEL,
                value: Some(name),
                under: path,
                after: None,
            };
            if let Some(found) = snapshot.find(&query, 10).expect("found").first() {
                return String::from_utf8_lossy(&found.path).rsplit(':').next().expect("its key").to_owned();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), waiting).await.expect("asked in time")
}

fn call(id: &str, tool: &str, arguments: &serde_json::Value) -> (String, String, String) {
    (id.into(), tool.into(), arguments.to_string())
}

fn folder() -> (tempfile::TempDir, PathBuf) {
    let folder = tempfile::tempdir().expect("a folder");
    let path = folder.path().canonicalize().expect("its real path");
    (folder, path)
}

async fn sub_thread(runtime: &Runtime, id: &str, kind: SubThreadKind) {
    let mut changes = Changes::new();
    changes.mkdir(crate::threads::thread::path(id)).meta(
        crate::threads::thread::path(id),
        vec![(
            Bytes::from_static(crate::threads::thread::PARENT_LABEL.as_bytes()),
            Some(Bytes::from_static(b"parent")),
        )],
        vec![(
            Bytes::from_static(crate::threads::thread::SUB_THREAD_KIND_ANNOTATION.as_bytes()),
            Some(serde_json::to_vec(&kind).unwrap().into()),
        )],
    );
    runtime.commit(changes).await.unwrap();
}

fn no_requests(runtime: &Runtime, under: &[u8]) {
    let query = beak::Query {
        kind: layout::REQUEST_KIND,
        name: layout::REQUEST_LABEL,
        value: None,
        under,
        after: None,
    };
    assert_eq!(runtime.snapshot().unwrap().find(&query, 100).unwrap(), Vec::<beak::Found>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn explorers_keep_their_read_scope_in_full_access_and_with_existing_project_grants() {
    let beak = Beak::new().unwrap();
    let runtime = runtime(&beak);
    let (_folder, workspace) = folder();
    std::fs::write(workspace.join("source.txt"), "readable").unwrap();
    sub_thread(&runtime, "explorer", SubThreadKind::Explorer).await;
    let grants = crate::tools::permissions::Grants {
        commands: vec!["*".into()],
        paths: vec!["*".into()],
        tools: vec!["*".into()],
    };
    runtime
        .commit(projects::grant(&runtime.snapshot().unwrap(), &workspace.to_string_lossy(), grants.clone()).unwrap())
        .await
        .unwrap();
    for (key, mode) in [("full", Mode::FullAccess), ("granted", Mode::Default)] {
        let asking = Asking {
            thread: "explorer".into(),
            mode,
            workspace: workspace.clone(),
            calls: vec![
                call("write", "write", &json!({ "path": "forbidden.txt", "content": "changed" })),
                call("shell", "shell", &json!({ "command": "touch forbidden.txt", "escalate": true })),
                call(
                    "browser",
                    "browser",
                    &json!({ "action": "evaluate", "expression": "fetch('/write')" }),
                ),
                call("server", "server.write", &json!({})),
                call("delegate", "delegate", &json!({})),
                call("read", "read", &json!({ "path": "source.txt" })),
                call("glob", "glob", &json!({ "pattern": "*.txt" })),
                call("grep", "grep", &json!({ "pattern": "readable" })),
            ],
        };
        let path = caller(&runtime, key, &asking).await;
        let results = ended(&runtime, &path).await;
        assert_eq!(
            results.iter().map(|(status, _)| *status).collect::<Vec<_>>(),
            [
                CallStatus::Unavailable,
                CallStatus::Unavailable,
                CallStatus::Unavailable,
                CallStatus::Unavailable,
                CallStatus::Unavailable,
                CallStatus::Ok,
                CallStatus::Ok,
                CallStatus::Ok,
            ],
            "{results:?}"
        );
        assert!(!workspace.join("forbidden.txt").exists());
        no_requests(&runtime, &path);
    }
    assert_eq!(
        projects::grants(&runtime.snapshot().unwrap(), &workspace.to_string_lossy()).unwrap(),
        grants
    );
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn explorer_scripts_expose_readers_and_cannot_request_write_permissions() {
    let beak = Beak::new().unwrap();
    let runtime = runtime(&beak);
    let (_folder, workspace) = folder();
    std::fs::write(workspace.join("source.txt"), "readable").unwrap();
    sub_thread(&runtime, "explorer", SubThreadKind::Explorer).await;
    for (key, code, status) in [
        ("bindings", "return Object.keys(tools).sort().join(',')", CallStatus::Ok),
        (
            "read",
            "return (await tools.read({path: 'source.txt'})).structuredContent.text",
            CallStatus::Ok,
        ),
        (
            "write",
            "await tools.write({path: 'forbidden.txt', content: 'changed'})",
            CallStatus::ToolError,
        ),
        (
            "shell",
            "await tools.shell({command: 'touch forbidden.txt'})",
            CallStatus::ToolError,
        ),
        (
            "browser",
            "await tools.browser({action: 'evaluate', expression: 'write()'})",
            CallStatus::ToolError,
        ),
        ("server", "await tools.server.write({})", CallStatus::ToolError),
        ("delegate", "await tools.delegate({task: 'write'})", CallStatus::ToolError),
    ] {
        let asking = Asking {
            thread: "explorer".into(),
            mode: Mode::FullAccess,
            workspace: workspace.clone(),
            calls: vec![call("script", "execute", &json!({ "code": code }))],
        };
        let path = caller(&runtime, key, &asking).await;
        let results = ended(&runtime, &path).await;
        assert_eq!(results[0].0, status, "{key}: {results:?}");
        if key == "read" {
            assert!(results[0].1.contains("readable"));
        }
        if key == "bindings" {
            assert_eq!(results[0].1, "\"execute,glob,grep,read\"");
        }
        assert!(!workspace.join("forbidden.txt").exists());
        no_requests(&runtime, &path);
    }
    assert_eq!(
        projects::grants(&runtime.snapshot().unwrap(), &workspace.to_string_lossy()).unwrap(),
        crate::tools::permissions::Grants::default()
    );
    runtime.close().await;
}

/// Recreates a permission wait whose answer the current call rules must recheck.
struct RecoveredCaller;
impl Fiber for RecoveredCaller {
    const KIND: &'static str = "test.recovered_caller";
    type Input = Asking;
    type Checkpoint = Calls;
    type Output = Vec<(CallStatus, String)>;
    fn run(&self, run: &mut Run<'_>, input: Asking, checkpoint: Option<Calls>) -> anyhow::Result<Ending<Calls, Self::Output>> {
        if let Some(calls) = checkpoint {
            return Caller { tools: tools() }.run(run, input, Some(calls));
        }
        let mut calls = Calls::new(run, input.calls)?;
        let call = &mut calls.0[0];
        call.state = crate::tools::State::Asking;
        let key = format!("{}.permission", call.key);
        run.request(
            &key,
            &Permission {
                thread: input.thread,
                call: call.key.clone(),
                shown: crate::tools::requests::Shown {
                    tool: call.tool.clone(),
                    arguments: call.arguments.clone(),
                    does: Vec::new(),
                    justification: None,
                },
                always: crate::tools::permissions::Grants::default(),
            },
        )?;
        Ok(Ending::Wait(calls, Wait::new().mail(key)))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn recovered_explorer_approval_cannot_grant_or_execute_an_unavailable_tool() {
    let beak = Beak::new().unwrap();
    let runtime = runtime(&beak);
    let (_folder, workspace) = folder();
    let (_outside, outside) = folder();
    let target = outside.join("forbidden.txt");
    sub_thread(&runtime, "explorer", SubThreadKind::Explorer).await;
    let asking = Asking {
        thread: "explorer".into(),
        mode: Mode::FullAccess,
        workspace: workspace.clone(),
        calls: vec![call("write", "write", &json!({ "path": target, "content": "changed" }))],
    };
    let mut changes = Changes::new();
    changes.start::<RecoveredCaller>(Bytes::new(), "restore", &asking).unwrap();
    runtime.commit(changes).await.unwrap();
    let path = layout::process(b"", b"restore");
    let key = asked(&runtime, &path, b"siphon.permission").await;
    runtime
        .answer(&path, key.as_bytes(), &json!({ "decision": "allow", "always": true }))
        .await
        .unwrap();
    assert_eq!(ended(&runtime, &path).await[0].0, CallStatus::Unavailable);
    assert!(!target.exists());
    assert_eq!(
        projects::grants(&runtime.snapshot().unwrap(), &workspace.to_string_lossy()).unwrap(),
        crate::tools::permissions::Grants::default()
    );
    no_requests(&runtime, &path);
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn general_purpose_sub_threads_keep_normal_permissions_and_cannot_delegate() {
    let beak = Beak::new().unwrap();
    let runtime = runtime(&beak);
    let (_folder, workspace) = folder();
    sub_thread(&runtime, "general", SubThreadKind::GeneralPurpose).await;
    let asking = Asking {
        thread: "general".into(),
        mode: Mode::FullAccess,
        workspace: workspace.clone(),
        calls: vec![
            call("write", "write", &json!({ "path": "made.txt", "content": "changed" })),
            call("delegate", "delegate", &json!({})),
        ],
    };
    let path = caller(&runtime, "general", &asking).await;
    let results = ended(&runtime, &path).await;
    assert_eq!(
        results.iter().map(|(status, _)| *status).collect::<Vec<_>>(),
        [CallStatus::Ok, CallStatus::Unavailable]
    );
    assert_eq!(std::fs::read_to_string(workspace.join("made.txt")).unwrap(), "changed");
    let (_outside, outside) = folder();
    let target = outside.join("approved.txt");
    let asking = Asking {
        thread: "general".into(),
        mode: Mode::Default,
        workspace: workspace.clone(),
        calls: vec![call("approval", "write", &json!({ "path": target, "content": "changed" }))],
    };
    let path = caller(&runtime, "general-default", &asking).await;
    let key = asked(&runtime, &path, b"siphon.permission").await;
    runtime.answer(&path, key.as_bytes(), &json!({ "decision": "deny" })).await.unwrap();
    assert_eq!(ended(&runtime, &path).await[0].0, CallStatus::Rejected);
    assert!(!target.exists());
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn calls_that_act_go_one_after_another_and_a_read_after_them_sees_what_they_did() {
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak);
    let (_folder, workspace) = folder();
    let asking = Asking {
        thread: "t1".into(),
        mode: Mode::Default,
        workspace: workspace.clone(),
        calls: vec![
            call("a", "write", &json!({ "path": "a.txt", "content": "one\n" })),
            call("b", "shell", &json!({ "command": "echo two >> a.txt" })),
            call("c", "read", &json!({ "path": "a.txt" })),
            call("d", "nonesuch", &json!({})),
            call("e", "read", &json!({ "limit": 3 })),
        ],
    };
    let path = caller(&runtime, "p1", &asking).await;
    let said = ended(&runtime, &path).await;
    let statuses: Vec<CallStatus> = said.iter().map(|(status, _)| *status).collect();
    assert_eq!(
        statuses,
        [
            CallStatus::Ok,
            CallStatus::Ok,
            CallStatus::Ok,
            CallStatus::Unavailable,
            CallStatus::InvalidArguments
        ],
        "{said:?}"
    );
    assert_eq!(said[2].1, "one\ntwo", "read after what acted before it");
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_the_person_lets_go_ahead_for_good_is_granted_its_project_and_one_they_decline_is_rejected() {
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak);
    let (_folder, workspace) = folder();
    let (_outside_folder, outside) = folder();
    let target = outside.join("made.txt");
    let asking = Asking {
        thread: "t1".into(),
        mode: Mode::Default,
        workspace: workspace.clone(),
        calls: vec![
            call(
                "a",
                "shell",
                &json!({ "command": format!("touch {}", target.display()), "escalate": true, "justification": "to make it" }),
            ),
            call("b", "write", &json!({ "path": outside.join("other.txt"), "content": "x" })),
        ],
    };
    let path = caller(&runtime, "p1", &asking).await;
    for (key, answer) in [
        ("permission", json!({ "decision": "allow", "always": true })),
        ("permission", json!({ "decision": "deny", "reason": "not there" })),
    ] {
        let waiting = async {
            loop {
                let snapshot = runtime.snapshot().expect("a snapshot");
                let asked = snapshot
                    .find(
                        &beak::Query {
                            kind: layout::REQUEST_KIND,
                            name: layout::REQUEST_LABEL,
                            value: Some(b"siphon.permission"),
                            under: &path,
                            after: None,
                        },
                        10,
                    )
                    .expect("found");
                if let Some(found) = asked.first() {
                    let name = String::from_utf8_lossy(&found.path).rsplit(':').next().expect("its key").to_owned();
                    let payload = snapshot.read(&layout::payload(&found.path), 0, u64::MAX).expect("read");
                    let payload: serde_json::Value = serde_json::from_slice(&payload.unwrap_or_default()).expect("JSON");
                    return (name, payload);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        let (request, payload) = tokio::time::timeout(Duration::from_secs(10), waiting).await.expect("asked in time");
        assert!(request.ends_with(&format!(".{key}")));
        let call = payload["call"].as_str().expect("its call");
        assert_eq!(request, format!("{call}.{key}"), "the ask names its call by Octoii's key of it");
        runtime.answer(&path, request.as_bytes(), &answer).await.expect("answered");
    }
    let said = ended(&runtime, &path).await;
    assert_eq!(said[0].0, CallStatus::Ok);
    assert!(target.exists(), "run beyond the sandbox, as the person let it");
    assert_eq!(said[1], (CallStatus::Rejected, "the person declined it: not there".into()));
    let grants = projects::grants(&runtime.snapshot().expect("a snapshot"), &workspace.to_string_lossy()).expect("read");
    assert_eq!(grants.commands, ["touch *"]);
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn in_auto_a_second_model_judges_what_would_ask() {
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak);
    let (_folder, workspace) = folder();
    let asking = Asking {
        thread: "t1".into(),
        mode: Mode::Auto,
        workspace: workspace.clone(),
        calls: vec![
            call("a", "shell", &json!({ "command": "echo built > out.txt" })),
            call("b", "shell", &json!({ "command": "rm -rf out.txt" })),
            call("c", "shell", &json!({ "command": "ls" })),
        ],
    };
    let path = caller(&runtime, "p1", &asking).await;
    let said = ended(&runtime, &path).await;
    assert_eq!((said[0].0, said[2].0), (CallStatus::Ok, CallStatus::Ok));
    assert_eq!(said[1], (CallStatus::Rejected, "blocked: it removes files".into()));
    assert!(workspace.join("out.txt").exists(), "what was blocked did not run");
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_s_long_output_is_cut_to_its_head_and_tail_the_whole_kept_in_its_thread() {
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak);
    let mut changes = Changes::new();
    changes.mkdir(crate::threads::thread::path("t1"));
    runtime.commit(changes).await.expect("a thread's directory");
    let (_folder, workspace) = folder();
    let asking = Asking {
        thread: "t1".into(),
        mode: Mode::Default,
        workspace,
        calls: vec![call("a", "shell", &json!({ "command": "seq 1 20000" }))],
    };
    let path = caller(&runtime, "p1", &asking).await;
    let said = ended(&runtime, &path).await;
    let text = &said[0].1;
    assert!(
        text.starts_with("1\n2\n3\n") && text.contains("characters left out"),
        "{}",
        &text[..200]
    );
    assert!(text.contains("20000\n<system-reminder>exit 0</system-reminder>"), "its newest tail");
    assert!(text.contains("this host does not expose it for file reads"));
    assert!(!text.contains("kept at siphon.thread:"));
    let snapshot = runtime.snapshot().expect("a snapshot");
    let outputs = crate::threads::thread::within("t1", "outputs");
    let files = snapshot
        .list(
            &outputs,
            &beak::Range {
                limit: usize::MAX,
                ..beak::Range::default()
            },
        )
        .expect("outputs")
        .expect("kept");
    assert_eq!(files.len(), 1);
    let place = [&outputs[..], b"/", &files[0].name].concat();
    let kept = snapshot.read(&place, 0, u64::MAX).expect("read").expect("kept");
    assert!(kept.ends_with(b"19999\n20000\n<system-reminder>exit 0</system-reminder>"));
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tool_the_person_answers_is_answered_with_what_fits_what_it_asks() {
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak);
    let (_folder, workspace) = folder();
    let asking = Asking {
        thread: "t1".into(),
        mode: Mode::Default,
        workspace,
        calls: vec![call("q", "pick", &json!({ "colours": ["red", "blue"] }))],
    };
    let path = caller(&runtime, "p1", &asking).await;
    let key = asked(&runtime, &path, b"siphon.asks").await;
    let unfit = runtime.answer(&path, key.as_bytes(), &json!("green")).await;
    assert!(matches!(unfit, Err(NotGiven::Unfit(_))), "{unfit:?}");
    runtime.answer(&path, key.as_bytes(), &json!("blue")).await.expect("answered");
    let said = ended(&runtime, &path).await;
    assert_eq!(said[0], (CallStatus::Ok, "picked blue".into()));
    runtime.close().await;
}
