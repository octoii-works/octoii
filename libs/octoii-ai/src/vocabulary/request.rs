//! A request of a model: what it reads, the tools it may call, and how it answers.

use serde::{Deserialize, Serialize};

use super::items::{Item, ReasoningEffort};

/// A tool a model may call: a function of the caller's, its arguments' JSON Schema given; or the provider's own web
/// search, which the provider runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Tool {
    Function {
        name: String,
        description: String,
        parameters: serde_json::Value,
    },
    WebSearch,
}

/// Which tools the model may call: as it sees fit, none, at least one, or the function named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolChoice {
    Auto,
    None,
    Required,
    Function { name: String },
}

/// How hard the model reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct ReasoningConfig {
    pub effort: ReasoningEffort,
}

/// An answer in JSON of a schema, by the schema's name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct Format {
    pub name: String,
    pub schema: serde_json::Value,
}

/// A request: the model by its reference, such as `anthropic/claude-opus-5-5`; the instructions at the start; the
/// conversation; the tools it may call and which; how hard it reasons; the JSON its answer is, where it is; the most it
/// writes; and the name of the conversation, by which a provider sends its requests to the same cache.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct Request {
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    pub input: Vec<Item>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<Format>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
}
