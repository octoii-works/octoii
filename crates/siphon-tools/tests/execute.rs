//! `execute`: a script of the model's calling its tools — at once, in loops, on what they gave — each call weighed and
//! exec'd as every call is, a call that waits on the person waited for as the script is run again from its start, and
//! the script's end what it returned or threw, with what it wrote to its console.

use std::path::PathBuf;
use std::time::Duration;

use beak::{Beak, Outcome, layout};
use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use serde_json::json;
use siphon_machine::Host;
use siphon_machine::paths::Base;
use siphon_runtime::{Asked, Changes, Ending, Options, Program, Run, Runtime};
use siphon_threads::Mode;
use siphon_tools::{
    CallStatus, Calls, Exec, ExecuteTool, Permission, ReadTool, ScriptProgram, Setting, ShellTool, Tool, Tools, WriteTool, answer_exec,
};

fn inner() -> Tools {
    Tools::new().with(ReadTool).with(WriteTool).with(ShellTool)
}

fn tools() -> Tools {
    inner().with(ExecuteTool { tools: inner() })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Asking {
    workspace: PathBuf,
    code: String,
}

/// Takes on one `execute` call, then ends with how it did and what it said.
struct Caller;

impl Program for Caller {
    const KIND: &'static str = "test.caller";
    type Input = Asking;
    type Checkpoint = Calls;
    type Output = (CallStatus, String);

    fn run(&self, run: &mut Run<'_>, input: Asking, checkpoint: Option<Calls>) -> anyhow::Result<Ending<Calls, (CallStatus, String)>> {
        let mut calls = match checkpoint {
            Some(calls) => calls,
            None => Calls::new(run, vec![("x".into(), "execute".into(), json!({ "code": input.code }).to_string())])?,
        };
        let base = Base {
            workspace: input.workspace.clone(),
            home: None,
        };
        let tools = tools();
        let setting = Setting {
            thread: "t1",
            tools: &tools,
            catalog: &[],
            mode: Mode::Default,
            base: &base,
            project: None,
            confines: beak::machine_confines(),
        };
        if let Some(wait) = calls.advance(run, &setting)? {
            return Ok(Ending::Wait(calls, wait));
        }
        let call = calls.0.remove(0);
        let siphon_tools::State::Ended { status, output, .. } = call.state else {
            anyhow::bail!("not ended")
        };
        let text = output
            .iter()
            .filter_map(|part| match part {
                InputContent::InputText { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Ending::Done((status, text)))
    }
}

fn runtime(beak: &Beak) -> Runtime {
    Runtime::builder()
        .program(Caller)
        .program(ScriptProgram { tools: inner() })
        .handle(|asked: Asked<Exec>| async move { answer_exec(&inner(), &Host::default(), asked).await })
        .ask_person::<Permission>()
        .start(beak, Options::default())
        .expect("started")
}

async fn executed(runtime: &Runtime, workspace: PathBuf, code: &str) -> Bytes {
    let mut changes = Changes::new();
    let asking = Asking {
        workspace,
        code: code.into(),
    };
    changes.start::<Caller>(Bytes::new(), "p1", &asking).expect("an input");
    runtime.commit(changes).await.expect("started");
    layout::process(b"", b"p1")
}

async fn ended(runtime: &Runtime, path: &[u8]) -> (CallStatus, String) {
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
    tokio::time::timeout(Duration::from_secs(20), waiting).await.expect("ended in time")
}

/// The path and key of the permission a process under `under` asks, once one does.
async fn permission(runtime: &Runtime, under: &[u8]) -> (Vec<u8>, String) {
    let waiting = async {
        loop {
            let snapshot = runtime.snapshot().expect("a snapshot");
            let query = beak::Query {
                kind: layout::REQUEST_KIND,
                name: layout::REQUEST_LABEL,
                value: Some(b"siphon.permission"),
                under,
                after: None,
            };
            if let Some(found) = snapshot.find(&query, 10).expect("found").first() {
                let path = String::from_utf8_lossy(&found.path).into_owned();
                let marker = format!("/{}:", String::from_utf8_lossy(layout::REQUEST_KIND));
                let (process, key) = path.rsplit_once(&marker).expect("a request's path");
                return (process.as_bytes().to_vec(), key.to_owned());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), waiting).await.expect("asked in time")
}

fn folder() -> (tempfile::TempDir, PathBuf) {
    let folder = tempfile::tempdir().expect("a folder");
    let path = folder.path().canonicalize().expect("its real path");
    (folder, path)
}

#[test]
fn the_tools_a_script_calls_are_named_by_the_functions_they_are() {
    let described = tools()
        .declared()
        .into_iter()
        .find(|declared| declared.name == "execute")
        .expect("execute")
        .description;
    assert!(described.contains("`tools` holds read, shell, write,"), "{described}");
    assert!(!described.contains("execute,"), "a script calls no script: {described}");
    assert_eq!(ExecuteTool::NAME, "execute");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_script_calls_its_tools_at_once_and_in_turn_working_on_what_they_gave() {
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak);
    let (_folder, workspace) = folder();
    let code = r"
        await Promise.all([1, 2, 3].map(n => tools.write({ path: `f${n}.txt`, content: `${n * 10}\n` })));
        let sum = 0;
        for (const n of [1, 2, 3]) {
            sum += Number(await tools.read({ path: `f${n}.txt` }));
        }
        console.log('read three');
        return sum;
    ";
    let path = executed(&runtime, workspace.clone(), code).await;
    let (status, said) = ended(&runtime, &path).await;
    assert_eq!((status, said.as_str()), (CallStatus::Ok, "read three\n60"));
    assert_eq!(std::fs::read_to_string(workspace.join("f2.txt")).expect("written"), "20\n");
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_that_waits_on_the_person_is_waited_for_and_one_they_decline_rejects() {
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak);
    let (_folder, workspace) = folder();
    let (_outside_folder, outside) = folder();
    let code = format!(
        r"
        const before = await tools.read({{ path: 'nonesuch.txt' }}).catch(error => 'none: ' + error.message.length);
        await tools.write({{ path: '{0}/a.txt', content: 'a' }});
        let declined;
        try {{ await tools.write({{ path: '{0}/b.txt', content: 'b' }}); }} catch (error) {{ declined = error.message; }}
        return [before.startsWith('none'), declined];
    ",
        outside.display()
    );
    let path = executed(&runtime, workspace, &code).await;
    for answer in [json!({ "decision": "allow" }), json!({ "decision": "deny", "reason": "not b" })] {
        let (process, key) = permission(&runtime, &path).await;
        runtime.answer(&process, key.as_bytes(), &answer).await.expect("answered");
    }
    let (status, said) = ended(&runtime, &path).await;
    assert_eq!(status, CallStatus::Ok);
    assert_eq!(said, r#"[true,"the person declined it: not b"]"#);
    assert!(outside.join("a.txt").exists() && !outside.join("b.txt").exists());
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn what_a_script_threw_is_what_it_gives() {
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak);
    let (_folder, workspace) = folder();
    let path = executed(&runtime, workspace, "console.log('began'); throw new TypeError('no such thing')").await;
    let (status, said) = ended(&runtime, &path).await;
    assert_eq!(
        (status, said.as_str()),
        (CallStatus::Ok, "began\n[the script threw: TypeError: no such thing]")
    );
    runtime.close().await;
}
