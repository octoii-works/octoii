//! Octoii, the Agent host on Siphon, in layers, each module using only those above it in this list: the
//! system reminders Octoii speaks to a model in; the machine a thread reaches; the threads themselves, their store; the
//! environment's settings and the providers' accounts; the tools a thread's model calls and the weighing of its calls;
//! the person's trust in what a project's configuration runs; the agent that runs a thread's turns, tells its model and
//! compacts it; the extensions a host builds Octoii with; the protocol a client speaks, and its connection — started as
//! with the fibers that run a thread's processes and the handlers that answer their requests.

pub mod access;
pub(crate) mod accounts;
pub mod agent;
pub mod connection;
mod extension;
mod kept;
pub mod machine;
pub mod protocol;
pub mod providers;
pub mod reminder;
pub mod settings;
pub mod testing;
#[cfg(test)]
mod tests;
pub mod threads;
pub mod tools;
pub mod trust;
pub mod volume;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::agent::compaction::{AwaitCompaction, answer_compaction};
use crate::agent::{
    CORE, CompactFiber, DelegatedFiber, Gather, ModelRequest, ResumeFiber, Sources, ThreadFiber, TitleFiber, Told, TurnFiber,
    answer_gather, answer_model, answer_review,
};
use crate::machine::Host;
use crate::protocol::Scope;
use crate::threads::Threads;
use crate::tools::{
    Asks, EditTool, Exec, ExecuteTool, GlobTool, GrepTool, Permission, ReadTool, Review, ScriptFiber, ShellTool, Tools, WriteTool,
    answer_exec,
};
use octoii_ai::catalogue::Model;
use octoii_ai::host::Credentials;
use octoii_ai::models::{Host as Network, Models, Provider};
use octoii_ai::transport::Transport;
use octoii_siphon_lib::{Asked, Runtime, RuntimeHandle, Status};

pub use extension::{Caller, Extension, Full, Lagged, Registrar, Sink, Subscription};
pub use kept::{Ends, Kept};

/// What Octoii is built on: the machine its work is done on; the host's credentials
/// and where the models' requests go, the host's providers and the catalogue of models; and the volume's directory. The
/// ids the models' answers are given are the volume's, and their clock the runtime's.
pub struct Setup {
    pub machine: Host,
    pub sign_in: providers::SignInSetup,
    pub credentials: Arc<dyn Credentials>,
    pub transport: Arc<dyn Transport>,
    pub providers: Vec<Provider>,
    pub catalogue: Vec<Model>,
    /// The persistent volume directory. None uses memory.
    pub directory: Option<PathBuf>,
}

/// Octoii, started: the machine its work is done on, its threads, the models they ask — reached with the
/// host's providers, the credentials kept, and how a sign-in comes back — the sign-ins begun, and the protocol's
/// methods and topics as Octoii and its extensions brought them.
#[derive(Clone)]
pub struct Octoii {
    pub machine: Host,
    runtime: Runtime,
    pub threads: Threads<ThreadFiber>,
    pub models: Arc<Models>,
    pub credentials: Arc<dyn Credentials>,
    pub providers: Arc<Vec<Provider>>,
    pub sign_in: Arc<providers::SignInSetup>,
    pub sign_ins: providers::SignIns,
    pub(crate) registry: Arc<extension::Registry>,
    sources: Sources,
}

impl Octoii {
    /// The sessions and pairing links stored in this product volume.
    #[must_use]
    pub fn access(&self) -> access::Access {
        access::Access::new(self.runtime.runtime_handle())
    }

    /// Stops the product's execution and waits for it to close.
    pub async fn close(&self) {
        let runtime = self.runtime.clone();
        let _ = tokio::task::spawn_blocking(move || runtime.close()).await;
    }

    /// The volume through Octoii's public operations, without exposing its storage implementation.
    #[must_use]
    pub fn volume(&self) -> crate::volume::Volume {
        crate::volume::Volume {
            runtime: self.runtime.runtime_handle(),
        }
    }

    /// The sets of methods Octoii and its extensions brought, as the protocol is described by.
    #[must_use]
    pub fn sets(&self) -> &[crate::protocol::Set] {
        &self.registry.sets
    }

    /// What the model of the thread `id` is told now, as it would be gathered before its next turn: its sections, its
    /// tools, and the names its scripts may call.
    ///
    /// # Errors
    /// The volume's failure; a thread's settings not as Octoii writes them.
    pub async fn told(&self, id: &str) -> anyhow::Result<Told> {
        crate::agent::gather::gathered(&self.sources, &self.runtime, id).await
    }

    /// The machine as the person reads it, beside the threads: their paths taken whole, from the root, read alone, what
    /// the node keeps for itself hidden from it as from every thread.
    ///
    /// # Errors
    /// Beak's failure to make its namespace.
    pub fn person_machine(&self) -> octoii_siphon_lib::Result<crate::machine::Machine> {
        let base = crate::machine::paths::Base {
            workspace: std::path::PathBuf::from("/"),
            home: self.machine.home.clone(),
        };
        crate::machine::Machine::open(
            &self.runtime,
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

/// Who calls a method of the host's own: Octoii whole, which the host is given and an extension is not, and the session
/// of the client that called.
#[derive(Debug, Clone)]
pub struct HostCaller {
    pub siphon: Octoii,
    pub session: String,
}

/// Octoii being built: what its extensions add to, and what it is started with.
pub struct Building {
    builder: extension::Builder,
    runtime: Runtime,
    sign_in: providers::SignInSetup,
    credentials: Arc<dyn Credentials>,
    network: Network,
    catalogue: Vec<Model>,
}

impl std::fmt::Debug for Octoii {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Octoii").field("machine", &self.machine).finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Setup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Setup").field("machine", &self.machine).finish_non_exhaustive()
    }
}

impl Octoii {
    /// Octoii to be built as `setup` says, its volume opened, with its own tools, fibers, methods and
    /// topics; its extensions next.
    ///
    /// # Errors
    /// The host executor or volume cannot be opened.
    ///
    /// # Panics
    /// Never as an id is minted for a model's answer: Beak mints one at any time, going on into the next millisecond
    /// past one's ids, till the year 10889.
    pub fn builder(setup: Setup) -> octoii_siphon_lib::Result<Building> {
        let Setup {
            machine,
            sign_in,
            credentials,
            transport,
            providers,
            catalogue,
            directory,
        } = setup;
        let runtime = Runtime::open(directory.as_deref(), None)?;
        let handle = runtime.runtime_handle();
        let registry = octoii_siphon_lib::Builder::new(&handle)?;
        let (minting, clock) = (handle.clone(), handle.clone());
        let network = Network {
            credentials: Arc::clone(&credentials),
            transport,
            new_id: Arc::new(move || minting.new_id().expect("the volume mints an id")),
            clock: Arc::new(move || clock.now_ms().expect("the runtime clock")),
        };
        let mut builder = extension::Builder {
            handle,
            machine,
            execution: registry,
            tools: Tools::new(),
            owners: HashMap::new(),
            providers,
            tellers: Vec::new(),
            prompts: Vec::new(),
            natives: Vec::new(),
            writables: Vec::new(),
            writable_folders: crate::agent::WritableFolders::new(),
            registry: extension::Registry::default(),
        };
        own(&mut builder.registrar(CORE));
        builder.methods(
            CORE,
            crate::protocol::siphon::set(),
            crate::protocol::siphon::Method::read,
            connection::answered,
        );
        builder.topics(CORE, Scope::ThreadsRead, connection::told);
        Ok(Building {
            builder,
            runtime,
            sign_in,
            credentials,
            network,
            catalogue,
        })
    }
}

impl std::fmt::Debug for Building {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Building").field("builder", &self.builder).finish_non_exhaustive()
    }
}

impl Building {
    /// Adds the host's own methods of `set`, each read by `read` and done by `handle`: methods of the host's, as Octoii's
    /// are Octoii's, which no setting switches off.
    #[must_use]
    pub fn methods<M, F, Done>(
        mut self,
        set: crate::protocol::Set,
        read: fn(&str, serde_json::Value) -> serde_json::Result<M>,
        handle: F,
    ) -> Self
    where
        M: Send + 'static,
        F: Fn(HostCaller, M) -> Done + Send + Sync + 'static,
        Done: std::future::Future<Output = Result<serde_json::Value, crate::protocol::Error>> + Send + 'static,
    {
        self.builder.methods(CORE, set, read, move |caller: Caller, method| {
            handle(
                HostCaller {
                    siphon: caller.siphon,
                    session: caller.session,
                },
                method,
            )
        });
        self
    }

    /// Adds what `extension` brings.
    #[must_use]
    pub fn extension(mut self, extension: impl Extension + 'static) -> Self {
        self.builder.extension(extension);
        self
    }

    /// Octoii, started on the current Tokio runtime.
    ///
    /// # Errors
    /// The volume's failure, a missing or duplicate base extension, or invalid independent model prompts.
    pub fn start(self) -> octoii_siphon_lib::Result<Octoii> {
        let base_count = self
            .builder
            .registry
            .extensions
            .iter()
            .filter(|extension| extension.name == "base")
            .count();
        if base_count != 1 {
            return Err(octoii_siphon_lib::Error::new(
                Status::Invalid,
                format!("exactly one base extension is required; found {base_count}"),
            ));
        }
        let [registered] = self.builder.prompts.as_slice() else {
            return Err(octoii_siphon_lib::Error::new(
                Status::Invalid,
                "base must supply independent model prompts exactly once",
            ));
        };
        let prompts = registered.item;
        if registered.extension != "base"
            || [prompts.title, prompts.recap, prompts.review]
                .iter()
                .any(|text| text.trim().is_empty())
        {
            return Err(octoii_siphon_lib::Error::new(
                Status::Invalid,
                "independent model prompts must be nonempty and supplied by base",
            ));
        }
        let Building {
            builder,
            runtime,
            sign_in,
            credentials,
            network,
            catalogue,
        } = self;
        builder.writable_folders.finish(builder.writables.clone())?;
        let extension::Builder {
            handle,
            machine,
            execution,
            tools,
            owners,
            providers,
            tellers,
            prompts: _,
            natives,
            writables,
            writable_folders: _,
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
        let execution = register_fibers_and_handlers(execution, handle, &machine, sources.clone(), tools, writables, prompts)?;
        let siphon = Octoii {
            machine,
            threads: Threads::new(runtime.runtime_handle()),
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
        execution.start()?;
        Ok(siphon)
    }
}

/// Octoii's built-in coding tools and execute; model instructions are supplied by extensions.
fn own(registrar: &mut Registrar<'_>) {
    registrar.tool(ReadTool);
    registrar.tool(WriteTool);
    registrar.tool(EditTool);
    registrar.tool(GlobTool);
    registrar.tool(GrepTool);
    registrar.tool(ShellTool);
    registrar.tool(ExecuteTool);
}

/// The runtime's own fibers and handlers beside the extensions' in `runtime`: the thread's processes, the requests
/// answered by the machine and the models, and those the person answers.
fn register_fibers_and_handlers(
    mut runtime: octoii_siphon_lib::Builder,
    state: RuntimeHandle,
    machine: &Host,
    sources: Sources,
    tools: Tools,
    writables: Vec<crate::agent::Writable>,
    prompts: crate::agent::Prompts,
) -> octoii_siphon_lib::Result<octoii_siphon_lib::Builder> {
    let executor =
        tokio::runtime::Handle::try_current().map_err(|error| octoii_siphon_lib::Error::new(Status::Invalid, error.to_string()))?;
    let models = Arc::clone(&sources.models);
    let (gathering, asking, reviewing) = (sources, Arc::clone(&models), Arc::clone(&models));
    let (execs, host) = (tools.clone(), machine.clone());
    let (gathering_state, asking_state, reviewing_state) = (state.clone(), state.clone(), state.clone());
    let compaction_state = state.clone();
    runtime
        .local_machine()?
        .fiber(ThreadFiber)?
        .fiber(TurnFiber {
            tools: tools.clone(),
            machine: machine.clone(),
            models,
            writables,
        })?
        .fiber(ScriptFiber {
            tools,
            primitives: Arc::new(crate::agent::ThreadPrimitives),
        })?
        .fiber(TitleFiber {
            instruction: prompts.title,
        })?
        .fiber(CompactFiber {
            instruction: prompts.recap,
        })?
        .fiber(DelegatedFiber)?
        .fiber(ResumeFiber)?
        .handle(&executor, move |asked: Asked<Gather>| {
            let sources = gathering.clone();
            let state = gathering_state.clone();
            async move { answer_gather(&sources, &state, asked).await }
        })?
        .handle(&executor, move |asked: Asked<ModelRequest>| {
            let models = Arc::clone(&asking);
            let state = asking_state.clone();
            async move { answer_model(&models, &state, asked).await }
        })?
        .handle(&executor, move |asked: Asked<AwaitCompaction>| {
            let state = compaction_state.clone();
            async move { answer_compaction(&state, asked).await }
        })?
        .handle(&executor, move |asked: Asked<Review>| {
            let models = Arc::clone(&reviewing);
            let state = reviewing_state.clone();
            async move { answer_review(&models, prompts.review, &state, asked).await }
        })?
        .handle(&executor, move |asked: Asked<Exec>| {
            let (tools, host) = (execs.clone(), host.clone());
            let state = state.clone();
            async move { answer_exec(&tools, &host, &state, asked).await }
        })?
        .ask_person::<Permission>()?
        .ask_person::<Asks>()?
        .fiber(trust::TrustFiber)?
        .ask_person::<trust::Trust>()?;
    Ok(runtime)
}

#[cfg(test)]
mod base_tests {
    use super::*;

    struct Base;
    impl Extension for Base {
        fn name(&self) -> &'static str {
            "base"
        }
        fn title(&self) -> &'static str {
            "Base"
        }
        fn description(&self) -> &'static str {
            "Test fixture"
        }
        fn build(self: Box<Self>, _: &mut Registrar<'_>) {}
    }

    #[tokio::test]
    async fn starting_requires_exactly_one_base_extension() {
        let missing = Octoii::builder(testing::setup(Host::default())).unwrap().start().unwrap_err();
        assert!(missing.message.contains("found 0"));
        let duplicate = Octoii::builder(testing::setup(Host::default()))
            .unwrap()
            .extension(Base)
            .extension(Base)
            .start()
            .unwrap_err();
        assert!(duplicate.message.contains("found 2"));
    }

    #[tokio::test]
    async fn starting_rejects_invalid_independent_prompts() {
        let empty = crate::agent::Prompts {
            title: " ",
            ..testing::PROMPTS
        };
        for registrations in [
            vec![],
            vec![("base", testing::PROMPTS), ("base", testing::PROMPTS)],
            vec![("other", testing::PROMPTS)],
            vec![("base", empty)],
            vec![(
                "base",
                crate::agent::Prompts {
                    recap: "",
                    ..testing::PROMPTS
                },
            )],
            vec![(
                "base",
                crate::agent::Prompts {
                    review: "\n",
                    ..testing::PROMPTS
                },
            )],
        ] {
            let mut building = Octoii::builder(testing::setup(Host::default())).unwrap().extension(Base);
            for (owner, prompts) in registrations {
                building.builder.registrar(owner).prompts(prompts);
            }
            let error = building.start().unwrap_err();
            assert_eq!(error.status(), octoii_siphon_lib::Status::Invalid);
            assert!(error.message.contains("prompts"));
        }
    }
}
