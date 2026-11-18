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

use octoii_siphon_lib::{Builder, Runtime, RuntimeHandle};
use std::path::PathBuf;

fn open_runtime(builder: Builder, directory: Option<PathBuf>) -> octoii_siphon_lib::Result<Runtime> {
    match directory {
        Some(path) => builder.open(&path),
        None => builder.memory(),
    }
}

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
