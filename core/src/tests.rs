//! Octoii's own tests, of its parts as the crate holds them: the accounts, the settings, the threads, the machine, the
//! tools and the calls weighed, the agent's turns, renders and compaction, a client's connection, and a live model, run
//! only when asked.

mod accounts;
mod agent;
mod calls;
mod coding;
mod compaction;
mod connection;
mod execute;
mod files;
mod live;
mod namespace;
mod permissions;
mod render;
mod settings;
mod threads;

use octoii_siphon_lib::{Asked, Changes, Ending, Fiber, Request, Run, Runtime, RuntimeHandle, Wait};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use crate::machine::{Base, Host};
use crate::threads::preparation::{InspectWorkspace, PrepareWorkspace};
use crate::threads::{Start, StartRequest, StartState, StartWorkspace};
use crate::tools::{Context, permissions::Writes};
use crate::volume::Volume;
use tokio::sync::{mpsc, oneshot};

struct WithRuntime<T> {
    owner: Runtime,
    value: T,
}

impl<T> WithRuntime<T> {
    fn new(owner: Runtime, make: impl FnOnce(RuntimeHandle) -> T) -> Self {
        let value = make(owner.runtime_handle());
        Self { owner, value }
    }

    fn runtime(&self) -> &Runtime {
        &self.owner
    }
}

impl<T> std::ops::Deref for WithRuntime<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

const CONTEXT_KEY: &str = "tool-context";

#[derive(Serialize, Deserialize)]
struct ContextRequest;

impl Request for ContextRequest {
    const NAME: &'static str = "test.tool-context";
    type Answer = ();
}

struct ContextFiber;

impl Fiber for ContextFiber {
    const KIND: &'static str = "test.tool-context";
    type Input = ();
    type Checkpoint = ();
    type Output = ();

    fn run(&self, run: &mut Run<'_>, (): (), checkpoint: Option<()>) -> anyhow::Result<Ending<(), ()>> {
        if checkpoint.is_none() {
            run.request(CONTEXT_KEY, &ContextRequest)?;
        }
        Ok(Ending::Wait((), Wait::new().mail(CONTEXT_KEY)))
    }
}

/// Uses an active SDK request so tests exercise the same command authority as tools.
pub(crate) async fn tool_context(directory: Option<&Path>, workspace: PathBuf, writes: Writes) -> (Runtime, Context) {
    let (send, receive) = tokio::sync::oneshot::channel();
    let send = Mutex::new(Some(send));
    let runtime = crate::testing::start_runtime(directory, |builder, state| {
        let runtime = state.clone();
        builder
            .fiber(ContextFiber)?
            .handle(&tokio::runtime::Handle::current(), move |asked: Asked<ContextRequest>| {
                let context = asked.commands().map(|request_commands| Context {
                    thread: "t1".into(),
                    call: "c1".into(),
                    base: Base {
                        workspace: workspace.clone(),
                        home: None,
                    },
                    writes,
                    places: Vec::new(),
                    host: Host::default(),
                    runtime: runtime.clone(),
                    request_commands,
                    cancelled: tokio_util::sync::CancellationToken::new(),
                });
                if let Some(send) = send.lock().unwrap().take() {
                    let _ = send.send(context);
                }
                std::future::pending::<Result<(), octoii_siphon_lib::Unanswered>>()
            })?;
        Ok(())
    })
    .expect("a test runtime");
    let mut changes = Changes::new().expect("a batch");
    changes.start::<ContextFiber>(None, CONTEXT_KEY, &()).expect("a fixture fiber");
    runtime.commit(changes).await.expect("started");
    let context = tokio::time::timeout(Duration::from_secs(10), receive)
        .await
        .expect("request delivered")
        .unwrap()
        .unwrap();
    (runtime, context)
}

struct WorkspaceDelivery<A> {
    request: StartRequest,
    answer: oneshot::Sender<Result<A, octoii_siphon_lib::Unanswered>>,
}

struct StartupRequests {
    preparing: mpsc::UnboundedReceiver<WorkspaceDelivery<StartWorkspace>>,
    inspecting: mpsc::UnboundedReceiver<WorkspaceDelivery<String>>,
}

impl StartupRequests {
    fn register(builder: &mut octoii_siphon_lib::Builder) -> crate::volume::Result<Self> {
        let executor = tokio::runtime::Handle::current();
        let (prepare, preparing) = mpsc::unbounded_channel();
        let (inspect, inspecting) = mpsc::unbounded_channel();
        builder
            .handle(&executor, move |asked: Asked<PrepareWorkspace>| {
                deliver_workspace(&prepare, asked.payload.0)
            })?
            .handle(&executor, move |asked: Asked<InspectWorkspace>| {
                deliver_workspace(&inspect, asked.payload.0)
            })?;
        Ok(Self { preparing, inspecting })
    }

    async fn prepare(&mut self) -> WorkspaceDelivery<StartWorkspace> {
        tokio::time::timeout(Duration::from_secs(10), self.preparing.recv())
            .await
            .expect("a preparation request in time")
            .expect("a preparation request")
    }

    async fn inspect(&mut self) -> WorkspaceDelivery<String> {
        tokio::time::timeout(Duration::from_secs(10), self.inspecting.recv())
            .await
            .expect("an inspection request in time")
            .expect("an inspection request")
    }
}

fn deliver_workspace<A>(
    deliveries: &mpsc::UnboundedSender<WorkspaceDelivery<A>>,
    request: StartRequest,
) -> impl Future<Output = Result<A, octoii_siphon_lib::Unanswered>> + use<A> {
    let (answer, receive) = oneshot::channel();
    deliveries.send(WorkspaceDelivery { request, answer }).expect("a delivery observer");
    async move { receive.await.expect("a scripted answer") }
}

async fn startup_until(volume: &Volume, id: &str, expected: impl Fn(&StartState) -> bool) -> Start {
    let mut commits = volume.runtime.commits().expect("a commit watch");
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let start = volume.thread_start(id).expect("a thread").expect("a startup gate");
            if expected(&start.state) {
                return start;
            }
            commits.next().await.unwrap().expect("the runtime runs");
        }
    })
    .await
    .expect("the startup state changed")
}
