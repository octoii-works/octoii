//! Claude through the person's own Claude Code, as Siphon's plugin: a provider whose driver talks to `claude` over its
//! stream-json control protocol — Claude thinks, the thread acts: its tools an MCP server of each session's, their
//! calls held for the thread to make — the thread's log the master and Claude Code's sessions its replicas.

mod account;
pub mod conversation;
mod driver;
mod plugin;
pub mod process;
mod session;
pub mod store;

pub use account::{Account, Asked, Begin, Place, found, listed_tool};
pub use driver::{ClaudeCode, ID};
pub use plugin::ClaudeCodePlugin;
pub use session::{Session, Settled};
