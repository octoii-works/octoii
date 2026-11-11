//! Siphon's own methods, events and topics: the threads — each made, changed, delivered to, stopped, deleted, answered
//! and read — and the settings; a subscription to a thread, to the list of threads, or to the settings.

use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use siphon_settings::{Environment, SettingsChange};
use siphon_threads::log::Cursor;
use siphon_threads::{Config, ConfigChange, Entry, Listed, MarksChange, Status};
use ts_rs::TS;

use crate::{Nothing, Scope};

crate::methods! {
    events: SiphonEvent, topics: SiphonTopic;
    /// The threads, each as a list shows it.
    "threads.list" ListThreads(Nothing) -> ThreadsListed, Some(Scope::ThreadsRead);
    /// A thread made as `config` says, once for its idempotency key `key`: the id it has.
    "threads.create" CreateThread(CreateThread) -> Created, Some(Scope::ThreadsOperate);
    /// How a thread runs changed, from its next model request.
    "threads.configure" ConfigureThread(ConfigureThread) -> Nothing, Some(Scope::ThreadsOperate);
    /// How the person keeps a thread changed.
    "threads.mark" MarkThread(MarkThread) -> Nothing, Some(Scope::ThreadsOperate);
    /// What a message gives a thread delivered to its inbox, once for its idempotency key: its delivery's id.
    "threads.deliver" Deliver(Deliver) -> Delivered, Some(Scope::ThreadsOperate);
    /// A delivery taken back, where its thread has not taken it yet: whether it was.
    "threads.withdraw" Withdraw(Withdraw) -> Withdrawn, Some(Scope::ThreadsOperate);
    /// A thread's turn stopped.
    "threads.stop" StopThread(OfThread) -> Nothing, Some(Scope::ThreadsOperate);
    /// A thread deleted, and its sub-threads with it.
    "threads.delete" DeleteThread(OfThread) -> Nothing, Some(Scope::ThreadsOperate);
    /// What a thread asks the person answered.
    "threads.answer" AnswerThread(AnswerThread) -> Nothing, Some(Scope::ThreadsOperate);
    /// A page of a thread's entries, the latest before an entry, or the latest of all.
    "threads.read" ReadThread(ReadThread) -> ThreadRead, Some(Scope::ThreadsRead);
    /// The settings changed: the environment's each given, the device's merged.
    "settings.set" SetSettings(SettingsChange) -> Nothing, Some(Scope::ThreadsOperate);
}

/// A thread, by its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OfThread {
    pub thread: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ThreadsListed {
    pub threads: Vec<Listed>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CreateThread {
    pub config: Config,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Created {
    pub thread: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConfigureThread {
    pub thread: String,
    pub change: ConfigChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MarkThread {
    pub thread: String,
    pub change: MarksChange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Deliver {
    pub thread: String,
    pub content: Vec<InputContent>,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Delivered {
    pub delivery: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Withdraw {
    pub thread: String,
    pub delivery: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Withdrawn {
    pub withdrawn: bool,
}

/// What a thread asks the person, by its request's key, answered with `answer`, which fits the schema the ask gives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AnswerThread {
    pub thread: String,
    pub key: String,
    pub answer: serde_json::Value,
}

/// A page of a thread's entries: at most `limit`, the latest before the entry `before`, or the latest of all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReadThread {
    pub thread: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub before: Option<String>,
    pub limit: u32,
}

/// The entries read, oldest first, and whether they reach the thread's first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ThreadRead {
    pub entries: Vec<Entry>,
    pub complete: bool,
}

/// What a client subscribes to of Siphon's: a thread — its entries from where the client stood, none for from the start, then its
/// status and what streams of its requests — the list of threads, or the settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SiphonTopic {
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

/// An event of a subscription to Siphon's, by the subscription's id: a thread's entries appended, with where its
/// reader then stands; its status as it changed; what streams of a request as its model answers, by its offset in the
/// request's stream; a thread of the list as it changed, or gone; the settings as they changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SiphonEvent {
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
}
