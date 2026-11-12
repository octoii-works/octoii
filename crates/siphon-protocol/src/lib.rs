//! Siphon's protocol, as a client speaks it over its WebSocket — the version agreed as the socket opens, by its
//! subprotocol `octoii.1`: the welcome first; then each call the client makes, by its id, its method and the method's
//! params, answered once by that id with its result or the error it was refused with; and the events of each
//! subscription the client holds, as what it follows changes. A subscription that falls behind what its client reads
//! is let go and told it lagged, with the topic to subscribe to again from where it stood.
//!
//! The methods come in sets — Siphon's own, `siphon`, and each extension's, declared by `methods!` — which a host
//! assembles; `typescript` describes the protocol the sets make together, its types each set's own, so what is sent is
//! what Siphon and its extensions keep.

mod set;
pub mod siphon;
mod typescript;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use set::{Set, Subscriptions};
pub use typescript::{Declarations, typescript};
#[doc(hidden)]
pub use {serde, serde_json, ts_rs};

/// The subprotocol a client's WebSocket asks for, which names the protocol's version.
pub const SUBPROTOCOL: &str = "octoii.1";

/// A call a client makes: its id, by which its answer comes, its method's name and the method's params.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Call {
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Why a call was refused, or what it asked could not be done: a code a client may act on, and words for the person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Error {
    pub code: String,
    pub message: String,
}

/// An answer to a call: its id and its method's result, or the error it was refused with — a null id where the call's
/// id could not be read.
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
    pub fn of(name: String, host: &siphon_machine::Host) -> Self {
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

/// An event of the connection's own: the welcome a client is sent first — its session, what it may do, the machine
/// it reached and its extensions — or that a subscription fell behind and was let go, with the topic to subscribe to again from where it
/// stood.
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
}
