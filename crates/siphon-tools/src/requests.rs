//! The requests a call makes: its tool exec'd, the person asked whether it goes ahead, a second model asked to judge
//! it. Each is durable — made with its caller's commit, answered once — so a call that waits on a person waits through
//! the host's restarts.

use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use siphon_runtime::{AskedOfPerson, Request};

use crate::permissions::{Grants, Writes};

/// A call's tool exec'd for the thread `thread`: the tool by its name, the call's arguments, and where it may write as
/// its weighing said; the call's own key names its outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exec {
    pub thread: String,
    pub call: String,
    pub tool: String,
    pub arguments: serde_json::Value,
    pub writes: Writes,
    pub workspace: String,
}

impl Request for Exec {
    const NAME: &'static str = "siphon.exec";
    type Answer = Vec<InputContent>;
}

/// What a call would do, as the person and a second model are shown it: the tool, the call's arguments, each thing it
/// would do — a file it edits, a command it runs, the tool that may do anything — and why it must, where it said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Shown {
    pub tool: String,
    pub arguments: String,
    pub does: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub justification: Option<String>,
}

/// The person asked whether a call of the thread `thread` goes ahead, and offered what an "always" grants its project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permission {
    pub thread: String,
    pub call: Shown,
    pub always: Grants,
}

/// The person's answer: let it go ahead, for good where `always`; or not, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Decision {
    Allow {
        #[serde(default)]
        always: bool,
    },
    Deny {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

impl Request for Permission {
    const NAME: &'static str = "siphon.permission";
    type Answer = Decision;
}

impl AskedOfPerson for Permission {}

/// A second model asked to judge a call of the thread `thread`, as the person named it to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    pub thread: String,
    pub call: Shown,
}

/// The model's judgment: let it go ahead, or not; and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Judgment {
    pub allow: bool,
    pub reason: String,
}

impl Request for Review {
    const NAME: &'static str = "siphon.review";
    type Answer = Judgment;
}
