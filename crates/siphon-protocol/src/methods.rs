//! The methods a client calls, each by its name: the params it takes and the result it answers with — declared once,
//! as a call reads them in Rust and as the client's TypeScript names them.

use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use siphon_settings::SettingsChange;
use siphon_threads::{Config, ConfigChange, Entry, Listed, MarksChange};
use ts_rs::TS;

use crate::Scope;
use crate::events::Topic;

/// The methods: each its name, the variant of `Method` it is read as, its params, its result and the scope a session
/// needs to call it — none where its params decide, as a subscription's topic does.
macro_rules! methods {
    ($($(#[$doc:meta])* $name:literal $variant:ident($params:ty) -> $result:ty, $scope:expr;)*) => {
        /// A call's method, with its params.
        #[derive(Debug, Clone, PartialEq, Deserialize)]
        #[serde(tag = "method", content = "params")]
        pub enum Method {
            $($(#[$doc])* #[serde(rename = $name)] $variant($params),)*
        }

        impl Method {
            /// Its name, as a call names it.
            #[must_use]
            pub fn name(&self) -> &'static str {
                match self {
                    $(Self::$variant(_) => $name,)*
                }
            }
        }

        impl Method {
            /// The scope a session needs to call it; none where its params decide.
            #[must_use]
            pub fn scope(&self) -> Option<Scope> {
                match self {
                    $(Self::$variant(_) => $scope,)*
                }
            }
        }

        /// Each method's name, and the TypeScript names of its params and of its result.
        pub(crate) fn signatures(cfg: &ts_rs::Config) -> Vec<(&'static str, String, String)> {
            vec![$(($name, <$params as TS>::name(cfg), <$result as TS>::name(cfg)),)*]
        }

        /// Visits each type the methods' params and results name.
        pub(crate) fn visit(visitor: &mut impl ts_rs::TypeVisitor) {
            $(visitor.visit::<$params>(); visitor.visit::<$result>();)*
        }
    };
}

methods! {
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
    /// A subscription to a topic: its id, which its events carry.
    "subscribe" Subscribe(Subscribe) -> Subscribed, None;
    /// A subscription let go.
    "unsubscribe" Unsubscribe(Unsubscribe) -> Nothing, None;
}

/// Nothing: what a method takes or answers where it takes or answers nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Nothing {}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Subscribe {
    pub topic: Topic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Subscribed {
    pub subscription: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Unsubscribe {
    pub subscription: String,
}
