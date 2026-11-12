//! Siphon, Octoii's runtime of agents on Beak, in layers, each module using only those above it in this list: the
//! machine a thread reaches; the threads themselves, their store; the environment's settings and the providers'
//! accounts; the tools a thread's model calls and the weighing of its calls; the agent that runs a thread's turns,
//! tells its model and compacts it; the extensions a host builds Siphon with; the protocol a client speaks, and its
//! connection — started as one with the programs of a thread's and the handlers of its requests.

pub mod accounts;
pub mod agent;
pub mod connection;
mod extension;
mod kept;
pub mod machine;
pub mod protocol;
pub mod providers;
pub mod settings;
pub mod testing;
pub mod threads;
pub mod tools;

use std::collections::HashMap;
use std::sync::Arc;

use crate::agent::{
    CORE, CompactProgram, DelegatedProgram, Gather, ModelRequest, ResumeProgram, Sources, ThreadProgram, TitleProgram, Told, TurnProgram,
    answer_gather, answer_model, answer_review, sections,
};
use crate::machine::Host;
use crate::protocol::Scope;
use crate::threads::Threads;
use crate::tools::{
    Asks, EditTool, Exec, ExecuteTool, GlobTool, GrepTool, Permission, ReadTool, Review, ScriptProgram, ShellTool, Tools, WriteTool,
    answer_exec,
};
use beak::{Beak, Events};
use octoii_ai::catalogue::Model;
use octoii_ai::host::Credentials;
use octoii_ai::models::{Host as Network, Models, Provider};
use octoii_ai::transport::Transport;
use siphon_runtime::{Asked, Options, Runtime};

pub use extension::{Caller, Extension, Full, Lagged, Registrar, Sink, Subscription};
pub use kept::{Ends, Kept};

/// What Siphon is built on: the machine its work is done on; the folder of the person's own configuration, for every
/// project — their instructions, `AGENTS.md`, and their skills, `skills/` — where they keep one; the host's credentials
/// and where the models' requests go, the host's providers and the catalogue of models; and the volume's options. The
/// ids the models' answers are given are the volume's, and their clock the runtime's.
pub struct Setup {
    pub machine: Host,
    pub config: Option<std::path::PathBuf>,
    pub sign_in: providers::SignInSetup,
    pub credentials: Arc<dyn Credentials>,
    pub transport: Arc<dyn Transport>,
    pub providers: Vec<Provider>,
    pub catalogue: Vec<Model>,
    pub options: Options,
}

/// Siphon, started: the machine its work is done on, its runtime, its threads, the models they ask — reached with the
/// host's providers, the credentials kept, and how a sign-in comes back — the sign-ins begun, and the protocol's
/// methods and topics as Siphon and its extensions brought them.
#[derive(Clone)]
pub struct Siphon {
    pub machine: Host,
    pub runtime: Runtime,
    pub threads: Threads<ThreadProgram>,
    pub models: Arc<Models>,
    pub credentials: Arc<dyn Credentials>,
    pub providers: Arc<Vec<Provider>>,
    pub sign_in: Arc<providers::SignInSetup>,
    pub sign_ins: providers::SignIns,
    pub(crate) registry: Arc<extension::Registry>,
    sources: Sources,
}

impl Siphon {
    /// The sets of methods Siphon and its extensions brought, as the protocol is described by.
    #[must_use]
    pub fn sets(&self) -> &[crate::protocol::Set] {
        &self.registry.sets
    }

    /// What the model of the thread `id` is told now, as it would be gathered before its next turn: its sections, its
    /// tools, and the names its scripts may call.
    ///
    /// # Errors
    /// The volume's failure; a thread's settings not as Siphon writes them.
    pub async fn told(&self, id: &str) -> anyhow::Result<Told> {
        crate::agent::gather::gathered(&self.sources, &self.runtime, id).await
    }

    /// The machine as the person reads it, beside the threads: their paths taken whole, from the root, read alone, what
    /// the node keeps for itself hidden from it as from every thread.
    ///
    /// # Errors
    /// Beak's failure to make its namespace.
    pub fn person_machine(&self) -> beak::Result<crate::machine::Machine> {
        let base = crate::machine::paths::Base {
            workspace: std::path::PathBuf::from("/"),
            home: self.machine.home.clone(),
        };
        crate::machine::Machine::open(
            self.runtime.kernel(),
            &self.machine,
            base,
            &crate::machine::Writes::Within(Vec::new()),
            None,
        )
    }

    /// Deletes the thread `id` and every thread under it, the scratchpad of each, each extension letting go of what it
    /// held for each.
    ///
    /// # Errors
    /// `NoSuchThread`; the volume's failure.
    pub async fn delete_thread(&self, id: &str) -> Result<(), crate::threads::Error> {
        for deleted in self.threads.delete(id).await? {
            if let Some(scratchpad) = self.machine.scratchpad_of(&deleted) {
                // What is left there is the system's to clear in time, where it cannot be removed now.
                let _ = std::fs::remove_dir_all(scratchpad);
            }
            for extension in &self.registry.deleted {
                extension(&deleted);
            }
        }
        Ok(())
    }
}

/// Siphon being built: what its extensions add to, and what it is started with.
pub struct Building {
    builder: extension::Builder,
    sign_in: providers::SignInSetup,
    credentials: Arc<dyn Credentials>,
    events: Events,
    network: Network,
    catalogue: Vec<Model>,
    options: Options,
}

impl std::fmt::Debug for Siphon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Siphon").field("machine", &self.machine).finish_non_exhaustive()
    }
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
    /// topics; its extensions next.
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
            sign_in,
            credentials,
            transport,
            providers,
            catalogue,
            options,
        } = setup;
        let (kernel, events) = options.open(beak)?;
        let (minting, minted_at, clock) = (kernel.clone(), Arc::clone(&options.clock), Arc::clone(&options.clock));
        let network = Network {
            credentials: Arc::clone(&credentials),
            transport,
            // Beak mints a UUIDv7 at any time: past a millisecond's ids it goes on in the next, till the year 10889.
            new_id: Arc::new(move || minting.new_id(minted_at.now_ms()).expect("the volume mints an id")),
            clock: Arc::new(move || clock.now_ms()),
        };
        let mut builder = extension::Builder {
            kernel,
            machine,
            runtime: Runtime::builder(),
            tools: Tools::new(),
            owners: HashMap::new(),
            providers,
            tellers: Vec::new(),
            natives: Vec::new(),
            registry: extension::Registry::default(),
        };
        own(&mut builder.registrar(CORE), config.as_deref());
        builder.methods(
            CORE,
            crate::protocol::siphon::set(),
            crate::protocol::siphon::Method::read,
            connection::answered,
        );
        builder.topics(CORE, Scope::ThreadsRead, connection::told);
        Ok(Building {
            builder,
            sign_in,
            credentials,
            events,
            network,
            catalogue,
            options,
        })
    }
}

impl std::fmt::Debug for Building {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Building").field("builder", &self.builder).finish_non_exhaustive()
    }
}

impl Building {
    /// Adds the host's own methods of `set`, each read by `read` and done by `handle`: methods of the host's, as Siphon's
    /// are Siphon's, which no setting switches off.
    #[must_use]
    pub fn methods<M, F, Done>(
        mut self,
        set: crate::protocol::Set,
        read: fn(&str, serde_json::Value) -> serde_json::Result<M>,
        handle: F,
    ) -> Self
    where
        M: Send + 'static,
        F: Fn(Caller, M) -> Done + Send + Sync + 'static,
        Done: std::future::Future<Output = Result<serde_json::Value, crate::protocol::Error>> + Send + 'static,
    {
        self.builder.methods(CORE, set, read, handle);
        self
    }

    /// Adds what `extension` brings.
    #[must_use]
    pub fn extension(mut self, extension: impl Extension + 'static) -> Self {
        self.builder.extension(extension);
        self
    }

    /// Siphon, started on the current Tokio runtime.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn start(self) -> beak::Result<Siphon> {
        let Building {
            builder,
            sign_in,
            credentials,
            events,
            network,
            catalogue,
            options,
        } = self;
        let extension::Builder {
            kernel,
            machine,
            runtime,
            tools,
            owners,
            providers,
            tellers,
            natives,
            registry,
        } = builder;
        let hosts = Arc::new(providers);
        let models = Arc::new(Models::new(network, hosts.to_vec(), catalogue));
        let sources = Sources {
            tools: tools.clone(),
            owners,
            tellers: Sources::ordered(tellers),
            natives,
            models: Arc::clone(&models),
        };
        let runtime = programs(runtime, &machine, &models, sources.clone(), tools).start_on((kernel, events), options)?;
        let siphon = Siphon {
            machine,
            threads: Threads::new(runtime.clone()),
            runtime,
            models,
            credentials,
            providers: hosts,
            sign_in: Arc::new(sign_in),
            sign_ins: providers::SignIns::start(),
            registry: Arc::new(registry),
            sources,
        };
        // The accounts the person added are providers beside the host's; an extension switched off brings none.
        if let Err(providers::Refused::Beak(error)) = providers::refreshed(&siphon) {
            return Err(error);
        }
        Ok(siphon)
    }
}

/// What Siphon adds as its own, through `registrar`: the coding tools and `execute`; and what tells a thread's model the
/// base, the instructions it follows and the skills it may use — found in the folder of the person's configuration
/// `config` and the project's — its memory of the project, and the environment it works in.
fn own(registrar: &mut Registrar<'_>, config: Option<&std::path::Path>) {
    registrar.tool(ReadTool);
    registrar.tool(WriteTool);
    registrar.tool(EditTool);
    registrar.tool(GlobTool);
    registrar.tool(GrepTool);
    registrar.tool(ShellTool);
    registrar.tool(ExecuteTool);
    let host = registrar.machine().clone();
    for (shared, teller) in sections::tellers(&host, config) {
        registrar.teller(shared, teller);
    }
}

/// The runtime's own programs and handlers beside the extensions' in `runtime`: the thread's processes, the requests
/// answered by the machine and the models, and those the person answers.
fn programs(
    runtime: siphon_runtime::Builder,
    machine: &Host,
    models: &Arc<Models>,
    sources: Sources,
    tools: Tools,
) -> siphon_runtime::Builder {
    let (gathering, asking, reviewing) = (sources, Arc::clone(models), Arc::clone(models));
    let (execs, host) = (tools.clone(), machine.clone());
    runtime
        .program(ThreadProgram)
        .program(TurnProgram {
            tools: tools.clone(),
            machine: machine.clone(),
            models: Arc::clone(models),
        })
        .program(ScriptProgram {
            tools,
            primitives: Arc::new(crate::agent::ThreadPrimitives),
        })
        .program(TitleProgram)
        .program(CompactProgram)
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
