//! Octoii's HTTP commands and queries, and its WebSocket subscriptions. The method sets declared by `methods!`
//! supply their types and handlers. `typescript` generates the client declarations from the assembled sets.

pub mod octoii;
mod set;
mod typescript;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use set::{Set, Subscriptions};
pub use typescript::{Declarations, typescript};
#[doc(hidden)]
pub use {serde, serde_json, ts_rs};

pub const CALL_PATH: &str = "/api/v1/call";
pub const WATCH_PATH: &str = "/api/v1/watch";
pub const SUBPROTOCOL: &str = "octoii.watch.1";

/// A command or query sent over HTTP.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Its HTTP response, with the status set by the carrier.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum CommandReply {
    Result { result: serde_json::Value },
    Error { error: Error },
}

/// Why a call was refused, or what it asked could not be done: a code a client may act on, and words for the person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Error {
    pub code: String,
    pub message: String,
}

/// The response to a subscription control frame, correlated by its id.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Answer {
    Result { id: String, result: serde_json::Value },
    Error { id: Option<String>, error: Error },
}

/// What a session lets its client do: each scope opens some of the methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum Scope {
    #[serde(rename = "threads.read")]
    ThreadsRead,
    #[serde(rename = "threads.operate")]
    ThreadsOperate,
    #[serde(rename = "files.read")]
    FilesRead,
    #[serde(rename = "git.operate")]
    GitOperate,
    #[serde(rename = "terminals.operate")]
    TerminalsOperate,
    #[serde(rename = "browser.operate")]
    BrowserOperate,
    #[serde(rename = "providers.manage")]
    ProvidersManage,
    #[serde(rename = "access.read")]
    AccessRead,
    #[serde(rename = "access.write")]
    AccessWrite,
}

/// The machine a client reached: its name; the folder a thread of no folder of its own works in; and where the paths
/// its threads name lie that a client says by their places' names — the person's home folder, the folder of the
/// threads' scratchpads, a thread's by its id, and the folder of the person's memory, a project's in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Machine {
    pub name: String,
    pub workspace: String,
    pub home: Option<String>,
    pub scratchpads: Option<String>,
    pub memory: Option<String>,
}

impl Machine {
    /// The machine `host` gives, by the name `name`.
    #[must_use]
    pub fn of(name: String, host: &crate::machine::Host) -> Self {
        let shown = |path: &std::path::Path| path.to_string_lossy().into_owned();
        Self {
            name,
            workspace: shown(&host.workspace_of(None)),
            home: host.home.as_deref().map(shown),
            scratchpads: host.scratchpads.as_deref().map(shown),
            memory: host.memory.as_deref().map(shown),
        }
    }
}

/// An extension of the machine's: its name, which settings switch it by; the words it is shown by; and a line of what
/// it brings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ExtensionListed {
    pub name: String,
    pub title: String,
    pub description: String,
}

/// Nothing: what a method takes or answers where it takes or answers nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Nothing {}

/// A subscription made: its id, which its events carry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Subscribed {
    pub subscription: String,
}

/// A subscription to let go of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Unsubscribe {
    pub subscription: String,
}

/// The welcome, a request to resubscribe after lag, or the end of the session's access.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ConnectionEvent {
    Welcome {
        session: String,
        scopes: Vec<Scope>,
        machine: Machine,
        extensions: Vec<ExtensionListed>,
    },
    Lagged {
        subscription: String,
        #[ts(type = "Topic")]
        topic: serde_json::Value,
    },
    Revoked,
}
