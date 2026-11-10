//! MCP servers, whose tools a thread's model calls as its own: configured by the person in the Octoii Home's
//! `mcp.json`, and by a project in its `.mcp.json`, which runs once the person trusts it; each a command the machine runs
//! or a URL reached over streamable HTTP, connected as first needed and shared.

mod config;
mod servers;
mod stdio;
pub mod trust;

pub use config::{Configured, Launch, OWN_FILE, PROJECT_FILE, Server, parse};
pub use servers::{Catalog, Found, IDLE, Place, Servers};
pub use trust::{ShownServer, Trust, TrustProgram, Trusted};
