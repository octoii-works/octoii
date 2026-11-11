//! What Siphon sends a client beside its answers: its welcome, and the events of each subscription it holds.

use serde::{Deserialize, Serialize};
use siphon_settings::Environment;
use siphon_threads::log::Cursor;
use siphon_threads::{Entry, Listed, Status};
use ts_rs::TS;

use crate::{Machine, Scope};

/// What a client subscribes to: a thread — its entries from where the client stood, none for from the start, then its
/// status and what streams of its requests — the list of threads, or the settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Topic {
    Thread {
        thread: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        from: Option<Cursor>,
    },
    Threads,
    Settings,
}

/// The settings, as a client is told of them: the environment's, which every client shares, and its device's own,
/// which Siphon keeps for it and reads none of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Settings {
    pub environment: Environment,
    pub device: serde_json::Value,
}

/// An event: the welcome a client is sent first — its session, what it may do, the machine it reached — or one of a
/// subscription's, by the subscription's id: a thread's entries appended, with where its reader then stands; its
/// status as it changed; what streams of a request as its model answers, by its offset in the request's stream; a
/// thread of the list as it changed, or gone; the settings as they changed; or that the subscription fell behind and
/// was let go — where it stood in a thread's log, for it to be made again from there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Welcome {
        session: String,
        scopes: Vec<Scope>,
        machine: Machine,
    },
    Entries {
        subscription: String,
        entries: Vec<Entry>,
        cursor: Cursor,
    },
    Status {
        subscription: String,
        status: Status,
    },
    Stream {
        subscription: String,
        request: String,
        offset: u64,
        data: serde_json::Value,
    },
    Thread {
        subscription: String,
        thread: Box<Listed>,
    },
    ThreadGone {
        subscription: String,
        thread: String,
    },
    Settings {
        subscription: String,
        settings: Settings,
    },
    Lagged {
        subscription: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        cursor: Option<Cursor>,
    },
}
