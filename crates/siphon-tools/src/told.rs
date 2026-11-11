//! What a tool tells of a call: what the model reads, and — where the tool tells it — what the call did, typed by the
//! tool, as an MCP server's result gives its structured content beside the content its model reads. The model reads
//! the one, a client the other, so neither reads words written for the other.

use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What a tool told of a call: what the model reads, and what the call did, typed by the tool, where it tells it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Told {
    pub output: Vec<InputContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured: Option<serde_json::Value>,
}

impl Told {
    /// What a call did, the model reading `output`, a client `structured`.
    #[must_use]
    pub fn structured(output: Vec<InputContent>, structured: &Structured) -> Self {
        Self {
            output,
            structured: serde_json::to_value(structured).ok(),
        }
    }
}

impl From<Vec<InputContent>> for Told {
    fn from(output: Vec<InputContent>) -> Self {
        Self { output, structured: None }
    }
}

/// What a call of one of Siphon's own tools did, as a client reads it: a command that ran, and how it ended; a file a
/// write or an edit changed, whether it made it, and the lines added and taken out; the lines of a file a read holds,
/// and how many it has where the read reached its end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Structured {
    Ran {
        ended: CommandEnd,
    },
    Changed {
        path: String,
        created: bool,
        additions: usize,
        deletions: usize,
    },
    Read {
        path: String,
        from: usize,
        to: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        total: Option<usize>,
    },
}

/// How a command ended: it exited with a code, a signal ended it, its time ran out, it was stopped, or it wrote past
/// what a call takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandEnd {
    Exited { code: i32 },
    Signalled { signal: i32 },
    TimedOut { seconds: u64 },
    Stopped,
    TooLarge { bytes: usize },
}
