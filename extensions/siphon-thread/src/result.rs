//! The `thread` tool's business results, independent of the words its model reads.

use std::collections::BTreeMap;

use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use siphon::threads::{Listed, Turn};
use siphon::tools::Told;

/// What a successful `thread` call returned or requested.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ThreadResult {
    List {
        threads: Vec<ThreadSummary>,
        offset: usize,
        total: usize,
        next: Option<usize>,
    },
    Search {
        hits: Vec<SearchHit>,
        offset: usize,
        total: usize,
        next: Option<usize>,
    },
    Read {
        thread: String,
        entries: Vec<ConversationEntry>,
        /// Read older entries with this value as `before`.
        before: Option<String>,
        /// Read newer entries with this value as `after`.
        after: Option<String>,
    },
    Start {
        thread: String,
        delivery: String,
        report: bool,
    },
    Send {
        thread: String,
        delivery: String,
        report: bool,
    },
    /// A request to stop the thread's turn; its completion is not awaited.
    Stop {
        thread: String,
    },
}

impl ThreadResult {
    pub(crate) fn told(&self, content: Vec<InputContent>) -> Told {
        Told {
            structured_content: Some(serde_json::json!(self)),
            ..Told::from(content)
        }
    }
}

/// A listed thread, without display placeholders for an unset title or project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ThreadSummary {
    pub id: String,
    pub status: ThreadStatus,
    /// Last activity, in milliseconds since the epoch.
    pub active_at: i64,
    pub project: Option<String>,
    pub title: Option<String>,
    pub parent: Option<String>,
    pub archived: bool,
    pub current: bool,
}

impl ThreadSummary {
    pub(crate) fn of(row: &Listed, own: &str) -> Self {
        Self {
            id: row.id.clone(),
            status: ThreadStatus::of(row),
            active_at: crate::tool::active_at(row),
            project: row.project.clone(),
            title: row.config.title.clone(),
            parent: row.parent.as_ref().map(|parent| parent.thread.clone()),
            archived: row.marks.archived,
            current: row.id == own,
        }
    }
}

/// How a listed thread stands, including pending input and requests to the person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThreadStatus {
    Asking,
    Working,
    New,
    Replied,
    Failed,
    Stopped,
}

impl ThreadStatus {
    pub(crate) fn of(row: &Listed) -> Self {
        if !row.status.asks.is_empty() {
            return Self::Asking;
        }
        if row.status.turn == Turn::Running || !row.status.inbox.is_empty() {
            return Self::Working;
        }
        match row.status.turn {
            Turn::Created => Self::New,
            Turn::Running | Turn::Replied => Self::Replied,
            Turn::Failed => Self::Failed,
            Turn::Stopped => Self::Stopped,
        }
    }
}

/// A matching conversation entry and the excerpt around its first matching keyword.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchHit {
    pub thread: ThreadSummary,
    pub entry: String,
    /// Entry time, in milliseconds since the epoch, where its id carries one.
    pub at: Option<i64>,
    pub excerpt: String,
}

/// An entry visible to `read`; reasoning, system instructions and discarded replies are omitted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConversationEntry {
    pub id: String,
    /// Entry time, in milliseconds since the epoch, where its id carries one.
    pub at: Option<i64>,
    #[serde(flatten)]
    pub content: ConversationContent,
}

/// The business fields of a visible conversation entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConversationContent {
    Input {
        actor: String,
        content: Vec<ContentPart>,
    },
    Reply {
        items: Vec<ReplyItem>,
    },
    Result {
        call: String,
        status: ResultStatus,
        text: String,
        clipped: bool,
    },
    TurnEnd {
        outcome: TurnOutcome,
        reason: Option<String>,
    },
    Config {
        actor: String,
        changes: BTreeMap<String, SettingChange>,
    },
    Compaction {
        text: String,
    },
}

/// A message's text or a file reference, kept separate from its display label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    Image { file: ContentFile },
    File { file: ContentFile },
    ResourceLink { file: ContentFile, size: u64 },
}

/// A file kept by the thread, identified by its content reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContentFile {
    #[serde(rename = "ref")]
    pub reference: String,
    pub media_type: String,
    pub filename: Option<String>,
}

/// A visible part of a model reply; call arguments share the text view's character limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReplyItem {
    Message {
        content: Vec<ModelContent>,
    },
    Call {
        call_id: String,
        name: String,
        arguments: String,
        clipped: bool,
    },
    WebSearch,
}

/// Text the model wrote, including a refusal distinctly from an ordinary message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelContent {
    Text { text: String },
    Refusal { refusal: String },
}

/// A changed setting's values; their JSON types depend on the setting's name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SettingChange {
    pub from: serde_json::Value,
    pub to: serde_json::Value,
}

/// How a tool call ended in the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ResultStatus {
    Ok,
    Unavailable,
    InvalidArguments,
    ToolError,
    Rejected,
    Interrupted,
}

impl From<siphon::threads::CallStatus> for ResultStatus {
    fn from(status: siphon::threads::CallStatus) -> Self {
        match status {
            siphon::threads::CallStatus::Ok => Self::Ok,
            siphon::threads::CallStatus::Unavailable => Self::Unavailable,
            siphon::threads::CallStatus::InvalidArguments => Self::InvalidArguments,
            siphon::threads::CallStatus::ToolError => Self::ToolError,
            siphon::threads::CallStatus::Rejected => Self::Rejected,
            siphon::threads::CallStatus::Interrupted => Self::Interrupted,
        }
    }
}

/// How a conversation's turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcome {
    Replied,
    Failed,
    Stopped,
}
