//! What fills a thread's window, as the person is shown it: the tokens its next request takes, as near as its log
//! tells, shared among what holds them — what Siphon tells the model of itself, the machine and the thread; the
//! instructions; the skills; the MCP servers' catalog; the tools' declarations; the recap of what came before; the
//! person's messages; the model's replies — its words, its thoughts and its calls; and what the calls gave. Each is
//! estimated as the compaction estimates entries, a token for every four bytes, and the estimates scaled to the
//! window's tokens: near, not exact, as no tokenizer is run.

use serde::{Deserialize, Serialize};
use siphon_threads::{Entry, Window};
use ts_rs::TS;

use crate::compaction;

/// What holds a share of a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ContextPart {
    System,
    Instructions,
    Skills,
    Catalog,
    Tools,
    Recap,
    Messages,
    Replies,
    Results,
}

/// The parts, in the order they are shown.
const PARTS: [ContextPart; 9] = [
    ContextPart::System,
    ContextPart::Instructions,
    ContextPart::Skills,
    ContextPart::Catalog,
    ContextPart::Tools,
    ContextPart::Recap,
    ContextPart::Messages,
    ContextPart::Replies,
    ContextPart::Results,
];

/// A part's share of a window, in tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Share {
    pub part: ContextPart,
    pub tokens: u64,
}

/// What fills a window: its tokens, and each part's share of them — those that hold none left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Composition {
    pub tokens: u64,
    pub parts: Vec<Share>,
}

/// The tokens `bytes` take, as near as their length tells: one for every four.
fn estimated(bytes: usize) -> u64 {
    bytes as u64 / 4
}

/// What fills `window`.
#[must_use]
pub fn composition(window: &Window) -> Composition {
    let mut shares = [0_u64; PARTS.len()];
    let mut add = |part: ContextPart, tokens: u64| {
        if let Some(at) = PARTS.iter().position(|each| *each == part) {
            shares[at] += tokens;
        }
    };
    for entry in window.told.iter().chain(&window.conversation) {
        match entry {
            Entry::System { sections, tools, .. } => {
                for section in sections {
                    add(part_of(&section.name), estimated(section.text.as_deref().map_or(0, str::len)));
                }
                for tool in tools {
                    add(
                        ContextPart::Tools,
                        estimated(serde_json::to_vec(tool).map_or(0, |bytes| bytes.len())),
                    );
                }
            }
            Entry::Input { .. } => add(ContextPart::Messages, of_entry(entry)),
            Entry::Reply { .. } => add(ContextPart::Replies, of_entry(entry)),
            Entry::Result { .. } => add(ContextPart::Results, of_entry(entry)),
            _ => {}
        }
    }
    add(ContextPart::Recap, estimated(window.recap.as_deref().map_or(0, str::len)));
    let tokens = compaction::tokens(window);
    let whole: u64 = shares.iter().sum();
    let parts = PARTS
        .iter()
        .zip(shares)
        .filter(|(_, each)| *each > 0)
        .map(|(part, each)| Share {
            part: *part,
            tokens: each * tokens / whole,
        })
        .collect();
    Composition { tokens, parts }
}

/// The tokens `entry` takes, as its JSON's length tells.
fn of_entry(entry: &Entry) -> u64 {
    estimated(serde_json::to_vec(entry).map_or(0, |bytes| bytes.len()))
}

/// The part a system section of `name` holds a share of: what Siphon tells of itself, the machine and the thread, but
/// the instructions, the skills and the catalog.
fn part_of(name: &str) -> ContextPart {
    match name {
        "instructions" => ContextPart::Instructions,
        "skills" => ContextPart::Skills,
        "catalog" => ContextPart::Catalog,
        _ => ContextPart::System,
    }
}
