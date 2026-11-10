//! What a request comes to: the response's status, what the model wrote, what it used, and why it failed or stopped
//! short.

use serde::{Deserialize, Serialize};

use super::items::Item;

/// Where a response stands: under way, or ended `completed`, `incomplete` or `failed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus {
    InProgress,
    Completed,
    Incomplete,
    Failed,
}

/// Why a response ended `incomplete`: at its output limit, filtered by the provider's policy, or cut off by its caller,
/// with what had come of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Incomplete {
    MaxOutputTokens,
    ContentFilter,
    Cancelled,
}

/// What a request used, in tokens, and what it cost, in billionths of a US dollar — the provider's figure where it
/// gives one, else the catalogue's prices'.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    /// Of `input_tokens`, those read from the prompt cache.
    pub cached_tokens: u64,
    /// Of `input_tokens`, those written into the prompt cache.
    #[serde(default)]
    pub cache_write_tokens: u64,
    pub output_tokens: u64,
    /// Of `output_tokens`, those spent reasoning.
    #[serde(default)]
    pub reasoning_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_nanodollars: Option<u64>,
}

/// What kind of failure ended a request, which decides whether trying again may help.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    ContextOverflow,
    PayloadTooLarge,
    RateLimit,
    Quota,
    Auth,
    Forbidden,
    ContentFilter,
    InvalidRequest,
    Server,
    Transport,
    InvalidResponse,
    Timeout,
}

/// Why a request failed: its kind; the provider's code and words; the HTTP status it answered with; how long to wait
/// before trying again, where it says; and, for a quota used up, when it is given again and over what window it is
/// counted, where it says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub kind: FailureKind,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_minutes: Option<u32>,
}

impl Failure {
    /// Whether asking again may come to more: a failure of the way there or of the provider's, or a limit that passes.
    #[must_use]
    pub fn retryable(&self) -> bool {
        matches!(
            self.kind,
            FailureKind::RateLimit | FailureKind::Server | FailureKind::Transport | FailureKind::Timeout | FailureKind::InvalidResponse
        )
    }
}

/// What a request came to: its provider's id of it, the model that answered by its reference, its status, what the
/// model wrote, what it used, why it failed or why it stopped short, and the tokens its context took.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub model: String,
    pub status: ResponseStatus,
    pub output: Vec<Item>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<Failure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incomplete: Option<Incomplete>,
    /// The input tokens of the response's last request alone, where it was continued across several.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_tokens: Option<u64>,
}
