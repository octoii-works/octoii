//! The MCP servers' methods, as their extension adds them to the protocol: the servers a thread working in a folder
//! would use, each as it stands.

use serde::{Deserialize, Serialize};
use siphon::protocol::Scope;
use ts_rs::TS;

use crate::ServersListed;

siphon::methods! {
    /// The servers a thread working in a folder would use — the person's own alone, where none is named.
    "servers.list" ListServers(ListServers) -> ServersListed, Some(Scope::ThreadsRead);
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ListServers {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub workspace: Option<String>,
}
