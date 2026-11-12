//! Siphon's threads on Beak. Each is a directory `siphon.thread:<id>`: how it runs, and how the person keeps it, on its
//! labels and annotations; its inbox of what was delivered and not yet taken; its log in segments; and its main
//! process, which with the turn it runs writes the log — one of them at a time — taking the inbox into it in the order
//! it came, a change of how it runs logged where it took effect, from what it was to what it is.

mod config;
pub mod files;
pub mod idempotency;
pub mod inbox;
pub mod log;
mod marks;
mod operations;
mod status;
pub mod thread;
pub mod window;

pub use config::{Changed, Config, ConfigChange, ConfigChanged, Mode};
pub use log::{CallKey, CallStatus, Entry, Outcome, PERSON, SIPHON, Section};
pub use marks::{Marks, MarksChange};
pub use operations::{Error, MainInput, NewThread, Threads};
pub use status::{Ask, BEGAN_AT_ANNOTATION, ENDED_AT_ANNOTATION, Listed, Parent, Status, TURN_ANNOTATION, ThreadUsage, Turn, began, ended};
pub use window::Window;
