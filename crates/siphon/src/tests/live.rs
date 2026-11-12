//! The agent on a real model, run only when asked — `cargo test -p siphon --lib tests::live -- --ignored` — with the
//! key in the environment: OpenRouter's DeepSeek V4.1 Flash, as the cheapest model that reasons and calls tools.

use std::sync::Arc;
use std::time::Duration;

use crate::agent::{CompactProgram, Gather, ModelRequest, Sources, ThreadProgram, TitleProgram, TurnProgram, answer_gather, answer_model};
use crate::machine::Host as Machine;
use crate::threads::{Config, Mode, NewThread, Threads, Turn};
use crate::tools::{EditTool, Exec, ExecuteTool, GlobTool, GrepTool, ReadTool, ScriptProgram, ShellTool, Tools, WriteTool, answer_exec};
use beak::Beak;
use bytes::Bytes;
use octoii_ai::catalogue;
use octoii_ai::drivers::OPENROUTER;
use octoii_ai::host::{Credential, Credentials, Files, NoSuchFile, Unavailable};
use octoii_ai::models::{Host, Models, Provider};
use octoii_ai::vocabulary::InputContent;
use siphon_runtime::{Asked, Options, Runtime};

/// The key in the environment, as the host's credentials.
struct Environment;

impl Credentials for Environment {
    fn read(&self, _: &str) -> Result<Option<Credential>, Unavailable> {
        Ok(Some(Credential::ApiKey {
            api_key: std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY is set"),
        }))
    }

    fn keep(&self, _: &str, _: &Credential) -> Result<(), Unavailable> {
        Ok(())
    }

    fn forget(&self, _: &str) -> Result<(), Unavailable> {
        Ok(())
    }
}

impl Files for Environment {
    fn read(&self, reference: &str) -> Result<Bytes, NoSuchFile> {
        Err(NoSuchFile(reference.into()))
    }
}

const MODEL: &str = "openrouter/deepseek/deepseek-v4.1-flash";

fn models() -> Arc<Models> {
    let mut model = catalogue::bundled()
        .into_iter()
        .find(|model| model.provider == "deepseek" && model.id.contains("flash"))
        .expect("the catalogue lists DeepSeek's flash model");
    "deepseek/deepseek-v4.1-flash".clone_into(&mut model.id);
    model.provider = "openrouter".into();
    let host = Host {
        credentials: Arc::new(Environment),
        transport: Arc::new(octoii_ai::transport::Network::default()),
        new_id: Arc::new(new_id),
        clock: Arc::new(|| 0),
    };
    Arc::new(Models::new(host, vec![Provider::of(std::sync::Arc::new(OPENROUTER))], vec![model]))
}

/// An id of the time now, in nanoseconds, as hex, for what the provider leaves without one.
fn new_id() -> String {
    format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

fn runtime(beak: &Beak, models: &Arc<Models>) -> Runtime {
    let inner = Tools::new()
        .with(ReadTool)
        .with(WriteTool)
        .with(EditTool)
        .with(GlobTool)
        .with(GrepTool)
        .with(ShellTool);
    let tools = inner.clone().with(ExecuteTool);
    let sources = Sources {
        tools: tools.clone(),
        tellers: Sources::ordered(
            crate::agent::sections::tellers(&Machine::default(), None, Vec::new())
                .into_iter()
                .map(|(shared, teller)| crate::agent::OwnedTeller {
                    extension: crate::agent::CORE.to_owned(),
                    shared,
                    teller,
                })
                .collect(),
        ),
        owners: std::collections::HashMap::new(),
        natives: Vec::new(),
        models: Arc::clone(models),
    };
    let execs = tools.clone();
    let answering = Arc::clone(models);
    Runtime::builder()
        .program(ThreadProgram)
        .program(TurnProgram {
            tools,
            machine: crate::machine::Host::default(),
            models: Arc::clone(models),
            writables: Vec::new(),
        })
        .program(ScriptProgram {
            tools: inner,
            primitives: std::sync::Arc::new(crate::agent::ThreadPrimitives),
        })
        .program(TitleProgram)
        .program(CompactProgram)
        .handle(move |asked: Asked<Gather>| {
            let sources = sources.clone();
            async move { answer_gather(&sources, asked).await }
        })
        .handle(move |asked: Asked<ModelRequest>| {
            let models = Arc::clone(&answering);
            async move { answer_model(&models, asked).await }
        })
        .handle(move |asked: Asked<Exec>| {
            let tools = execs.clone();
            async move { answer_exec(&tools, &Machine::default(), asked).await }
        })
        .start(beak, Options::default())
        .expect("started")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "asks a real model, with the key in the environment"]
async fn the_agent_reads_a_file_and_writes_what_it_found() {
    let beak = Beak::new().expect("Beak");
    let models = models();
    let runtime = runtime(&beak, &models);
    let threads = Threads::<ThreadProgram>::new(runtime.clone());
    let folder = tempfile::tempdir().expect("a folder");
    let workspace = folder.path().canonicalize().expect("its real path");
    std::fs::write(workspace.join("notes.txt"), "The answer is 21.\n").expect("written");
    let new = NewThread {
        actor: "person".into(),
        config: Config {
            model: Some(MODEL.into()),
            workspace: Some(workspace.to_string_lossy().into_owned()),
            mode: Some(Mode::FullAccess),
            ..Config::default()
        },
        ..NewThread::default()
    };
    let id = threads.create(&new, "k").await.expect("made");
    let asked = "Read notes.txt, then write the number it names doubled, alone, into result.txt. Then say done.";
    threads
        .deliver(&id, "person", vec![InputContent::text(asked)], "d1")
        .await
        .expect("delivered");
    let mut committed = runtime.committed();
    let ended = async {
        loop {
            let turn = threads.listed(&id).expect("listed").status.turn;
            if matches!(turn, Turn::Replied | Turn::Failed | Turn::Stopped) {
                return turn;
            }
            committed.changed().await.expect("the runtime runs");
        }
    };
    let turn = tokio::time::timeout(Duration::from_mins(3), ended).await.expect("ended in time");
    let snapshot = runtime.snapshot().expect("a snapshot");
    for line in crate::threads::log::read_after(&snapshot, &id, None, u64::MAX).expect("read").lines {
        eprintln!("{}", String::from_utf8_lossy(&line).chars().take(400).collect::<String>());
    }
    let titled = async {
        while threads.listed(&id).expect("listed").config.title.is_none() {
            committed.changed().await.expect("the runtime runs");
        }
    };
    let titled = tokio::time::timeout(Duration::from_mins(1), titled).await.is_ok();
    let titling = beak::layout::process(&crate::threads::thread::main(&id), TitleProgram::KEY.as_bytes());
    eprintln!(
        "the title process: {:?}",
        runtime
            .snapshot()
            .expect("a snapshot")
            .record(&titling)
            .expect("read")
            .map(|record| record.outcome)
    );
    let listed = threads.listed(&id).expect("listed");
    eprintln!("title: {:?}; usage: {:?}", listed.config.title, listed.status.usage);
    assert_eq!(turn, Turn::Replied);
    assert!(titled, "titled");
    assert_eq!(std::fs::read_to_string(workspace.join("result.txt")).expect("written").trim(), "42");
    runtime.close().await;
}
