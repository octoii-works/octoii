//! Siphon, Octoii's runtime of agents on Beak, in layers, each module using only those above it in this list: the
//! system reminders Octoii speaks to a model in; the machine a thread reaches; the threads themselves, their store; the
//! environment's settings and the providers' accounts; the tools a thread's model calls and the weighing of its calls;
//! the person's trust in what a project's configuration runs; the agent that runs a thread's turns, tells its model and
//! compacts it; the extensions a host builds Siphon with; the protocol a client speaks, and its connection — started as
//! one with the programs of a thread's and the handlers of its requests.

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
use std::sync::Arc;

use crate::agent::{
    CORE, CompactProgram, DelegatedProgram, Gather, ModelRequest, ResumeProgram, Sources, ThreadProgram, TitleProgram, Told, TurnProgram,
    answer_gather, answer_model, answer_review,
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

/// What Siphon is built on: the machine its work is done on; the host's credentials
/// and where the models' requests go, the host's providers and the catalogue of models; and the volume's options. The
/// ids the models' answers are given are the volume's, and their clock the runtime's.
pub struct Setup {
    pub machine: Host,
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

/// Who calls a method of the host's own: Siphon whole, which the host is given and an extension is not, and the session
/// of the client that called.
#[derive(Debug, Clone)]
pub struct HostCaller {
    pub siphon: Siphon,
    pub session: String,
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
        f.debug_struct("Setup").field("machine", &self.machine).finish_non_exhaustive()
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

    /// Siphon, started on the current Tokio runtime.
    ///
    /// # Errors
    /// The volume's failure, a missing or duplicate base extension, or invalid independent model prompts.
    pub fn start(self) -> beak::Result<Siphon> {
        let base_count = self
            .builder
            .registry
            .extensions
            .iter()
            .filter(|extension| extension.name == "base")
            .count();
        if base_count != 1 {
            return Err(beak::Error {
                status: beak::Status::Invalid,
                reason: format!("exactly one base extension is required; found {base_count}"),
            });
        }
        let [registered] = self.builder.prompts.as_slice() else {
            return Err(beak::Error {
                status: beak::Status::Invalid,
                reason: "base must supply independent model prompts exactly once".into(),
            });
        };
        let prompts = registered.item;
        if registered.extension != "base"
            || [prompts.title, prompts.recap, prompts.review]
                .iter()
                .any(|text| text.trim().is_empty())
        {
            return Err(beak::Error {
                status: beak::Status::Invalid,
                reason: "independent model prompts must be nonempty and supplied by base".into(),
            });
        }
        let Building {
            builder,
            sign_in,
            credentials,
            events,
            network,
            catalogue,
            options,
        } = self;
        builder.writable_folders.finish(builder.writables.clone())?;
        let extension::Builder {
            kernel,
            machine,
            runtime,
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
        let runtime =
            programs(runtime, &machine, &models, sources.clone(), tools, writables, prompts).start_on((kernel, events), options)?;
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

/// Siphon's built-in coding tools and execute; model instructions are supplied by extensions.
fn own(registrar: &mut Registrar<'_>) {
    registrar.tool(ReadTool);
    registrar.tool(WriteTool);
    registrar.tool(EditTool);
    registrar.tool(GlobTool);
    registrar.tool(GrepTool);
    registrar.tool(ShellTool);
    registrar.tool(ExecuteTool);
}

/// The runtime's own programs and handlers beside the extensions' in `runtime`: the thread's processes, the requests
/// answered by the machine and the models, and those the person answers.
fn programs(
    runtime: siphon_runtime::Builder,
    machine: &Host,
    models: &Arc<Models>,
    sources: Sources,
    tools: Tools,
    writables: Vec<crate::agent::Writable>,
    prompts: crate::agent::Prompts,
) -> siphon_runtime::Builder {
    let (gathering, asking, reviewing) = (sources, Arc::clone(models), Arc::clone(models));
    let (execs, host) = (tools.clone(), machine.clone());
    runtime
        .program(ThreadProgram)
        .program(TurnProgram {
            tools: tools.clone(),
            machine: machine.clone(),
            models: Arc::clone(models),
            writables,
        })
        .program(ScriptProgram {
            tools,
            primitives: Arc::new(crate::agent::ThreadPrimitives),
        })
        .program(TitleProgram {
            instruction: prompts.title,
        })
        .program(CompactProgram {
            instruction: prompts.recap,
        })
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
            async move { answer_review(&models, prompts.review, asked).await }
        })
        .handle(move |asked: Asked<Exec>| {
            let (tools, host) = (execs.clone(), host.clone());
            async move { answer_exec(&tools, &host, asked).await }
        })
        .ask_person::<Permission>()
        .ask_person::<Asks>()
        .program(trust::TrustProgram)
        .ask_person::<trust::Trust>()
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
        let beak = Beak::new().unwrap();
        let missing = Siphon::builder(&beak, testing::setup(Host::default()))
            .unwrap()
            .start()
            .unwrap_err();
        assert!(missing.reason.contains("found 0"));
        let duplicate = Siphon::builder(&beak, testing::setup(Host::default()))
            .unwrap()
            .extension(Base)
            .extension(Base)
            .start()
            .unwrap_err();
        assert!(duplicate.reason.contains("found 2"));
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
            let beak = Beak::new().unwrap();
            let mut building = Siphon::builder(&beak, testing::setup(Host::default())).unwrap().extension(Base);
            for (owner, prompts) in registrations {
                building.builder.registrar(owner).prompts(prompts);
            }
            let error = building.start().unwrap_err();
            assert_eq!(error.status, beak::Status::Invalid);
            assert!(error.reason.contains("prompts"));
        }
    }
}
