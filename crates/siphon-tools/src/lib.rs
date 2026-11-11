//! The tools a thread's model calls. Each says what a call of it does to the machine — reads, edits, runs a command
//! line, acts through a tool that may do anything — which is weighed before it is exec'd: by its thread's mode and its
//! project's grants, a second model or the person deciding where they say; and gives what the model reads, as the
//! model's vocabulary holds a call's output, or refuses, saying why.

mod calls;
mod coding;
mod declarations;
mod effect;
mod execute;
mod outlet;
mod permissions;
pub mod projects;
mod question;
mod requests;
mod shell;
mod tool;

pub use calls::{Call, Calls, Setting, State};
pub use coding::{EditTool, GlobTool, GrepTool, ReadTool, WriteTool};
pub use declarations::declarations;
pub use effect::{Effect, Line};
pub use execute::{ExecuteArgs, ExecuteTool, ScriptInput, ScriptOutput, ScriptProgram, ScriptState, Settled};
pub use outlet::{TEXT_CHARS_MAX, answer_exec};
pub use permissions::{Bounds, Grants, Verdict, Writes, patterns_of, weigh};
pub use question::{Asked, Choice, QuestionArgs, QuestionTool};
pub use requests::{Asks, Decision, Exec, Judgment, Permission, Review, Shown};
pub use shell::ShellTool;
pub use siphon_threads::CallStatus;
pub use tool::{Context, Declared, Refusal, Started, Tool, Tools, Toolset};
