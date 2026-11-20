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
use crate::tools::{Context, permissions::Writes};

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
