//! MCP servers as Siphon's extension, whose tools a thread's model calls as its own: configured by the person in the
//! Octoii Home's `mcp.json`, and by a project in its `.mcp.json`, which runs once the person trusts it; each a command
//! the machine runs or a URL reached over streamable HTTP, connected as first needed and shared; their catalog told the
//! model before each turn, the person asked first where a project's configuration is not yet decided on.

mod config;
mod extension;
pub mod protocol;
mod servers;
pub mod trust;

pub use config::{Configured, Launch, OWN_FILE, PROJECT_FILE, Server, parse};
pub use extension::McpExtension;
pub use servers::{Catalog, Found, IDLE, ProjectServers, ServerListed, Servers, ServersListed};
pub use trust::{ShownServer, Trust, TrustProgram, Trusted};
