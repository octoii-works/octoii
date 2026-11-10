//! The units of a conversation: messages, function calls with their outputs, reasoning, the web searches a provider
//! runs, and changes of the reasoning effort from where they stand.

use serde::{Deserialize, Serialize};

use super::content::{InputContent, OutputContent};

/// Where an item a model writes stands: still being written, written whole, or cut off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    InProgress,
    Completed,
    Incomplete,
}

/// Which model wrote an item, by its reference, and what its provider attached to it that no field holds — a
/// signature on a call, say — an object of objects, each under its provider's id, which goes back to that provider
/// alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_metadata: Option<serde_json::Value>,
}

/// A message: the person's, a developer's or the system's, which update the instructions from where they stand; or
/// the model's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Message {
    User {
        content: Vec<InputContent>,
    },
    Developer {
        content: Vec<InputContent>,
    },
    System {
        content: Vec<InputContent>,
    },
    Assistant {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        content: Vec<OutputContent>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<ItemStatus>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<Origin>,
    },
}

/// A model's call of a function by name, its arguments JSON text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCall {
    /// The item's id: its provider's, carried as it came, as every id a provider gives is its data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The call's id, which its output carries: its provider's, or one minted where the provider left it empty or gave
    /// it to another call. A caller that names the call by an id of its own keeps this one beside it: the conversation
    /// goes by this one, and a provider takes back only ids of its own.
    pub call_id: String,
    pub name: String,
    pub arguments: String,
    /// `incomplete` for a call cut off, whose arguments may be cut and which is never run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ItemStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
}

/// What a function call gave back to the model, and whether it failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCallOutput {
    pub call_id: String,
    pub output: Vec<InputContent>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
}

/// What a model thought before it answered, as far as its provider shows it, with what it needs to go on from it —
/// which only the model that wrote it reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reasoning {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub summary: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<String>,
    /// The reasoning as its provider keeps it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    /// What the provider signed the reasoning with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ItemStatus>,
    pub origin: Origin,
}

/// Where a web search a provider runs stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchStatus {
    InProgress,
    Searching,
    Completed,
    Failed,
}

/// What a web search did: searched, opened a page, or looked for a pattern in one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSearchAction {
    Search {
        query: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        sources: Vec<String>,
    },
    OpenPage {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    FindInPage {
        url: String,
        pattern: String,
    },
}

/// A web search the provider ran, whichever provider ran it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebSearchCall {
    pub id: String,
    pub status: WebSearchStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<WebSearchAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
}

/// How hard a model reasons, from `none` to `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

/// A change of the reasoning effort from where it stands, so what a window began with is never changed within it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigurationUpdate {
    pub effort: ReasoningEffort,
}

/// An item of a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Item {
    Message(Message),
    FunctionCall(FunctionCall),
    FunctionCallOutput(FunctionCallOutput),
    Reasoning(Reasoning),
    WebSearchCall(WebSearchCall),
    ConfigurationUpdate(ConfigurationUpdate),
}
