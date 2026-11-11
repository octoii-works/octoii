//! Siphon whole: the runtime on Beak with the programs of a thread's and the handlers of its requests, the tools its
//! model calls, the models it asks, and the protocol a client speaks to it — built with the plugins a host picks, which
//! bring the rest: their providers, tools, programs, what they tell the model, and their methods.

pub mod connection;
mod plugin;
pub mod testing;

use std::sync::Arc;

use beak::{Beak, Events};
use octoii_ai::catalogue::Model;
use octoii_ai::host::Credentials;
use octoii_ai::models::{Host as Network, Models, Provider};
use octoii_ai::transport::Transport;
use siphon_agent::{
    CompactProgram, DelegateProgram, DelegateTool, DelegatedProgram, Gather, ModelRequest, ResumeProgram, Sources, ThreadProgram,
    TitleProgram, TurnProgram, answer_gather, answer_model, answer_review,
};
use siphon_machine::Host;
use siphon_protocol::Scope;
use siphon_runtime::{Asked, Options, Runtime};
use siphon_threads::Threads;
use siphon_tools::{
    Asks, EditTool, Exec, ExecuteTool, GlobTool, GrepTool, Permission, QuestionTool, ReadTool, Review, ScriptProgram, ShellTool, Tools,
    WriteTool, answer_exec,
};

pub use plugin::{Builder, Context, Full, Lagged, Plugin, Sink, Subscription};

/// What Siphon is built on: the machine its work is done on; the folder of the person's own configuration, for every
/// project — their instructions, `AGENTS.md`, and their skills, `skills/` — where they keep one; the host's credentials
/// and where the models' requests go, the host's providers and the catalogue of models; and the volume's options. The
/// ids the models' answers are given are the volume's, and their clock the runtime's.
pub struct Setup {
    pub machine: Host,
    pub config: Option<std::path::PathBuf>,
    pub credentials: Arc<dyn Credentials>,
    pub transport: Arc<dyn Transport>,
    pub providers: Vec<Provider>,
    pub catalogue: Vec<Model>,
    pub options: Options,
}

/// Siphon, started: the machine its work is done on, its runtime, its threads, the models they ask, and the protocol's
/// methods and topics as Siphon and its plugins brought them.
#[derive(Debug, Clone)]
pub struct Siphon {
    pub machine: Host,
    pub runtime: Runtime,
    pub threads: Threads<ThreadProgram>,
    pub models: Arc<Models>,
    pub(crate) registry: Arc<plugin::Registry>,
}

impl Siphon {
    /// The sets of methods Siphon and its plugins brought, as the protocol is described by.
    #[must_use]
    pub fn sets(&self) -> &[siphon_protocol::Set] {
        &self.registry.sets
    }

    /// Deletes the thread `id` and every thread under it, each plugin letting go of what it held for each.
    ///
    /// # Errors
    /// `NoSuchThread`; the volume's failure.
    pub async fn delete_thread(&self, id: &str) -> Result<(), siphon_threads::Error> {
        for deleted in self.threads.delete(id).await? {
            for plugin in &self.registry.deleted {
                plugin(&deleted);
            }
        }
        Ok(())
    }
}

/// Siphon being built: what its plugins add to, and what it is started with.
#[derive(Debug)]
pub struct Building {
    pub builder: Builder,
    config: Option<std::path::PathBuf>,
    events: Events,
    network: Network,
    catalogue: Vec<Model>,
    options: Options,
}

impl std::fmt::Debug for Setup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Setup")
            .field("machine", &self.machine)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Siphon {
    /// Siphon to be built on `beak` as `setup` says, its volume opened, with its own tools, programs, methods and
    /// topics; its plugins next.
    ///
    /// # Errors
    /// The volume's failure, as another holding its directory.
    ///
    /// # Panics
    /// Never as an id is minted for a model's answer: Beak mints one at any time, going on into the next millisecond
    /// past one's ids, till the year 10889.
    pub fn builder(beak: &Beak, setup: Setup) -> beak::Result<Building> {
        let Setup {
            machine,
            config,
            credentials,
            transport,
            providers,
            catalogue,
            options,
        } = setup;
        let (kernel, events) = options.open(beak)?;
        let (minting, minted_at, clock) = (kernel.clone(), Arc::clone(&options.clock), Arc::clone(&options.clock));
        let network = Network {
            credentials,
            transport,
            // Beak mints a UUIDv7 at any time: past a millisecond's ids it goes on in the next, till the year 10889.
            new_id: Arc::new(move || minting.new_id(minted_at.now_ms()).expect("the volume mints an id")),
            clock: Arc::new(move || clock.now_ms()),
        };
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
            tellers: Vec::new(),
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
            config,
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
            config,
            events,
            network,
            catalogue,
            options,
        } = self;
        let plugin::Builder {
            kernel,
            machine,
            runtime,
            tools: inner,
            providers,
            tellers,
            registry,
        } = builder;
        let outer = inner
            .clone()
            .with(ExecuteTool { tools: inner.clone() })
            .with(QuestionTool)
            .with(DelegateTool);
        let models = Arc::new(Models::new(network, providers, catalogue));
        let runtime = programs(runtime, &machine, config, &models, tellers, inner, outer).start_on((kernel, events), options)?;
        Ok(Siphon {
            machine,
            threads: Threads::new(runtime.clone()),
            runtime,
            models,
            registry: Arc::new(registry),
        })
    }
}

/// The runtime's own programs and handlers beside the plugins' in `runtime`: the thread's processes, the requests
/// answered by the machine and the models, and those the person answers.
fn programs(
    runtime: siphon_runtime::Builder,
    machine: &Host,
    config: Option<std::path::PathBuf>,
    models: &Arc<Models>,
    tellers: Vec<Arc<dyn siphon_agent::Teller>>,
    inner: Tools,
    tools: Tools,
) -> siphon_runtime::Builder {
    let sources = Sources {
        tools: tools.clone(),
        tellers,
        models: Arc::clone(models),
        host: machine.clone(),
        config,
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
}
