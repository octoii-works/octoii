//! The commands an extension runs on the machine — a namespace opened for its work, a command started in one, a
//! coprocess — through the kernel, which it is never given: the kernel writes the volume as well.

use octoii_siphon_lib::{Command, CommandEvents, Namespace, RequestCommands, RuntimeHandle, Spec};

use crate::machine::namespace::{Host, Machine, Writes};
use crate::machine::paths::Base;

/// A command launcher bound to a host or a request.
pub trait Launcher: std::fmt::Debug + Sync {
    /// Starts `spec` in `namespace` with the launcher's ownership.
    ///
    /// # Errors
    /// Siphon could not start the command.
    fn spawn(&self, namespace: &Namespace, spec: &Spec) -> octoii_siphon_lib::Result<(Command, CommandEvents)>;
}

/// The machine's host-owned commands, which can outlive a tool request.
#[derive(Debug, Clone)]
pub struct Commands {
    pub(crate) runtime: RuntimeHandle,
}

impl Commands {
    /// The machine `host` describes, opened for work at `base` that writes as `writes` says.
    ///
    /// # Errors
    /// Beak's failure to open its namespace.
    pub fn open(&self, host: &Host, base: Base, writes: &Writes) -> octoii_siphon_lib::Result<Machine> {
        Machine::open(&self.runtime, host, base, writes, None)
    }

    /// Starts `spec` in `namespace` under the runtime root.
    ///
    /// # Errors
    /// Beak's failure to start it.
    pub fn spawn(&self, namespace: &Namespace, spec: &Spec) -> octoii_siphon_lib::Result<(Command, CommandEvents)> {
        self.runtime.spawn(None, namespace, spec)
    }

    /// A new id minted by the volume.
    ///
    /// # Errors
    /// Beak's failure.
    pub fn new_id(&self) -> octoii_siphon_lib::Result<String> {
        self.runtime.new_id()
    }
}

impl Launcher for Commands {
    fn spawn(&self, namespace: &Namespace, spec: &Spec) -> octoii_siphon_lib::Result<(Command, CommandEvents)> {
        self.spawn(namespace, spec)
    }
}

impl Launcher for RequestCommands {
    fn spawn(&self, namespace: &Namespace, spec: &Spec) -> octoii_siphon_lib::Result<(Command, CommandEvents)> {
        self.spawn(namespace, spec)
    }
}
