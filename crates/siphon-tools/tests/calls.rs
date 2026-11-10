//! A model's calls taken on by their caller, a process of their own: checked, weighed, the person or a second model
//! asked where the weighing says, exec'd as their tools' requests — those that may act one after another, a read after
//! them seeing what they did — and each ended once.

use std::path::PathBuf;
use std::time::Duration;

use beak::{Beak, Outcome, layout};
use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use serde_json::json;
use siphon_machine::Host;
use siphon_machine::paths::Base;
use siphon_runtime::{Asked, Changes, Ending, Options, Program, Run, Runtime, Unanswered};
use siphon_threads::Mode;
use siphon_tools::{Calls, Context, Exec, Judgment, Permission, ReadTool, Review, Setting, ShellTool, Status, Tools, WriteTool, projects};

fn tools() -> Tools {
    Tools::new().with(ReadTool).with(WriteTool).with(ShellTool)
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

impl Program for Caller {
    const KIND: &'static str = "test.caller";
    type Input = Asking;
    type Checkpoint = Calls;
    type Output = Vec<(Status, String)>;

    fn run(&self, run: &mut Run<'_>, input: Asking, checkpoint: Option<Calls>) -> anyhow::Result<Ending<Calls, Vec<(Status, String)>>> {
        let mut calls = match checkpoint {
            Some(calls) => calls,
            None => Calls::new(run, input.calls)?,
        };
        let base = Base {
            workspace: input.workspace.clone(),
            home: None,
        };
        let folder = input.workspace.to_string_lossy().into_owned();
        let setting = Setting {
            thread: &input.thread,
            tools: &self.tools,
            mode: input.mode,
            base: &base,
            project: Some(&folder),
            confines: beak::machine_confines(),
        };
        match calls.advance(run, &setting)? {
            Some(wait) => Ok(Ending::Wait(calls, wait)),
            None => Ok(Ending::Done(calls.0.into_iter().map(said).collect())),
        }
    }
}

fn said(call: siphon_tools::Call) -> (Status, String) {
    let siphon_tools::State::Ended { status, output } = call.state else {
        return (Status::Interrupted, String::new());
    };
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
        .program(Caller { tools: tools() })
        .handle(|asked: Asked<Exec>| async move {
            let exec = asked.payload;
            let context = Context {
                thread: exec.thread,
                process: asked.process,
                call: exec.call,
                base: Base {
                    workspace: PathBuf::from(&exec.workspace),
                    home: None,
                },
                writes: exec.writes,
                kernel: asked.runtime.kernel().clone(),
                host: Host::default(),
                runtime: asked.runtime.clone(),
                cancelled: asked.cancelled,
            };
            tools()
                .exec(&exec.tool, context, exec.arguments)
                .await
                .map_err(|refusal| Unanswered::Failed(refusal.0))
        })
        .ask_person::<Permission>()
        .handle(|asked: Asked<Review>| async move {
            let removes = asked.payload.call.does.iter().any(|does| does.contains(" rm "));
            Ok(Judgment {
                allow: !removes,
                reason: if removes { "it removes files".into() } else { "it builds".into() },
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

async fn ended(runtime: &Runtime, path: &[u8]) -> Vec<(Status, String)> {
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

fn call(id: &str, tool: &str, arguments: &serde_json::Value) -> (String, String, String) {
    (id.into(), tool.into(), arguments.to_string())
}

fn folder() -> (tempfile::TempDir, PathBuf) {
    let folder = tempfile::tempdir().expect("a folder");
    let path = folder.path().canonicalize().expect("its real path");
    (folder, path)
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
    let statuses: Vec<Status> = said.iter().map(|(status, _)| *status).collect();
    assert_eq!(
        statuses,
        [Status::Ok, Status::Ok, Status::Ok, Status::Unavailable, Status::InvalidArguments],
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
                    return name;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        let request = tokio::time::timeout(Duration::from_secs(10), waiting).await.expect("asked in time");
        assert!(request.ends_with(&format!(".{key}")));
        runtime.answer(&path, request.as_bytes(), &answer).await.expect("answered");
    }
    let said = ended(&runtime, &path).await;
    assert_eq!(said[0].0, Status::Ok);
    assert!(target.exists(), "run beyond the sandbox, as the person let it");
    assert_eq!(said[1], (Status::Rejected, "the person declined it: not there".into()));
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
    assert_eq!((said[0].0, said[2].0), (Status::Ok, Status::Ok));
    assert_eq!(said[1], (Status::Rejected, "blocked: it removes files".into()));
    assert!(workspace.join("out.txt").exists(), "what was blocked did not run");
    runtime.close().await;
}
