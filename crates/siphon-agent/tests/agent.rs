//! The agent on a scripted model: a thread's turn asks, takes on the calls its replies make, appends each result and
//! asks again till a reply calls nothing; asks again after a failure that may pass; and a stop ends it with every call
//! it made answered.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use beak::Beak;
use bytes::Bytes;
use futures::future::BoxFuture;
use octoii_ai::catalogue::{Features, InputLimits, Model};
use octoii_ai::driver::{Call, Connection, Driver, Settings};
use octoii_ai::host::{Credential, Credentials, Files, NoSuchFile, Unavailable};
use octoii_ai::models::{Host, Models, Provider};
use octoii_ai::stream::{Event, Source};
use octoii_ai::vocabulary::{
    Failure, FailureKind, FunctionCall, InputContent, Item, Message, OutputContent, Request, Response, ResponseStatus, Tool, Usage,
};
use siphon_agent::{
    DelegateProgram, DelegateTool, DelegatedProgram, Gather, ModelRequest, Sources, ThreadProgram, TitleProgram, TurnProgram,
    answer_gather, answer_model, answer_review,
};
use siphon_machine::Host as Machine;
use siphon_runtime::{Asked, Options, Runtime};
use siphon_threads::{CallStatus, Config, Entry, Mode, NewThread, Outcome, Threads, Turn};
use siphon_tools::{Asks, Exec, ExecuteTool, Permission, ReadTool, Review, ScriptProgram, ShellTool, Tools, WriteTool, answer_exec};

/// What the scripted provider answers each request with, in turn — but a request with an input of words it keeps a
/// reply for, which it answers with that, once — and the requests it was sent.
#[derive(Default)]
struct Script {
    replies: Mutex<VecDeque<Result<Vec<Item>, Failure>>>,
    for_input: Mutex<Vec<(String, Vec<Item>)>>,
    asked: Mutex<Vec<Request>>,
}

impl Script {
    /// The reply it keeps for an input of `request`'s, taken.
    fn for_input(&self, request: &Request) -> Option<Vec<Item>> {
        let mut kept = self.for_input.lock().expect("held");
        let at = kept.iter().position(|(words, _)| {
            request
                .input
                .iter()
                .any(|item| matches!(item, Item::Message(Message::User { content }) if content == &[InputContent::text(words.clone())]))
        })?;
        Some(kept.remove(at).1)
    }
}

/// A provider that answers as its script says.
struct Scripted(Arc<Script>);

impl Driver for Scripted {
    fn id(&self) -> &'static str {
        "test"
    }

    fn name(&self) -> &'static str {
        "Test"
    }

    fn connection(&self) -> Connection {
        Connection::Server
    }

    fn stream<'a>(&'a self, _: &'a Call, model: &'a Model, request: &'a Request) -> BoxFuture<'a, Result<Source, Failure>> {
        Box::pin(async move {
            let script = &self.0;
            let output = if asks(request, siphon_agent::title::INSTRUCTION) {
                vec![says("Writing a file.")]
            } else if asks(request, siphon_agent::compaction::RECAP) {
                vec![says("the recap")]
            } else if request.format.is_some() {
                let told = format!("{:?}", request.input);
                let removes = told.rsplit("<call>").next().is_some_and(|call| call.contains("rm -rf"));
                vec![says(
                    &serde_json::json!({ "allow": !removes, "reason": if removes { "it removes files" } else { "" } }).to_string(),
                )]
            } else if let Some(output) = script.for_input(request) {
                output
            } else {
                script.asked.lock().expect("held").push(request.clone());
                let next = script.replies.lock().expect("held").pop_front();
                match next {
                    Some(Ok(output)) => output,
                    Some(Err(failure)) => return Err(failure),
                    None => futures::future::pending().await,
                }
            };
            let mut events: Vec<Result<Event, Failure>> = Vec::new();
            for (index, item) in output.iter().enumerate() {
                let index = u32::try_from(index).expect("few items");
                events.push(Ok(Event::Began { index, item: item.clone() }));
                events.push(Ok(Event::Ended { index, item: item.clone() }));
            }
            events.push(Ok(Event::Finished(Response {
                id: "r".into(),
                model: model.reference(),
                status: ResponseStatus::Completed,
                output,
                usage: Some(Usage {
                    input_tokens: 10,
                    cached_tokens: 0,
                    cache_write_tokens: 0,
                    output_tokens: 5,
                    reasoning_tokens: 0,
                    web_searches: 0,
                    cost_nanodollars: Some(100),
                }),
                failure: None,
                incomplete: None,
                context_tokens: None,
            })));
            let source: Source = Box::pin(futures::stream::iter(events));
            Ok(source)
        })
    }
}

/// Whether `request` asks `instruction` after the window.
fn asks(request: &Request, instruction: &str) -> bool {
    matches!(request.input.last(), Some(Item::Message(Message::User { content })) if content == &[InputContent::text(instruction)])
}

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

impl Files for Nothing {
    fn read(&self, reference: &str) -> Result<Bytes, NoSuchFile> {
        Err(NoSuchFile(reference.into()))
    }
}

fn models(script: &Arc<Script>) -> Arc<Models> {
    let host = Host {
        credentials: Arc::new(Nothing),
        transport: Arc::new(octoii_ai::transport::Network::default()),
        new_id: Arc::new(|| "minted".into()),
        clock: Arc::new(|| 0),
    };
    let provider = Provider {
        id: "test".into(),
        name: "Test".into(),
        driver: Arc::new(Scripted(Arc::clone(script))),
        settings: Settings::default(),
        catalogue: "test".into(),
    };
    let model = Model {
        id: "m".into(),
        provider: "test".into(),
        name: "m".into(),
        context_length: Some(30_000),
        max_output_tokens: None,
        reads_images: true,
        reads_files: true,
        reasoning: None,
        parameters: None,
        pricing: None,
        release_date: None,
        searches_web: false,
        limits: InputLimits::default(),
        features: Features::default(),
    };
    Arc::new(Models::new(host, vec![provider], vec![model]))
}

fn runtime(beak: &Beak, script: &Arc<Script>) -> Runtime {
    let inner = Tools::new().with(ReadTool).with(WriteTool).with(ShellTool);
    let tools = inner.clone().with(ExecuteTool { tools: inner.clone() }).with(DelegateTool);
    let models = models(script);
    let sources = Sources {
        tools: tools.clone(),
        tellers: Vec::new(),
        models: Arc::clone(&models),
        host: Machine::default(),
        config: None,
    };
    let execs = tools.clone();
    Runtime::builder()
        .program(ThreadProgram)
        .program(TurnProgram {
            tools,
            home: None,
            models: Arc::clone(&models),
        })
        .program(ScriptProgram { tools: inner })
        .program(TitleProgram)
        .program(siphon_agent::CompactProgram)
        .program(DelegateProgram)
        .program(DelegatedProgram)
        .program(siphon_agent::ResumeProgram)
        .handle(move |asked: Asked<Gather>| {
            let sources = sources.clone();
            async move { answer_gather(&sources, asked).await }
        })
        .handle({
            let models = Arc::clone(&models);
            move |asked: Asked<ModelRequest>| {
                let models = Arc::clone(&models);
                async move { answer_model(&models, asked).await }
            }
        })
        .handle(move |asked: Asked<Review>| {
            let models = Arc::clone(&models);
            async move { answer_review(&models, asked).await }
        })
        .handle(move |asked: Asked<Exec>| {
            let tools = execs.clone();
            async move { answer_exec(&tools, &Machine::default(), asked).await }
        })
        .ask_person::<Permission>()
        .ask_person::<Asks>()
        .start(beak, Options::default())
        .expect("started")
}

fn script(replies: Vec<Result<Vec<Item>, Failure>>) -> Arc<Script> {
    Arc::new(Script {
        replies: Mutex::new(replies.into()),
        ..Script::default()
    })
}

fn says(text: &str) -> Item {
    Item::Message(Message::Assistant {
        id: None,
        content: vec![OutputContent::OutputText {
            text: text.into(),
            annotations: Vec::new(),
        }],
        status: None,
        origin: None,
    })
}

fn calls(call_id: &str, name: &str, arguments: &serde_json::Value) -> Item {
    Item::FunctionCall(FunctionCall {
        id: None,
        call_id: call_id.into(),
        name: name.into(),
        arguments: arguments.to_string(),
        status: None,
        origin: None,
    })
}

async fn thread(threads: &Threads<ThreadProgram>, workspace: &std::path::Path) -> String {
    made(threads, workspace, Mode::FullAccess).await
}

async fn made(threads: &Threads<ThreadProgram>, workspace: &std::path::Path, mode: Mode) -> String {
    let new = NewThread {
        actor: "person".into(),
        config: Config {
            model: Some("test/m".into()),
            workspace: Some(workspace.to_string_lossy().into_owned()),
            mode: Some(mode),
            ..Config::default()
        },
        ..NewThread::default()
    };
    let key = threads.runtime().new_id().expect("an id");
    threads.create(&new, &key).await.expect("made")
}

/// Waits till the thread `id`'s turn stands as `turn`.
async fn until(threads: &Threads<ThreadProgram>, id: &str, turn: Turn) {
    let mut committed = threads.runtime().committed();
    let waiting = async {
        while threads.listed(id).expect("listed").status.turn != turn {
            committed.changed().await.expect("the runtime runs");
        }
    };
    tokio::time::timeout(Duration::from_secs(20), waiting).await.expect("in time");
}

/// The entries of the thread `id`'s log, in its order.
fn entries(threads: &Threads<ThreadProgram>, id: &str) -> Vec<Entry> {
    let snapshot = threads.runtime().snapshot().expect("a snapshot");
    let read = siphon_threads::log::read_after(&snapshot, id, None, u64::MAX).expect("read");
    read.lines
        .iter()
        .map(|line| siphon_threads::log::entry_of(line).expect("an entry"))
        .collect()
}

fn folder() -> (tempfile::TempDir, PathBuf) {
    let folder = tempfile::tempdir().expect("a folder");
    let path = folder.path().canonicalize().expect("its real path");
    (folder, path)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_takes_on_the_calls_its_replies_make_and_asks_again_till_one_calls_nothing() {
    let script = script(vec![
        Ok(vec![calls(
            "c1",
            "write",
            &serde_json::json!({ "path": "a.txt", "content": "hello\n" }),
        )]),
        Ok(vec![says("written")]),
    ]);
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak, &script);
    let threads = Threads::<ThreadProgram>::new(runtime.clone());
    let (_folder, workspace) = folder();
    let id = thread(&threads, &workspace).await;
    threads
        .deliver(&id, "person", vec![InputContent::text("write a.txt")], "d1")
        .await
        .expect("delivered");
    until(&threads, &id, Turn::Replied).await;
    assert_eq!(std::fs::read_to_string(workspace.join("a.txt")).expect("written"), "hello\n");
    let mut titled = threads.runtime().committed();
    tokio::time::timeout(Duration::from_secs(10), async {
        while threads.listed(&id).expect("listed").config.title.is_none() {
            titled.changed().await.expect("the runtime runs");
        }
    })
    .await
    .expect("titled in time");
    assert_eq!(threads.listed(&id).expect("listed").config.title.as_deref(), Some("Writing a file"));
    let kinds: Vec<&str> = entries(&threads, &id)
        .iter()
        .filter(|entry| !matches!(entry, Entry::Config { actor, .. } if actor == siphon_agent::title::ACTOR))
        .map(|entry| match entry {
            Entry::Config { .. } => "config",
            Entry::System { .. } => "system",
            Entry::Input { .. } => "input",
            Entry::Reply { .. } => "reply",
            Entry::Result { .. } => "result",
            Entry::TurnEnd { .. } => "turn-end",
            Entry::Compaction { .. } => "compaction",
            Entry::Discarded { .. } => "discarded",
        })
        .collect();
    assert_eq!(kinds, ["config", "system", "input", "reply", "result", "reply", "turn-end"]);
    let asked = script.asked.lock().expect("held").clone();
    assert_eq!(asked.len(), 2);
    let names: Vec<&str> = asked[0]
        .tools
        .iter()
        .filter_map(|tool| match tool {
            Tool::Function { name, .. } => Some(name.as_str()),
            Tool::WebSearch => None,
        })
        .collect();
    assert_eq!(names, ["delegate", "execute", "read", "shell", "write"]);
    assert!(
        matches!(&asked[0].input[0], Item::Message(Message::Developer { content }) if content.len() == 2),
        "the base and the environment"
    );
    assert!(
        matches!(asked[1].input.last(), Some(Item::FunctionCallOutput(output)) if output.call_id == "c1" && !output.is_error),
        "{:?}",
        asked[1].input.last()
    );
    assert_eq!(asked[1].prompt_cache_key.as_deref(), Some(id.as_str()));
    let usage = threads.listed(&id).expect("listed").status.usage.expect("used");
    assert_eq!(
        (usage.input_tokens, usage.output_tokens, usage.cost_nanodollars),
        (30, 15, 300),
        "the turn's two requests and the title's"
    );
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failure_that_may_pass_is_kept_and_the_model_asked_again() {
    let mut failure = Failure::new(FailureKind::Server, "overloaded");
    failure.retry_after_ms = Some(10);
    let script = script(vec![Err(failure), Ok(vec![says("there")])]);
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak, &script);
    let threads = Threads::<ThreadProgram>::new(runtime.clone());
    let (_folder, workspace) = folder();
    let id = thread(&threads, &workspace).await;
    threads
        .deliver(&id, "person", vec![InputContent::text("hi")], "d1")
        .await
        .expect("delivered");
    until(&threads, &id, Turn::Replied).await;
    let all = entries(&threads, &id);
    assert!(
        matches!(&all[3], Entry::Discarded { response, .. } if response.failure.as_ref().is_some_and(|failure| failure.message == "overloaded"))
    );
    assert!(matches!(&all[4], Entry::Reply { .. }));
    assert_eq!(script.asked.lock().expect("held").len(), 2);
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stop_ends_the_turn_with_every_call_it_made_answered() {
    let script = script(vec![Ok(vec![
        calls("c1", "shell", &serde_json::json!({ "command": "sleep 30" })),
        calls("c2", "write", &serde_json::json!({ "path": "never.txt", "content": "x" })),
    ])]);
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak, &script);
    let threads = Threads::<ThreadProgram>::new(runtime.clone());
    let (_folder, workspace) = folder();
    let id = thread(&threads, &workspace).await;
    threads
        .deliver(&id, "person", vec![InputContent::text("wait")], "d1")
        .await
        .expect("delivered");
    let mut committed = runtime.committed();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !entries(&threads, &id).iter().any(|entry| matches!(entry, Entry::Reply { .. })) {
            committed.changed().await.expect("the runtime runs");
        }
    })
    .await
    .expect("replied in time");
    tokio::time::sleep(Duration::from_millis(200)).await;
    threads.stop(&id, "person").await.expect("stopped");
    until(&threads, &id, Turn::Stopped).await;
    let all = entries(&threads, &id);
    let results: Vec<(String, CallStatus)> = all
        .iter()
        .filter_map(|entry| match entry {
            Entry::Result { output, status, .. } => Some((output.call_id.clone(), *status)),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 2, "{all:?}");
    assert_eq!(results[1], ("c2".into(), CallStatus::Interrupted));
    assert!(!workspace.join("never.txt").exists());
    assert!(matches!(
        all.last(),
        Some(Entry::TurnEnd {
            outcome: Outcome::Stopped,
            ..
        })
    ));
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn in_auto_a_call_that_would_ask_is_judged_by_the_model_the_environment_names() {
    let script = script(vec![
        Ok(vec![
            calls("c1", "shell", &serde_json::json!({ "command": "rm -rf build" })),
            calls("c2", "shell", &serde_json::json!({ "command": "echo built > out.txt" })),
        ]),
        Ok(vec![says("done")]),
    ]);
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak, &script);
    let change = siphon_settings::SettingsChange {
        environment: Some(siphon_settings::EnvironmentChange {
            review_model: Some(Some("test/m".into())),
            ..siphon_settings::EnvironmentChange::default()
        }),
        device: None,
    };
    siphon_settings::set(&runtime, "s", &change).await.expect("set");
    let threads = Threads::<ThreadProgram>::new(runtime.clone());
    let (_folder, workspace) = folder();
    std::fs::create_dir(workspace.join("build")).expect("a folder");
    let id = made(&threads, &workspace, Mode::Auto).await;
    threads
        .deliver(&id, "person", vec![InputContent::text("build")], "d1")
        .await
        .expect("delivered");
    until(&threads, &id, Turn::Replied).await;
    let results: Vec<(CallStatus, String)> = entries(&threads, &id)
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Result { status, output, .. } => Some((status, format!("{:?}", output.output))),
            _ => None,
        })
        .collect();
    assert_eq!(results[0].0, CallStatus::Rejected, "{results:?}");
    assert!(results[0].1.contains("blocked: it removes files"));
    assert_eq!(results[1].0, CallStatus::Ok, "{results:?}");
    assert!(workspace.join("build").exists() && workspace.join("out.txt").exists());
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_window_too_full_for_its_next_request_is_compacted_and_the_model_asked_on_the_new_one() {
    let script = script(vec![Ok(vec![says("hello")]), Ok(vec![says("read it")])]);
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak, &script);
    let threads = Threads::<ThreadProgram>::new(runtime.clone());
    let (_folder, workspace) = folder();
    let id = thread(&threads, &workspace).await;
    threads
        .deliver(&id, "person", vec![InputContent::text("hi")], "d1")
        .await
        .expect("delivered");
    until(&threads, &id, Turn::Replied).await;
    let long = "word ".repeat(22_000);
    threads
        .deliver(&id, "person", vec![InputContent::text(long.clone())], "d2")
        .await
        .expect("delivered");
    let mut committed = runtime.committed();
    tokio::time::timeout(Duration::from_secs(20), async {
        while entries(&threads, &id)
            .iter()
            .filter(|entry| matches!(entry, Entry::TurnEnd { .. }))
            .count()
            < 2
        {
            committed.changed().await.expect("the runtime runs");
        }
    })
    .await
    .expect("ended in time");
    let all = entries(&threads, &id);
    let compaction = all
        .iter()
        .position(|entry| matches!(entry, Entry::Compaction { .. }))
        .expect("compacted");
    let Entry::Compaction { kept, from, .. } = &all[compaction] else {
        unreachable!()
    };
    assert_eq!(*kept, None, "cut at the long input, which it keeps as it stands");
    assert!(
        matches!(&all[compaction + 1], Entry::System { tools, .. } if !tools.is_empty()),
        "the new window begins told anew"
    );
    let long_input = all
        .iter()
        .find(|entry| matches!(entry, Entry::Input { content, .. } if content[0] == InputContent::text(long.clone())));
    assert_eq!(Some(from.as_str()), long_input.map(Entry::id));
    let asked = script.asked.lock().expect("held").clone();
    let last = asked.last().expect("asked");
    assert!(
        matches!(&last.input[1], Item::Message(Message::Developer { content }) if content == &[InputContent::text("<recap>\nthe recap\n</recap>")])
    );
    assert!(matches!(&last.input[2], Item::Message(Message::User { content }) if content[0] == InputContent::text(long)));
    assert_eq!(last.input.len(), 3, "nothing before the cut is sent");
    runtime.close().await;
}

/// The threads made, but `except`, by their ids.
fn others(threads: &Threads<ThreadProgram>, except: &str) -> Vec<siphon_threads::Listed> {
    threads
        .list()
        .expect("listed")
        .into_iter()
        .filter(|listed| listed.id != except)
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_delegated_is_worked_on_by_a_sub_thread_and_what_it_said_last_given_to_the_call() {
    let script = script(vec![
        Ok(vec![calls(
            "c1",
            "delegate",
            &serde_json::json!({ "task": "say done", "title": "Helper" }),
        )]),
        Ok(vec![says("all done")]),
    ]);
    script
        .for_input
        .lock()
        .expect("held")
        .push(("say done".into(), vec![says("the child is done")]));
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak, &script);
    let threads = Threads::<ThreadProgram>::new(runtime.clone());
    let (_folder, workspace) = folder();
    let id = thread(&threads, &workspace).await;
    threads
        .deliver(&id, "person", vec![InputContent::text("delegate it")], "d1")
        .await
        .expect("delivered");
    until(&threads, &id, Turn::Replied).await;
    let children = others(&threads, &id);
    assert_eq!(children.len(), 1);
    let child = &children[0];
    assert_eq!(child.parent.as_ref().map(|parent| parent.thread.as_str()), Some(id.as_str()));
    assert_eq!(child.config.title.as_deref(), Some("Helper"));
    // The reply that made the call keys it as it is written: the sub-thread names its call by that key, as its result does.
    let keyed = entries(&threads, &id).into_iter().find_map(|entry| match entry {
        Entry::Reply { calls, .. } => calls.into_iter().find(|call| call.call_id == "c1"),
        _ => None,
    });
    let key = keyed.expect("the call keyed by its reply").key;
    assert_eq!(child.parent.as_ref().map(|parent| parent.call.as_str()), Some(key.as_str()));
    let answered = entries(&threads, &id).into_iter().find_map(|entry| match entry {
        Entry::Result { call, .. } => Some(call),
        _ => None,
    });
    assert_eq!(answered.as_deref(), Some(key.as_str()));
    let result = entries(&threads, &id).into_iter().find_map(|entry| match entry {
        Entry::Result { status, output, .. } => Some((status, output.output)),
        _ => None,
    });
    let (status, output) = result.expect("a result");
    assert_eq!(status, CallStatus::Ok);
    let InputContent::InputText { text } = &output[0] else {
        panic!("text")
    };
    assert!(text.starts_with("the child is done\n[the sub-thread "), "{text}");
    assert!(text.contains(&child.id) && text.contains("finished"), "{text}");
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_delegated_in_the_background_is_answered_at_once_and_its_end_delivered_to_the_thread() {
    let script = script(vec![
        Ok(vec![calls(
            "c1",
            "delegate",
            &serde_json::json!({ "task": "say done", "background": true }),
        )]),
        Ok(vec![says("started it")]),
        Ok(vec![says("noted")]),
    ]);
    script
        .for_input
        .lock()
        .expect("held")
        .push(("say done".into(), vec![says("the child is done")]));
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak, &script);
    let threads = Threads::<ThreadProgram>::new(runtime.clone());
    let (_folder, workspace) = folder();
    let id = thread(&threads, &workspace).await;
    threads
        .deliver(&id, "person", vec![InputContent::text("delegate it")], "d1")
        .await
        .expect("delivered");
    let mut committed = runtime.committed();
    tokio::time::timeout(Duration::from_secs(20), async {
        while entries(&threads, &id)
            .iter()
            .filter(|entry| matches!(entry, Entry::TurnEnd { .. }))
            .count()
            < 2
        {
            committed.changed().await.expect("the runtime runs");
        }
    })
    .await
    .expect("two turns in time");
    let all = entries(&threads, &id);
    let child = others(&threads, &id).pop().expect("a sub-thread");
    let notice = all.iter().find_map(|entry| match entry {
        Entry::Input { actor, content, .. } if *actor == child.id => Some(content.clone()),
        _ => None,
    });
    let notice = notice.expect("its end delivered");
    assert!(matches!(&notice[0], InputContent::InputText { text } if text.contains("which you gave a task in the background, finished")));
    assert_eq!(notice[1], InputContent::text("the child is done"));
    let first = all.iter().find_map(|entry| match entry {
        Entry::Result { output, .. } => Some(format!("{:?}", output.output)),
        _ => None,
    });
    assert!(first.expect("a result").contains("works on it in the background"));
    runtime.close().await;
}

/// Arms the resume of the thread `id` to go on `after` from now.
async fn arm(runtime: &Runtime, id: &str, after: i64) {
    let snapshot = runtime.snapshot().expect("a snapshot");
    let input = siphon_agent::ResumeInput::now(&snapshot, id, runtime.now_ms() + after, "person").expect("read");
    let mut changes = siphon_runtime::Changes::new();
    siphon_agent::resume::arm(&snapshot, &mut changes, &input).expect("armed");
    runtime.commit(changes).await.expect("committed");
}

fn words_given(threads: &Threads<ThreadProgram>, id: &str) -> Vec<String> {
    entries(threads, id)
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Input { content, .. } => match &content[0] {
                InputContent::InputText { text } => Some(text.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_resume_goes_on_at_its_time_unless_the_person_went_on_first() {
    let script = script(vec![Ok(vec![says("going on")]), Ok(vec![says("as you said")])]);
    let beak = Beak::new().expect("Beak");
    let runtime = runtime(&beak, &script);
    let threads = Threads::<ThreadProgram>::new(runtime.clone());
    let (_folder, workspace) = folder();
    let id = thread(&threads, &workspace).await;
    arm(&runtime, &id, 300).await;
    until(&threads, &id, Turn::Replied).await;
    assert_eq!(words_given(&threads, &id), [siphon_agent::resume::WORD]);

    let other = made(&threads, &workspace, Mode::FullAccess).await;
    arm(&runtime, &other, 60_000).await;
    threads
        .deliver(&other, "person", vec![InputContent::text("myself")], "d1")
        .await
        .expect("delivered");
    until(&threads, &other, Turn::Replied).await;
    let resume = beak::layout::process(&siphon_threads::thread::main(&other), siphon_agent::ResumeProgram::KEY.as_bytes());
    let mut committed = runtime.committed();
    tokio::time::timeout(Duration::from_secs(10), async {
        while runtime
            .snapshot()
            .expect("a snapshot")
            .record(&resume)
            .expect("read")
            .and_then(|record| record.outcome)
            .is_none()
        {
            committed.changed().await.expect("the runtime runs");
        }
    })
    .await
    .expect("ended in time");
    assert_eq!(words_given(&threads, &other), ["myself"]);
    runtime.close().await;
}
