//! The commands an extension runs on the machine — a namespace opened for its work, a command started in one, a
//! coprocess — through the kernel, which it is never given: the kernel writes the volume as well.

use octoii_siphon_lib::{Command, CommandEvents, Namespace, RuntimeHandle, Spec};

use crate::machine::namespace::{Host, Machine, Writes};
use crate::machine::paths::Base;

/// The machine's commands, as an extension runs them.
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

    /// The command `spec` started at `time` in `namespace`, recorded under `under`: it, and its events.
    ///
    /// # Errors
    /// Beak's failure to start it.
    pub fn spawn(
        &self,
        time: i64,
        under: &[u8],
        namespace: &Namespace,
        spec: &Spec,
    ) -> octoii_siphon_lib::Result<(Command, CommandEvents)> {
        self.runtime.spawn(time, under, namespace, spec)
    }

    /// A new id, minted at `time`.
    ///
    /// # Errors
    /// Beak's failure.
    pub fn new_id(&self) -> octoii_siphon_lib::Result<String> {
        self.runtime.new_id()
    }
}
