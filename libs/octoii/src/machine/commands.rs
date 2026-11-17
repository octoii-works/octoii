//! The commands an extension runs on the machine — a namespace opened for its work, a command started in one, a
//! coprocess — through the kernel, which it is never given: the kernel writes the volume as well.

use beak::{Command, CommandEvents, Kernel, Namespace, Spec};

use crate::machine::namespace::{Host, Machine, Writes};
use crate::machine::paths::Base;

/// The machine's commands, as an extension runs them.
#[derive(Debug, Clone)]
pub struct Commands {
    pub(crate) kernel: Kernel,
}

impl Commands {
    /// The machine `host` describes, opened for work at `base` that writes as `writes` says.
    ///
    /// # Errors
    /// Beak's failure to open its namespace.
    pub fn open(&self, host: &Host, base: Base, writes: &Writes) -> beak::Result<Machine> {
        Machine::open(&self.kernel, host, base, writes, None)
    }

    /// The command `spec` started at `time` in `namespace`, recorded under `under`: it, and its events.
    ///
    /// # Errors
    /// Beak's failure to start it.
    pub fn spawn(&self, time: i64, under: &[u8], namespace: &Namespace, spec: &Spec) -> beak::Result<(Command, CommandEvents)> {
        self.kernel.spawn(time, under, namespace, spec)
    }

    /// A new id, minted at `time`.
    ///
    /// # Errors
    /// Beak's failure.
    pub fn new_id(&self, time: i64) -> beak::Result<String> {
        self.kernel.new_id(time)
    }
}
