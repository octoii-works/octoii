//! Octoii, the Agent host on Siphon.

pub mod access;
pub(crate) mod accounts;
pub mod agent;
pub mod connection;
mod extension;
mod host;
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

pub use extension::{Caller, Extension, Full, Lagged, Registrar, Sink, Subscription};
pub use host::{Building, HostCaller, Octoii, Setup};
pub use kept::{Ends, Kept};
