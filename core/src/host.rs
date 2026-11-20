//! The running Octoii host and its product capabilities.

mod build;

use std::path::PathBuf;
use std::sync::Arc;

use octoii_ai::catalogue::Model;
use octoii_ai::host::Credentials;
use octoii_ai::models::{Models, Provider};
use octoii_ai::transport::Transport;
use octoii_siphon_lib::Runtime;

use crate::agent::{Sources, ThreadFiber, Told};
use crate::machine::Host;
use crate::threads::Threads;
use crate::{access, extension, providers};

pub use build::Building;

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
    pub(crate) runtime: Runtime,
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

    /// Whether the owner of a startup request has an enabled workspace preparer.
    ///
    /// # Errors
    /// The volume could not read the current environment settings.
    pub fn can_prepare_workspace(&self, owner: &str) -> crate::volume::Result<bool> {
        let snapshot = self.runtime.snapshot()?;
        Ok(self.registry.workspace_preparer(&snapshot, owner)?.is_some())
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

// The Thread startup capability uses the host's registered ThreadFiber.
impl crate::volume::Volume {
    /// Accepts a thread, its ordered initial input, fitted attachments and the caller's key in one commit.
    /// A thread with a preparation request waits for its workspace before executing.
    /// `new` holds the original request. The host's resolved current directory is bound at first acceptance.
    ///
    /// # Errors
    /// Invalid or reused input, a deleted thread, a planning failure, or a storage failure.
    pub async fn start_thread(
        &self,
        caller: &str,
        key: &str,
        new: &crate::threads::NewThread,
        content: Vec<crate::threads::InputPart>,
        directory: crate::threads::StartDirectory<'_>,
    ) -> anyhow::Result<String> {
        Ok(self.starting_threads().start(caller, key, new, content, directory).await?)
    }

    /// Reads the startup state; none for a normally created thread.
    ///
    /// # Errors
    /// Missing thread or unreadable state.
    pub fn thread_start(&self, id: &str) -> anyhow::Result<Option<crate::threads::Start>> {
        Ok(self.starting_threads().start_state(id)?)
    }

    /// Starts another preparation attempt from a failure; a replay returns the same attempt.
    ///
    /// # Errors
    /// Stale token, invalid transition, missing thread or storage failure.
    pub async fn retry_thread_start(&self, id: &str, token: &str) -> anyhow::Result<crate::threads::Start> {
        Ok(self
            .starting_threads()
            .start_command(id, token, crate::threads::start::Action::Retry { next: self.new_id()? })
            .await?)
    }

    /// Cancels this preparation attempt and prevents its result from releasing the input.
    ///
    /// # Errors
    /// Stale token, invalid transition, missing thread or storage failure.
    pub async fn cancel_thread_start(&self, id: &str, token: &str) -> anyhow::Result<crate::threads::Start> {
        Ok(self
            .starting_threads()
            .start_command(id, token, crate::threads::start::Action::Cancel)
            .await?)
    }

    fn starting_threads(&self) -> Threads<ThreadFiber> {
        crate::threads::Threads::new(self.runtime.clone())
    }
}
