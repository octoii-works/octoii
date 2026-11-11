//! Siphon whole: the runtime on Beak with every program of a thread's and every handler of its requests, the tools its
//! model calls, the MCP servers they reach, and the terminals and the browser the person and the model share — started
//! as one on a volume, for a node to serve.

pub mod connection;

use std::path::PathBuf;
use std::sync::Arc;

use beak::Beak;
use octoii_ai::models::Models;
use siphon_agent::{
    CompactProgram, DelegateProgram, DelegateTool, DelegatedProgram, Gather, ModelRequest, ResumeProgram, Sources, ThreadProgram,
    TitleProgram, TurnProgram, answer_gather, answer_model, answer_review,
};
use siphon_browser::{Browser, BrowserHost};
use siphon_machine::Host;
use siphon_runtime::{Asked, Options, Runtime};
use siphon_terminals::Terminals;
use siphon_threads::Threads;
use siphon_tools::mcp::{Servers, Trust, TrustProgram};
use siphon_tools::{
    Asks, BrowserTool, EditTool, Exec, ExecuteTool, GlobTool, GrepTool, Permission, QuestionTool, ReadTool, Review, ScriptProgram,
    ShellTool, Tools, WriteTool, answer_exec,
};

/// What Siphon is started with: the machine its work is done on, the models its threads ask, the browser's program
/// and folders, and the volume's options.
#[derive(Debug)]
pub struct Setup {
    pub machine: Host,
    pub models: Arc<Models>,
    pub browser: BrowserSetup,
    pub options: Options,
}

/// The browser's program — one found on the machine, where none — the folder of its profile, and the folder what a
/// page downloads goes to — nothing downloaded, where none.
#[derive(Debug, Clone, Default)]
pub struct BrowserSetup {
    pub program: Option<PathBuf>,
    pub profile: Option<PathBuf>,
    pub downloads: Option<PathBuf>,
}

/// Siphon, started: its runtime, its threads, the models they ask, the MCP servers, the terminals and the browser.
#[derive(Debug, Clone)]
pub struct Siphon {
    pub runtime: Runtime,
    pub threads: Threads<ThreadProgram>,
    pub models: Arc<Models>,
    pub servers: Servers,
    pub terminals: Terminals,
    pub browser: Browser,
}

impl Siphon {
    /// Siphon on `beak`, as `setup` says, its tasks on the current Tokio runtime.
    ///
    /// # Errors
    /// The volume's failure, as another holding its directory.
    pub fn start(beak: &Beak, setup: Setup) -> beak::Result<Self> {
        let Setup {
            machine,
            models,
            browser,
            options,
        } = setup;
        let opened = options.open(beak)?;
        let kernel = opened.0.clone();
        let terminals = Terminals::start(kernel.clone(), machine.clone());
        let browser = Browser::start(BrowserHost {
            kernel,
            machine: machine.clone(),
            program: browser.program,
            profile: browser.profile,
            downloads: browser.downloads,
        });
        let servers = Servers::start(machine.clone(), machine.home.as_deref());
        let (inner, tools) = tools(&servers, &browser);
        let runtime = programs(&machine, &models, &servers, inner, tools).start_on(opened, options)?;
        Ok(Self {
            threads: Threads::new(runtime.clone()),
            runtime,
            models,
            servers,
            terminals,
            browser,
        })
    }
}

/// The tools a script calls, and those a turn's model calls: a script's, and running a script, asking the person and
/// delegating to a sub-thread besides.
fn tools(servers: &Servers, browser: &Browser) -> (Tools, Tools) {
    let inner = Tools::new()
        .with(ReadTool)
        .with(WriteTool)
        .with(EditTool)
        .with(GlobTool)
        .with(GrepTool)
        .with(ShellTool)
        .with(BrowserTool { browser: browser.clone() })
        .with_servers(servers.clone());
    let outer = inner
        .clone()
        .with(ExecuteTool { tools: inner.clone() })
        .with(QuestionTool)
        .with(DelegateTool);
    (inner, outer)
}

/// The runtime's programs and handlers: the thread's processes, the requests answered by the machine and the models,
/// and those the person answers.
fn programs(machine: &Host, models: &Arc<Models>, servers: &Servers, inner: Tools, tools: Tools) -> siphon_runtime::Builder {
    let sources = Sources {
        tools: tools.clone(),
        servers: servers.clone(),
        models: Arc::clone(models),
        host: machine.clone(),
        home: machine.home.clone(),
    };
    let (gathering, asking, reviewing) = (sources, Arc::clone(models), Arc::clone(models));
    let (execs, host) = (tools.clone(), machine.clone());
    Runtime::builder()
        .program(ThreadProgram)
        .program(TurnProgram {
            tools,
            home: machine.home.clone(),
            models: Arc::clone(models),
        })
        .program(ScriptProgram { tools: inner })
        .program(TrustProgram)
        .program(TitleProgram)
        .program(CompactProgram)
        .program(DelegateProgram)
        .program(DelegatedProgram)
        .program(ResumeProgram)
        .handle(move |asked: Asked<Gather>| {
            let sources = gathering.clone();
            async move { answer_gather(&sources, asked).await }
        })
        .handle(move |asked: Asked<ModelRequest>| {
            let models = Arc::clone(&asking);
            async move { answer_model(&models, asked).await }
        })
        .handle(move |asked: Asked<Review>| {
            let models = Arc::clone(&reviewing);
            async move { answer_review(&models, asked).await }
        })
        .handle(move |asked: Asked<Exec>| {
            let (tools, host) = (execs.clone(), host.clone());
            async move { answer_exec(&tools, &host, asked).await }
        })
        .ask_person::<Permission>()
        .ask_person::<Asks>()
        .ask_person::<Trust>()
}
