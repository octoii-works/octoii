//! The MCP servers' methods, as their extension adds them to the protocol: the servers a thread working in a folder
//! would use, each as it stands; and the person's decision on a project's configuration of them.

use serde::{Deserialize, Serialize};
use siphon_protocol::{Nothing, Scope};
use ts_rs::TS;

use crate::ServersListed;

siphon_protocol::methods! {
    /// The servers a thread working in a folder would use — the person's own alone, where none is named.
    "servers.list" ListServers(ListServers) -> ServersListed, Some(Scope::ThreadsRead);
    /// Whether the person trusts a project's configuration of servers as it is, by its digest.
    "trust.decide" DecideTrust(DecideTrust) -> Nothing, Some(Scope::ThreadsOperate);
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ListServers {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DecideTrust {
    pub folder: String,
    pub digest: String,
    pub trusted: bool,
}
