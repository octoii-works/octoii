//! The tools a thread's model calls. Each says what a call of it does to the machine — reads, edits, runs a command
//! line, acts through a tool that may do anything — which is weighed before it is exec'd: by its thread's mode and its
//! project's grants, a second model or the person deciding where they say; and gives what the model reads, as the
//! model's vocabulary holds a call's output, or refuses, saying why.

mod effect;
mod permissions;
mod tool;

pub use effect::{Effect, Line};
pub use permissions::{Bounds, Grants, Verdict, Writes, patterns_of, weigh};
pub use tool::{Context, Declared, Refusal, Tool, Tools};
