//! Construction, extension registration and execution startup.

use std::collections::HashMap;
use std::sync::Arc;

use octoii_ai::catalogue::Model;
use octoii_ai::host::Credentials;
use octoii_ai::models::{Host as Network, Models};
use octoii_siphon_lib::{Asked, Runtime, RuntimeHandle, Status};

use super::{HostCaller, Octoii, Setup};
use crate::agent::compaction::{AwaitCompaction, answer_compaction};
use crate::agent::{
    CORE, CompactFiber, DelegatedFiber, Gather, ModelRequest, ResumeFiber, Sources, ThreadFiber, TitleFiber, TurnFiber, answer_gather,
    answer_model, answer_review,
};
use crate::extension::{Caller, Extension, Registrar};
use crate::machine::Host;
use crate::protocol::Scope;
use crate::threads::{Threads, preparation};
use crate::tools::{
    Asks, EditTool, Exec, ExecuteTool, GlobTool, GrepTool, Permission, ReadTool, Review, ScriptFiber, ShellTool, Tools, WriteTool,
    answer_exec,
};
use crate::{connection, extension, providers, trust};

/// Octoii being built: what its extensions add to, and what it is started with.
pub struct Building {
    builder: extension::Builder,
    runtime: Runtime,
    sign_in: providers::SignInSetup,
    credentials: Arc<dyn Credentials>,
    network: Network,
    catalogue: Vec<Model>,
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
            crate::protocol::octoii::set(),
            crate::protocol::octoii::Method::read,
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
            tools,
            owners,
            tellers: Sources::ordered(tellers),
            natives,
            models: Arc::clone(&models),
        };
        let registry = Arc::new(registry);
        let execution = register_fibers_and_handlers(execution, handle, &machine, sources.clone(), writables, prompts, &registry)?;
        let siphon = Octoii {
            machine,
            threads: Threads::new(runtime.runtime_handle()),
            runtime,
            models,
            credentials,
            providers: hosts,
            sign_in: Arc::new(sign_in),
            sign_ins: providers::SignIns::start(),
            registry,
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
    writables: Vec<crate::agent::Writable>,
    prompts: crate::agent::Prompts,
    registry: &Arc<extension::Registry>,
) -> octoii_siphon_lib::Result<octoii_siphon_lib::Builder> {
    let executor =
        tokio::runtime::Handle::try_current().map_err(|error| octoii_siphon_lib::Error::new(Status::Invalid, error.to_string()))?;
    preparation::register(&mut runtime, &executor, &state, machine, registry)?;
    let models = Arc::clone(&sources.models);
    let tools = sources.tools.clone();
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
mod tests {
    use super::*;
    use crate::testing;

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
