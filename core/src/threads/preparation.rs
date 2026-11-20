//! Workspace preparation at the boundary between a thread's durable requests and its registered extensions.

use std::sync::Arc;

use futures::future::BoxFuture;
use octoii_siphon_lib::{Asked, Builder, Request, RequestCommands, RuntimeHandle, Unanswered};
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;

use crate::extension::Registry;
use crate::machine::{Base, Host, Launcher, Machine, Writes};

use super::{StartRequest, StartWorkspace};

/// The machine available to one delivery of a workspace preparation request.
#[derive(Debug)]
pub struct PreparationContext {
    host: Host,
    runtime: RuntimeHandle,
    commands: RequestCommands,
    cancelled: CancellationToken,
}

impl PreparationContext {
    /// Launches commands owned by this exact request.
    #[must_use]
    pub fn commands(&self) -> &dyn Launcher {
        &self.commands
    }

    /// The machine as the host describes it.
    #[must_use]
    pub fn host(&self) -> &Host {
        &self.host
    }

    /// Opens the machine with the given base and writable folders.
    ///
    /// # Errors
    /// Siphon could not open or mount the namespace.
    pub fn open(&self, base: Base, writes: &Writes) -> crate::volume::Result<Machine> {
        Machine::open(&self.runtime, &self.host, base, writes, None)
    }

    /// Signals that this request's execution should stop.
    #[must_use]
    pub fn cancelled(&self) -> &CancellationToken {
        &self.cancelled
    }
}

/// An extension that prepares the workspace specified by an accepted startup request.
pub trait WorkspacePreparer: Send + Sync {
    /// Performs the preparation and returns the directory to bind to the thread.
    fn prepare<'a>(&'a self, context: &'a PreparationContext, request: &'a StartRequest) -> BoxFuture<'a, anyhow::Result<StartWorkspace>>;

    /// Describes the effects of an interrupted preparation using read-only operations.
    fn inspect<'a>(&'a self, context: &'a PreparationContext, request: &'a StartRequest) -> BoxFuture<'a, anyhow::Result<String>>;
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PrepareWorkspace(pub StartRequest);

impl Request for PrepareWorkspace {
    const NAME: &'static str = "siphon.workspace.prepare";
    type Answer = StartWorkspace;
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct InspectWorkspace(pub StartRequest);

impl Request for InspectWorkspace {
    const NAME: &'static str = "siphon.workspace.inspect";
    type Answer = String;
}

pub(crate) fn register(
    builder: &mut Builder,
    executor: &Handle,
    runtime: &RuntimeHandle,
    host: &Host,
    registry: &Arc<Registry>,
) -> crate::volume::Result<()> {
    let handlers = Arc::new(Handlers {
        runtime: runtime.clone(),
        host: host.clone(),
        registry: Arc::clone(registry),
    });
    builder
        .handle(executor, {
            let handlers = Arc::clone(&handlers);
            move |asked: Asked<PrepareWorkspace>| {
                let handlers = Arc::clone(&handlers);
                async move {
                    handlers
                        .answer(&asked, &asked.payload.0, |preparer, context, request| {
                            preparer.prepare(context, request)
                        })
                        .await
                }
            }
        })?
        .handle(executor, move |asked: Asked<InspectWorkspace>| {
            let handlers = Arc::clone(&handlers);
            async move {
                handlers
                    .answer(&asked, &asked.payload.0, |preparer, context, request| {
                        preparer.inspect(context, request)
                    })
                    .await
            }
        })?;
    Ok(())
}

struct Handlers {
    runtime: RuntimeHandle,
    host: Host,
    registry: Arc<Registry>,
}

impl Handlers {
    async fn answer<R: Request>(
        &self,
        asked: &Asked<R>,
        request: &StartRequest,
        execute: impl for<'a> FnOnce(
            &'a dyn WorkspacePreparer,
            &'a PreparationContext,
            &'a StartRequest,
        ) -> BoxFuture<'a, anyhow::Result<R::Answer>>,
    ) -> Result<R::Answer, Unanswered> {
        if asked.stop.is_requested() {
            return Err(Unanswered::Interrupted);
        }
        let snapshot = self.runtime.snapshot().map_err(failed)?;
        let preparer = self
            .registry
            .workspace_preparer(&snapshot, &request.owner)
            .map_err(failed)?
            .ok_or_else(|| Unanswered::Failed(format!("workspace preparation is unavailable for {}", request.owner)))?;
        drop(snapshot);
        let commands = asked.commands().map_err(failed)?;
        let cancelled = CancellationToken::new();
        let _cancel_on_drop = cancelled.clone().drop_guard();
        let context = PreparationContext {
            host: self.host.clone(),
            runtime: self.runtime.clone(),
            commands,
            cancelled,
        };
        let execution = async { execute(preparer, &context, request).await };
        tokio::pin!(execution);
        tokio::select! {
            biased;
            () = asked.stop.requested() => {
                context.cancelled.cancel();
                let _ = execution.await;
                Err(Unanswered::Interrupted)
            }
            result = &mut execution => result.map_err(failed),
        }
    }
}

fn failed(error: impl std::fmt::Display) -> Unanswered {
    Unanswered::Failed(format!("{error:#}"))
}
