//! Siphon's threads on Beak. Each is a directory `siphon.thread:<id>`: how it runs, and how the person keeps it, on its
//! labels and annotations; its inbox of what was delivered and not yet taken; its log in segments; and its main
//! process, the log's one writer, which takes the inbox into the log in the order it came — a change of how it runs
//! logged where it took effect, from what it was to what it is.

mod config;
pub mod files;
pub mod idempotency;
pub mod inbox;
pub mod log;
mod marks;
mod status;
pub mod thread;
mod threads;

pub use config::{Changed, Config, ConfigChange, ConfigChanged, Mode};
pub use marks::{Marks, MarksChange};
pub use status::{Ask, BEGAN_AT_ANNOTATION, ENDED_AT_ANNOTATION, Listed, Parent, Status, TURN_ANNOTATION, Turn, Usage};
pub use threads::{Error, MainInput, NewThread, Threads};
