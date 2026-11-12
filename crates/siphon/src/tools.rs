//! The tools a thread's model calls. Each says what a call of it does to the machine — reads, edits, runs a command
//! line, acts through a tool that may do anything — which is weighed before it is exec'd: by its thread's mode and its
//! project's grants, a second model or the person deciding where they say; and gives what the model reads, as the
//! model's vocabulary holds a call's output, or refuses, saying why.

pub(crate) mod calls;
pub(crate) mod coding;
pub(crate) mod declarations;
pub(crate) mod effect;
pub(crate) mod execute;
pub(crate) mod outlet;
pub(crate) mod permissions;
pub(crate) mod plan;
pub(crate) mod projects;
pub(crate) mod requests;
pub(crate) mod shell;
pub(crate) mod told;
pub(crate) mod tool;

pub(crate) use calls::{Call, Calls, Setting, State};
pub(crate) use coding::{EditTool, GlobTool, GrepTool, ReadTool, WriteTool};

pub(crate) use execute::{ExecuteTool, ScriptProgram};
pub(crate) use outlet::answer_exec;

pub(crate) use plan::{Awaited, Primitives, Step};
pub(crate) use requests::{Asks, Exec, Judgment, Permission, Review};
pub(crate) use shell::ShellTool;
pub(crate) use told::{INSTRUCTIONS_FILE, Instructions, Structured};

pub use crate::threads::CallStatus;
pub use declarations::declarations;
pub use effect::Effect;
/// What extensions and hosts use of it.
pub use plan::{Plan, Report, Spawned};
pub use projects::{Place, sha256_hex};
pub use told::Told;
pub use tool::{Context, Declared, Refusal, Tool, Tools, Toolset};
