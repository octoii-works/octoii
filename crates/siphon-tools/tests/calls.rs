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
use siphon_runtime::{Asked, Changes, Ending, NotGiven, Options, Program, Run, Runtime};
use siphon_threads::Mode;
use siphon_tools::{
    CallStatus, Calls, Exec, Judgment, Permission, QuestionTool, ReadTool, Review, Setting, ShellTool, Tools, WriteTool, answer_exec,
    projects,
};

fn tools() -> Tools {
    Tools::new().with(ReadTool).with(WriteTool).with(ShellTool).with(QuestionTool)
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
        let setting = Setting {
            thread: &input.thread,
            tools: &self.tools,
            catalog: &[],
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

fn said(call: siphon_tools::Call) -> (CallStatus, String) {
    let siphon_tools::State::Ended { status, output, .. } = call.state else {
        return (CallStatus::Interrupted, String::new());
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
        .handle(|asked: Asked<Exec>| async move { answer_exec(&tools(), &Host::default(), asked).await })
        .ask_person::<Permission>()
        .ask_person::<siphon_tools::Asks>()
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
        assert_eq!(request, format!("{call}.{key}"), "the ask names its call by Siphon's key of it");
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
    changes.mkdir(siphon_threads::thread::path("t1"));
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
    assert!(text.contains("20000\n[exit 0]"), "its newest tail");
    let place = text.rsplit("kept at ").next().expect("where").split(';').next().expect("the path");
    let kept = runtime
        .snapshot()
        .expect("a snapshot")
        .read(place.as_bytes(), 0, u64::MAX)
        .expect("read")
        .expect("kept");
    assert!(kept.ends_with(b"19999\n20000\n[exit 0]"));
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_question_is_answered_by_the_person_with_what_fits_the_choices_it_offers() {
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak);
    let (_folder, workspace) = folder();
    let questions = json!({ "questions": [
        { "question": "Which database?", "options": [{ "label": "Postgres" }, { "label": "SQLite" }] },
        { "question": "Anything else?" },
        { "question": "Which features?", "options": [{ "label": "auth" }, { "label": "search" }], "multiple": true }
    ] });
    let asking = Asking {
        thread: "t1".into(),
        mode: Mode::Default,
        workspace,
        calls: vec![call("q", "question", &questions)],
    };
    let path = caller(&runtime, "p1", &asking).await;
    let key = asked(&runtime, &path, b"siphon.asks").await;
    let unfit = runtime
        .answer(&path, key.as_bytes(), &json!({ "answers": [{ "chosen": ["MySQL"] }] }))
        .await;
    assert!(matches!(unfit, Err(NotGiven::Unfit(_))), "{unfit:?}");
    let answer = json!({ "answers": [{ "chosen": ["SQLite"] }, null, { "chosen": ["auth", "search"], "text": "both" }] });
    runtime.answer(&path, key.as_bytes(), &answer).await.expect("answered");
    let said = ended(&runtime, &path).await;
    assert_eq!(
        said[0],
        (
            CallStatus::Ok,
            "Which database?\n→ SQLite\nAnything else?\n→ (unanswered)\nWhich features?\n→ auth, search; both".into()
        )
    );
    runtime.close().await;
}
