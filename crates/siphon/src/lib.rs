//! Siphon whole: the runtime on Beak with the programs of a thread's and the handlers of its requests, the tools its
//! model calls and the MCP servers they reach, the models it asks, and the protocol a client speaks to it — built with
//! the plugins a host picks, which bring the rest: their providers, tools, programs and methods.

pub mod connection;
mod plugin;

use std::sync::Arc;

use beak::{Beak, Events};
use octoii_ai::catalogue::Model;
use octoii_ai::models::{Host as Network, Models, Provider};
use siphon_agent::{
    CompactProgram, DelegateProgram, DelegateTool, DelegatedProgram, Gather, ModelRequest, ResumeProgram, Sources, ThreadProgram,
    TitleProgram, TurnProgram, answer_gather, answer_model, answer_review,
};
use siphon_machine::Host;
use siphon_protocol::Scope;
use siphon_runtime::{Asked, Options, Runtime};
use siphon_threads::Threads;
use siphon_tools::mcp::{Servers, Trust, TrustProgram};
use siphon_tools::{
    Asks, EditTool, Exec, ExecuteTool, GlobTool, GrepTool, Permission, QuestionTool, ReadTool, Review, ScriptProgram, ShellTool, Tools,
    WriteTool, answer_exec,
};

pub use plugin::{Builder, Context, Full, Lagged, Plugin, Sink, Subscription};

/// What Siphon is built on: the machine its work is done on; the host's network, credentials and files the models are
/// reached with, the host's providers and the catalogue of models; and the volume's options.
#[derive(Debug)]
pub struct Setup {
    pub machine: Host,
    pub network: Network,
    pub providers: Vec<Provider>,
    pub catalogue: Vec<Model>,
    pub options: Options,
}

/// Siphon, started: its runtime, its threads, the models they ask, the MCP servers, and the protocol's methods and
/// topics as Siphon and its plugins brought them.
#[derive(Debug, Clone)]
pub struct Siphon {
    pub runtime: Runtime,
    pub threads: Threads<ThreadProgram>,
    pub models: Arc<Models>,
    pub servers: Servers,
    pub(crate) registry: Arc<plugin::Registry>,
}

/// Siphon being built: what its plugins add to, and what it is started with.
#[derive(Debug)]
pub struct Building {
    pub builder: Builder,
    events: Events,
    network: Network,
    catalogue: Vec<Model>,
    options: Options,
}

impl Siphon {
    /// Siphon to be built on `beak` as `setup` says, its volume opened, with its own tools, programs, methods and
    /// topics; its plugins next.
    ///
    /// # Errors
    /// The volume's failure, as another holding its directory.
    pub fn builder(beak: &Beak, setup: Setup) -> beak::Result<Building> {
        let Setup {
            machine,
            network,
            providers,
            catalogue,
            options,
        } = setup;
        let (kernel, events) = options.open(beak)?;
        let mut builder = plugin::Builder {
            kernel,
            machine,
            runtime: Runtime::builder(),
            tools: Tools::new()
                .with(ReadTool)
                .with(WriteTool)
                .with(EditTool)
                .with(GlobTool)
                .with(GrepTool)
                .with(ShellTool),
            providers,
            registry: plugin::Registry::default(),
        };
        builder.methods(
            siphon_protocol::siphon::set(),
            siphon_protocol::siphon::Method::read,
            connection::answered,
        );
        builder.topics(Scope::ThreadsRead, connection::told);
        Ok(Building {
            builder,
            events,
            network,
            catalogue,
            options,
        })
    }
}

impl Building {
    /// Adds what `plugin` brings.
    #[must_use]
    pub fn plugin(mut self, plugin: impl Plugin + 'static) -> Self {
        self.builder.plugin(plugin);
        self
    }

    /// Siphon, started on the current Tokio runtime.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn start(self) -> beak::Result<Siphon> {
        let Building {
            builder,
            events,
            network,
            catalogue,
            options,
        } = self;
        let plugin::Builder {
            kernel,
            machine,
            runtime,
            tools,
            providers,
            registry,
        } = builder;
        let servers = Servers::start(machine.clone(), machine.home.as_deref());
        let inner = tools.with_servers(servers.clone());
        let outer = inner
            .clone()
            .with(ExecuteTool { tools: inner.clone() })
            .with(QuestionTool)
            .with(DelegateTool);
        let models = Arc::new(Models::new(network, providers, catalogue));
        let runtime = programs(runtime, &machine, &models, &servers, inner, outer).start_on((kernel, events), options)?;
        Ok(Siphon {
            threads: Threads::new(runtime.clone()),
            runtime,
            models,
            servers,
            registry: Arc::new(registry),
        })
    }
}

/// The runtime's own programs and handlers beside the plugins' in `runtime`: the thread's processes, the requests
/// answered by the machine and the models, and those the person answers.
fn programs(
    runtime: siphon_runtime::Builder,
    machine: &Host,
    models: &Arc<Models>,
    servers: &Servers,
    inner: Tools,
    tools: Tools,
) -> siphon_runtime::Builder {
    let sources = Sources {
        tools: tools.clone(),
        servers: servers.clone(),
        models: Arc::clone(models),
        host: machine.clone(),
        home: machine.home.clone(),
    };
    let (gathering, asking, reviewing) = (sources, Arc::clone(models), Arc::clone(models));
    let (execs, host) = (tools.clone(), machine.clone());
    runtime
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
