//! Claude through a Claude Code of the tests' own, which speaks `claude`'s stream-json control protocol: the models it
//! offers listed with their windows; a conversation answered — words, its session closed as its turn ended and resumed
//! in place for the next, then a call of the thread's tool held across requests till its result comes — resumed in
//! place where the node started again, forked where the
//! conversation parted from the session behind its tip, and let go of as its thread is deleted; a computer without
//! `claude` told how to install it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::StreamExt as _;
use octoii_ai::catalogue::Model;
use octoii_ai::driver::{Call, Driver, Settings};
use octoii_ai::host::{Credential, Credentials, Unavailable};
use octoii_ai::stream::Event;
use octoii_ai::vocabulary::{FailureKind, FunctionCallOutput, InputContent, Item, Message, OutputContent, Request, Response, Tool};
use serde_json::{Value, json};
use siphon_claude_code::{ClaudeCode, Place};

struct Nothing;

impl Credentials for Nothing {
    fn read(&self, _: &str) -> Result<Option<Credential>, Unavailable> {
        Ok(None)
    }

    fn keep(&self, _: &str, _: &Credential) -> Result<(), Unavailable> {
        Ok(())
    }

    fn forget(&self, _: &str) -> Result<(), Unavailable> {
        Ok(())
    }
}

/// The account the tests answer through: one the person added, whose configuration is kept in its own folder.
const PROVIDER: &str = "added";

fn call() -> Call {
    Call {
        provider: PROVIDER.into(),
        settings: Settings::default(),
        credentials: Arc::new(Nothing),
        transport: Arc::new(octoii_ai::transport::Network::default()),
        files: None,
        now_ms: 1_000,
    }
}

/// Claude Code on `kernel` with its files under `state`, its `claude` the tests' own, which logs each start there.
fn claude_code(kernel: &beak::Kernel, state: &Path, program: Option<PathBuf>) -> ClaudeCode {
    let place = Place {
        kernel: kernel.clone(),
        machine: siphon_machine::Host {
            home: Some(state.join("home")),
            ..siphon_machine::Host::default()
        },
        program,
        folder: state.to_owned(),
    };
    std::fs::create_dir_all(place.work()).expect("a work folder");
    ClaudeCode::new(place)
}

fn fake() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fake-claude.py")
}

fn user(text: &str) -> Item {
    Item::Message(Message::User {
        content: vec![InputContent::text(text)],
    })
}

fn request(input: Vec<Item>) -> Request {
    Request {
        model: format!("{PROVIDER}/fake"),
        instructions: Some("Be brief.".into()),
        input,
        tools: vec![Tool::Function {
            name: "echo".into(),
            description: "Echoes its text.".into(),
            parameters: json!({ "type": "object", "properties": { "text": { "type": "string" } } }),
        }],
        tool_choice: None,
        reasoning: None,
        format: None,
        max_output_tokens: None,
        prompt_cache_key: Some("thread-1".into()),
    }
}

/// The response `claude` answers `request` with, its output the items it ended.
async fn answered(claude: &ClaudeCode, model: &Model, request: &Request) -> Response {
    let call = call();
    let mut source = claude.stream(&call, model, request).await.expect("a stream");
    let mut output = Vec::new();
    while let Some(event) = source.next().await {
        match event.expect("no failure") {
            Event::Ended { item, .. } => output.push(item),
            Event::Finished(response) => return Response { output, ..response },
            _ => {}
        }
    }
    panic!("the stream ended unfinished");
}

/// The words of a response's messages.
fn words(response: &Response) -> String {
    response
        .output
        .iter()
        .filter_map(|item| match item {
            Item::Message(Message::Assistant { content, .. }) => Some(content),
            _ => None,
        })
        .flatten()
        .filter_map(|part| match part {
            OutputContent::OutputText { text, .. } => Some(text.as_str()),
            OutputContent::Refusal { .. } => None,
        })
        .collect()
}

/// Each start's arguments, as the tests' `claude` logged them.
fn starts(log: &Path) -> Vec<Vec<String>> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("JSON"))
        .map(|value| {
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|each| each.as_str().map(str::to_owned))
                .collect()
        })
        .collect()
}

/// The sessions the account's Claude Code keeps.
fn sessions(state: &Path) -> usize {
    let projects = state.join(PROVIDER).join("claude").join("projects");
    std::fs::read_dir(projects)
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|folder| std::fs::read_dir(folder.path()).into_iter().flatten().flatten())
        .filter(|file| file.path().extension().is_some_and(|extension| extension == "jsonl"))
        .count()
}

/// Waits till `holds` is true, as a session settles after its answer's stream ended; at most two seconds.
async fn settled(holds: impl Fn() -> bool) -> bool {
    for _ in 0..200 {
        if holds() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    false
}

fn flag<'a>(start: &'a [String], name: &str) -> Option<&'a str> {
    start.iter().find_map(|argument| argument.strip_prefix(&format!("--{name}=")))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_is_answered_in_a_live_session_resumed_in_place_forked_behind_its_tip_and_let_go_of() {
    let folder = tempfile::tempdir().expect("a folder");
    let state = folder.path().canonicalize().expect("its real path");
    let log = state.join("starts.jsonl");
    let beak = beak::Beak::new().expect("Beak");
    let kernel = beak::Kernel::open(&beak, &beak::Options::default()).expect("a kernel").0;
    let claude = claude_code(&kernel, &state, Some(fake()));

    let listed = claude.list(&call()).await.expect("listed").expect("a listing");
    assert_eq!(listed.len(), 1);
    let model = listed[0].clone();
    assert_eq!(
        (model.id.as_str(), model.name.as_str(), model.context_length),
        ("fake", "Fake", Some(200_000))
    );
    let efforts = model.reasoning.as_ref().and_then(|reasoning| reasoning.efforts.clone());
    assert_eq!(efforts.map(|efforts| efforts.len()), Some(2), "the efforts it takes, highest first");

    let mut input = vec![user("hello")];
    let first = answered(&claude, &model, &request(input.clone())).await;
    assert_eq!(words(&first), "heard: hello");
    input.extend(first.output.clone());
    input.push(user("call echo hi"));
    let calling = answered(&claude, &model, &request(input.clone())).await;
    let Some(Item::FunctionCall(made)) = calling.output.iter().find(|item| matches!(item, Item::FunctionCall(_))) else {
        panic!("a call of the thread's tool: {calling:?}");
    };
    assert_eq!((made.name.as_str(), made.arguments.as_str()), ("echo", "{\"text\": \"hi\"}"));
    input.extend(calling.output.clone());
    input.push(Item::FunctionCallOutput(FunctionCallOutput {
        call_id: made.call_id.clone(),
        output: vec![InputContent::text("echoed hi")],
        is_error: false,
    }));
    let done = answered(&claude, &model, &request(input.clone())).await;
    assert_eq!(words(&done), "done: echoed hi", "the held call given its result");
    let started = starts(&log);
    assert_eq!(
        started.len(),
        3,
        "the listing's; a session closed as its turn ended, resumed for the next; live while it held its call: {started:?}"
    );
    assert_eq!(flag(&started[1], "resume"), None);
    assert!(flag(&started[2], "resume").is_some() && !started[2].contains(&"--fork-session".to_owned()));
    let holds = state.join(PROVIDER).join("holds.jsonl");
    let held = || std::fs::read_to_string(&holds).is_ok_and(|holds| holds.contains("thread-1"));
    assert!(settled(held).await, "the conversation holds its session");
    assert_eq!(sessions(&state), 1);

    // The node started again: the conversation goes on in the session where it ended, in place.
    let claude = claude_code(&kernel, &state, Some(fake()));
    input.extend(done.output.clone());
    let mut again = input.clone();
    again.push(user("again"));
    assert_eq!(words(&answered(&claude, &model, &request(again)).await), "heard: again");
    let started = starts(&log);
    let resumed = started.last().expect("a start");
    assert!(
        flag(resumed, "resume").is_some() && flag(resumed, "resume-session-at").is_some(),
        "{resumed:?}"
    );
    assert!(!resumed.contains(&"--fork-session".to_owned()), "at its tip, in place");

    // The conversation parted from the session behind its tip: forked there, the session it held let go of.
    let mut parted = vec![user("hello")];
    parted.extend(first.output);
    parted.push(user("branch"));
    assert_eq!(words(&answered(&claude, &model, &request(parted)).await), "heard: branch");
    let forked = starts(&log).last().cloned().expect("a start");
    assert!(forked.contains(&"--fork-session".to_owned()), "{forked:?}");
    assert!(settled(|| sessions(&state) == 1).await, "the session no conversation holds removed");

    claude.forget_conversation("thread-1");
    assert_eq!(sessions(&state), 0, "its thread deleted, the conversation's session goes");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_account_stands_as_its_claude_code_tells_and_goes_signed_out_with_its_files() {
    let folder = tempfile::tempdir().expect("a folder");
    let state = folder.path().canonicalize().expect("its real path");
    let beak = beak::Beak::new().expect("Beak");
    let kernel = beak::Kernel::open(&beak, &beak::Options::default()).expect("a kernel").0;
    let claude = claude_code(&kernel, &state, Some(fake()));
    let standing = claude.status(&call()).await.expect("told").expect("a standing");
    assert_eq!(
        (standing.account.as_deref(), standing.plan.as_deref(), standing.guidance),
        (Some("person@example.com"), Some("Claude Test"), None)
    );
    let limits: Vec<_> = standing
        .limits
        .iter()
        .map(|limit| (limit.window_minutes, limit.scope.as_deref(), limit.used_percent, limit.resets_at_ms))
        .collect();
    assert_eq!(
        limits,
        [
            (Some(10_080), None, Some(8.0), Some(1_794_927_600_000)),
            (Some(10_080), Some("Fable"), Some(0.0), None),
        ]
    );
    assert!(state.join(PROVIDER).is_dir());
    claude.forget(&call()).await.expect("forgotten");
    assert!(!state.join(PROVIDER).exists(), "the account's files go with it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_computer_without_claude_is_told_how_to_install_it_and_sign_in() {
    let folder = tempfile::tempdir().expect("a folder");
    let beak = beak::Beak::new().expect("Beak");
    let kernel = beak::Kernel::open(&beak, &beak::Options::default()).expect("a kernel").0;
    let claude = claude_code(&kernel, folder.path(), None);
    let guidance = claude.status(&call()).await.expect("told").and_then(|standing| standing.guidance);
    let commands = guidance.map(|guidance| guidance.commands).unwrap_or_default();
    assert!(commands.len() == 2 && commands[1].ends_with("claude auth login"), "{commands:?}");
    let failure = claude.list(&call()).await.expect_err("no listing");
    assert_eq!(failure.kind, FailureKind::Auth);
    assert!(
        failure.message.contains("install.sh") && failure.message.contains("claude auth login"),
        "{}",
        failure.message
    );
}
