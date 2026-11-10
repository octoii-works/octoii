//! The vocabulary of a conversation, the one shape every request goes to a model in and every answer comes back in,
//! whichever provider serves it — the Responses API's, kept to what Octoii says — and the shape a thread's log keeps
//! it in. A driver renders it for its provider and reads its provider's answer back into it.

mod content;
mod items;
mod request;
mod response;

pub use content::{FileRef, ImageDetail, InputContent, OutputContent, UrlCitation};
pub use items::{
    ConfigurationUpdate, FunctionCall, FunctionCallOutput, Item, ItemStatus, Message, Origin, Reasoning, ReasoningEffort, WebSearchAction,
    WebSearchCall, WebSearchStatus,
};
pub use request::{Format, ReasoningConfig, Request, Tool, ToolChoice};
pub use response::{Failure, FailureKind, Incomplete, Response, ResponseStatus, Usage};
