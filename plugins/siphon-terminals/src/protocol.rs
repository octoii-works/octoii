//! The terminals' methods, events and topics, as their plugin adds them to the protocol: a terminal of a thread's
//! opened, typed to, sized and closed; the terminals of a thread as they change, and a terminal as it writes — its
//! screen drawn first, then each piece it writes, from where the client stood.

use serde::{Deserialize, Serialize};
use siphon_protocol::{Nothing, Scope};
use ts_rs::TS;

siphon_protocol::methods! {
    events: TerminalsEvent, topics: TerminalsTopic;
    /// A terminal opened for a thread, the person's login shell in its workspace, sized as the person's view of it is:
    /// its id.
    "terminals.open" Open(Open) -> Opened, Some(Scope::TerminalsOperate);
    /// Keys the person typed, and what they pasted, to a terminal's program.
    "terminals.write" Write(Write) -> Nothing, Some(Scope::TerminalsOperate);
    /// A terminal sized anew, its program told.
    "terminals.resize" Resize(Resize) -> Nothing, Some(Scope::TerminalsOperate);
    /// A terminal closed, its program hung up on.
    "terminals.close" Close(OfTerminal) -> Nothing, Some(Scope::TerminalsOperate);
}

/// A terminal's size, in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Cells {
    pub rows: u16,
    pub cols: u16,
}

/// A terminal, by its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OfTerminal {
    pub terminal: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Open {
    pub thread: String,
    pub size: Cells,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Opened {
    pub terminal: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Write {
    pub terminal: String,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Resize {
    pub terminal: String,
    pub size: Cells,
}

/// A terminal of a thread's: its id, its number in the thread, its size, and the title its program set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TerminalListed {
    pub id: String,
    pub number: u32,
    pub size: Cells,
    pub title: String,
}

/// What a client subscribes to of the terminals': a thread's terminals, or a terminal as it writes — from the offset
/// in its output the client stood at, or its screen drawn first, where none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalsTopic {
    Terminals {
        thread: String,
    },
    Terminal {
        terminal: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        from: Option<u64>,
    },
}

/// An event of a subscription to the terminals': a thread's terminals as they changed; a terminal's screen drawn again
/// whole, through the offset in its output it shows; or a piece of what it wrote, from an offset. A screen and a piece
/// are binary frames, whose bytes after this event are the screen's sequences, or the piece.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum TerminalsEvent {
    Terminals {
        subscription: String,
        terminals: Vec<TerminalListed>,
    },
    Screen {
        subscription: String,
        through: u64,
    },
    Output {
        subscription: String,
        offset: u64,
    },
}
