//! Siphon's threads on Beak. Each is a directory `siphon.thread:<id>`: how it runs, and how the person keeps it, on its
//! labels and annotations; its inbox of what was delivered and not yet taken; its log in segments; and its main
//! process, which with the turn it runs writes the log — one of them at a time — taking the inbox into it in the order
//! it came, a change of how it runs logged where it took effect, from what it was to what it is.

pub(crate) mod config;
pub mod files;
pub(crate) mod idempotency;
pub(crate) mod inbox;
pub(crate) mod log;
pub(crate) mod marks;
pub(crate) mod operations;
pub(crate) mod status;
pub(crate) mod thread;
pub(crate) mod window;

pub(crate) use config::ConfigChange;
pub(crate) use log::{CallKey, Entry, Outcome, PERSON, SIPHON, Section};
pub(crate) use marks::MarksChange;
pub(crate) use operations::{Error, MainInput, Threads};
pub(crate) use status::{Listed, Parent, Status, TURN_ANNOTATION, ThreadUsage, Turn, began, ended};
pub(crate) use window::Window;

/// What extensions and hosts use of it.
pub use config::{Config, Mode};
pub use inbox::{Ended, Joins};
pub use log::CallStatus;
pub use operations::NewThread;
