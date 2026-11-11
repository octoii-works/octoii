//! Siphon's protocol, as a client speaks it over its WebSocket — the version agreed as the socket opens, by its
//! subprotocol `octoii.1`: Siphon's welcome first; then each call the client makes, by its id, its method and the
//! method's params, answered once by that id with its result or the error it was refused with; and the events of each
//! subscription the client holds, as what it follows changes — a thread's entries and status as they come, the list of
//! threads, the settings. A subscription that falls behind what its client reads is told it lagged, where it stood,
//! and is to be made again from there. Its types are the threads', the settings' and the vocabulary's own, so what is
//! sent is what Siphon keeps; `typescript` describes them all for the client.

mod events;
mod methods;
mod typescript;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use events::{Event, Settings, Topic};
pub use methods::*;
pub use typescript::typescript;

/// The subprotocol a client's WebSocket asks for, which names the protocol's version.
pub const SUBPROTOCOL: &str = "octoii.1";

/// A call a client makes: its id, by which its answer comes, and its method with that method's params.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Call {
    pub id: String,
    #[serde(flatten)]
    pub method: Method,
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

/// The machine a client reached: its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Machine {
    pub name: String,
}
