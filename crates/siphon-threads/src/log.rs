//! A thread's log: JSON Lines in segments under its `log/`, each named by the id of the entry that begins it, as a
//! log's segment is named by its first offset, so the names sort in the order the segments began. Only the last is
//! appended to; it is sealed as the next begins, past `SEGMENT_BYTES_MAX`. An entry is one line: its `id`, made as it
//! is appended, its `type`, and its fields. The log has one writer at a time — the commit that makes the thread, then
//! its main process, or the turn it runs while it runs — so its ids rise along it and each entry's place is where its
//! writer put it.

use beak::{Range, Snapshot};
use bytes::Bytes;
use octoii_ai::vocabulary::{FunctionCallOutput, InputContent, ReasoningEffort, Response, Tool};
use serde::{Deserialize, Serialize};
use siphon_runtime::Changes;

use crate::config::ConfigChanged;
use crate::inbox::ReplyTo;
use crate::thread;

/// Past it, the next entry begins a segment of its own.
pub const SEGMENT_BYTES_MAX: u64 = 32 << 20;

/// What a segment's name ends in, after the id of the entry that begins it.
const SEGMENT_EXTENSION: &str = ".jsonl";

/// An entry of a thread's log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Entry {
    /// How the thread runs changed, from here on: by whom, and each field from what it was to what it is.
    Config {
        id: String,
        actor: String,
        #[serde(flatten)]
        changed: ConfigChanged,
    },
    /// An input taken from the inbox: whose it is, the inbox's id of it, what it says, and who is told how the turn that
    /// took it ends.
    Input {
        id: String,
        actor: String,
        delivery: String,
        content: Vec<InputContent>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_to: Option<ReplyTo>,
    },
    /// What the model is told of itself and of where it works, where it changed before a turn: each section new or
    /// changed, whole, and each no longer told; and, in the entry that begins a window, the tools the window declares.
    System {
        id: String,
        sections: Vec<Section>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tools: Vec<Tool>,
    },
    /// The model's reply, as it came, to the model request of `request`, by its key; `cancelled` where a stop cut it.
    Reply {
        id: String,
        request: String,
        response: Response,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        cancelled: bool,
    },
    /// A call's result, as its turn appends it once the call ended: Siphon's key of the call, how it ended, and the
    /// output the model is sent.
    Result {
        id: String,
        call: String,
        status: CallStatus,
        output: FunctionCallOutput,
    },
    /// A turn's end: the turn, by its id — none for inputs that failed before a turn ran — how it ended, and why,
    /// unless it replied.
    TurnEnd {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn: Option<String>,
        outcome: Outcome,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Where the conversation grew past what its model takes: the recap the model wrote, by the request of `request`,
    /// its response as it came, of everything before the entry `from` but the input `kept` — the latest the person gave
    /// before it, none where the cut falls at an input — and how hard the model reasons as the new window begins. From
    /// here on a window of its own begins with what the model is told, gathered anew, then the recap, then the input
    /// as the person gave it and the entries from `from` on as they stand, nothing copied.
    Compaction {
        id: String,
        request: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kept: Option<String>,
        from: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        effort: Option<ReasoningEffort>,
        response: Response,
    },
    /// A model request's answer that came to nothing — one that failed, or one a stop came before it was used — its
    /// response as it came, kept for the record and never sent to the model.
    Discarded { id: String, request: String, response: Response },
}

impl Entry {
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Config { id, .. }
            | Self::Input { id, .. }
            | Self::System { id, .. }
            | Self::Reply { id, .. }
            | Self::Result { id, .. }
            | Self::TurnEnd { id, .. }
            | Self::Compaction { id, .. }
            | Self::Discarded { id, .. } => id,
        }
    }
}

/// A section of what the model is told, by its name, with its text as it is sent; none for a section no longer told.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub name: String,
    pub text: Option<String>,
}

/// How a call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CallStatus {
    Ok,
    /// Its tool is none of those the model may call.
    Unavailable,
    InvalidArguments,
    /// Its tool refused, or failed.
    ToolError,
    /// The person, or a second model, did not let it go ahead.
    Rejected,
    /// It was cut off with nothing to give: by a stop, or by the host's end.
    Interrupted,
}

/// How a turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Replied,
    Failed,
    Stopped,
}

/// Where an entry is: its segment, and its line's offset and length there, newline and all, so it is read again as
/// `pread` reads, in place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    pub segment: String,
    pub offset: u64,
    pub length: u64,
}

/// Where a reader of a log stands: the segment it read, and how far.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    pub segment: String,
    pub offset: u64,
}

/// What a reader found: the lines after where it stood, each without its newline; where it stands after them, none
/// for a log not begun; and whether lines are left past them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    pub lines: Vec<Bytes>,
    pub cursor: Option<Cursor>,
    pub more: bool,
}

/// Why a log could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Beak(#[from] beak::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// The log's directory in the thread `id`.
#[must_use]
pub fn path(id: &str) -> Bytes {
    thread::within(id, "log")
}

/// The segment `segment`, by its name, of the log of the thread `id`.
#[must_use]
pub fn segment(id: &str, segment: &str) -> Bytes {
    [&path(id)[..], b"/", segment.as_bytes()].concat().into()
}

/// The segments of the log of the thread `id`, in the order they began, each with its length.
///
/// # Errors
/// The volume's failure.
pub fn segments(snapshot: &Snapshot, id: &str) -> beak::Result<Vec<(String, u64)>> {
    let range = Range {
        limit: usize::MAX,
        ..Range::default()
    };
    Ok(snapshot
        .list(&path(id), &range)?
        .unwrap_or_default()
        .into_iter()
        .map(|entry| (String::from_utf8_lossy(&entry.name).into_owned(), entry.length))
        .collect())
}

/// What appends to the log of a thread with a commit: the segment its entries go to, as the snapshot it was opened on
/// left it and with what was appended since.
#[derive(Debug)]
pub struct Writer {
    thread: String,
    last: Option<(String, u64)>,
}

impl Writer {
    /// A writer of the log of the thread `id`, as `snapshot` left it.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn open(snapshot: &Snapshot, id: &str) -> beak::Result<Self> {
        Ok(Self {
            thread: id.to_owned(),
            last: segments(snapshot, id)?.pop(),
        })
    }

    /// A writer of the log of a thread made in the same commit, which has none yet.
    #[must_use]
    pub fn new_log(id: &str) -> Self {
        Self {
            thread: id.to_owned(),
            last: None,
        }
    }

    /// Appends `entry` as one line with `changes`: to the last segment, or to one it begins where there is none or the
    /// last would pass its bound, the last sealed. Where the entry is.
    ///
    /// # Errors
    /// An entry that cannot be written as JSON.
    pub fn append(&mut self, changes: &mut Changes, entry: &Entry) -> serde_json::Result<Address> {
        let mut line = serde_json::to_vec(entry)?;
        line.push(b'\n');
        let length = line.len() as u64;
        let fits = self.last.as_ref().is_some_and(|(_, used)| used + length <= SEGMENT_BYTES_MAX);
        if !fits && let Some((full, _)) = self.last.take() {
            changes.seal(segment(&self.thread, &full));
        }
        let (name, used) = self.last.get_or_insert_with(|| (format!("{}{SEGMENT_EXTENSION}", entry.id()), 0));
        changes.append(segment(&self.thread, name), line);
        let address = Address {
            segment: name.clone(),
            offset: *used,
            length,
        };
        *used += length;
        Ok(address)
    }
}

/// The lines of the log of the thread `id` past `from` — from its first, where none — as `snapshot` left it: about
/// `bytes_max` of them, though a whole line at least where one is left. A reader that goes on from where it stands
/// reads each line once.
///
/// # Errors
/// The volume's failure.
pub fn read_after(snapshot: &Snapshot, id: &str, from: Option<&Cursor>, bytes_max: u64) -> beak::Result<Read> {
    let all = segments(snapshot, id)?;
    let start = from.map_or(0, |cursor| {
        all.iter().position(|(name, _)| *name == cursor.segment).unwrap_or(all.len())
    });
    let mut lines = Vec::new();
    let mut cursor = from.cloned();
    let mut read = 0;
    for (index, (name, length)) in all.iter().enumerate().skip(start) {
        let offset = cursor.as_ref().filter(|at| at.segment == *name).map_or(0, |at| at.offset);
        let whole = if offset < *length {
            whole_lines(snapshot, &segment(id, name), offset, *length, bytes_max.saturating_sub(read))?
        } else {
            Bytes::new()
        };
        read += whole.len() as u64;
        lines.extend(
            whole
                .split(|&byte| byte == b'\n')
                .filter(|line| !line.is_empty())
                .map(|line| whole.slice_ref(line)),
        );
        let at = offset + whole.len() as u64;
        cursor = Some(Cursor {
            segment: name.clone(),
            offset: at,
        });
        if at < *length || (read >= bytes_max && index + 1 < all.len()) {
            return Ok(Read { lines, cursor, more: true });
        }
    }
    Ok(Read {
        lines,
        cursor,
        more: false,
    })
}

/// The whole lines of the segment at `path`, `length` long, from `offset`: about `budget` bytes of them, up to the
/// last newline within, or the whole line it begins where none ends within.
fn whole_lines(snapshot: &Snapshot, path: &[u8], offset: u64, length: u64, budget: u64) -> beak::Result<Bytes> {
    let bytes = snapshot.read(path, offset, budget.max(1).min(length - offset))?.unwrap_or_default();
    if let Some(end) = bytes.iter().rposition(|&byte| byte == b'\n') {
        return Ok(bytes.slice(..=end));
    }
    let rest = snapshot.read(path, offset, length - offset)?.unwrap_or_default();
    let end = rest.iter().position(|&byte| byte == b'\n').map_or(rest.len(), |end| end + 1);
    Ok(rest.slice(..end))
}

/// Where the entry `entry`, by its id, begins in the log of the thread `id`, looked for from `from` on — from the log's
/// first, where none; none where it is not there.
///
/// # Errors
/// The volume's failure; a line that is no entry.
pub fn cursor_of(snapshot: &Snapshot, id: &str, from: Option<&Cursor>, entry: &str) -> Result<Option<Cursor>, Error> {
    #[derive(Deserialize)]
    struct Head {
        id: String,
    }
    let all = segments(snapshot, id)?;
    let start = from.map_or(0, |cursor| all.iter().position(|(name, _)| *name == cursor.segment).unwrap_or(0));
    for (name, _) in all.iter().skip(start) {
        let bytes = snapshot.read(&segment(id, name), 0, u64::MAX)?.unwrap_or_default();
        let mut offset = 0;
        for line in bytes.split(|&byte| byte == b'\n') {
            if !line.is_empty() && serde_json::from_slice::<Head>(line)?.id == entry {
                return Ok(Some(Cursor {
                    segment: name.clone(),
                    offset,
                }));
            }
            offset += line.len() as u64 + 1;
        }
    }
    Ok(None)
}

/// The time, in milliseconds since the epoch, an id made by the volume — a `UUIDv7` — was made at; none for what is no
/// such id. A segment's name begins with one.
#[must_use]
pub fn time_of(id: &str) -> Option<i64> {
    let hex: String = id.chars().filter(char::is_ascii_hexdigit).take(12).collect();
    (hex.len() == 12).then(|| i64::from_str_radix(&hex, 16).ok()).flatten()
}

/// The entry of `line`.
///
/// # Errors
/// A line that is no entry.
pub fn entry_of(line: &[u8]) -> serde_json::Result<Entry> {
    serde_json::from_slice(line)
}
