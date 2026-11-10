//! A window of a thread's conversation: what its model is sent, as the log holds it from where the window begins — the
//! system entries it begins with, what the model is told before anything else of it, then every entry after them in
//! the order the log holds them. A window begins where the thread's directory says, by its `siphon.window_from`
//! annotation, and at the log's beginning where it says nothing; what a window begins with is never changed within it,
//! so a provider's cache of it holds.

use beak::Snapshot;

use crate::log::{self, Cursor, Entry, Error};
use crate::thread;

/// The annotation on a thread's directory that holds where its window begins in its log, as a reader's cursor, as
/// JSON; none for a window that begins with the log.
pub const FROM_ANNOTATION: &str = "siphon.window_from";

/// A window of a thread's conversation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Window {
    /// The system entries it begins with, before any other.
    pub told: Vec<Entry>,
    /// The rest, in the order the log holds them.
    pub conversation: Vec<Entry>,
}

impl Window {
    /// The window the log of the thread `id` ends in, as `snapshot` left it.
    ///
    /// # Errors
    /// The volume's failure; a line that is no entry, or an annotation not as Siphon writes it.
    pub fn read(snapshot: &Snapshot, id: &str) -> Result<Self, Error> {
        let directory = snapshot.stat(&thread::path(id))?.and_then(|entry| entry.directory);
        let from: Option<Cursor> = directory
            .and_then(|directory| directory.annotation(FROM_ANNOTATION.as_bytes()).cloned())
            .map(|value| serde_json::from_slice(&value))
            .transpose()?;
        let read = log::read_after(snapshot, id, from.as_ref(), u64::MAX)?;
        let entries = read
            .lines
            .iter()
            .map(|line| log::entry_of(line))
            .collect::<serde_json::Result<Vec<_>>>()?;
        Ok(Self::of(entries))
    }

    /// The window of `entries`, the log from where it begins: the system entries before its first input its own,
    /// among the changes of how the thread runs that came with them, which the conversation holds where they stand.
    #[must_use]
    pub fn of(entries: Vec<Entry>) -> Self {
        let mut window = Self::default();
        let mut begun = false;
        for entry in entries {
            begun |= !matches!(entry, Entry::System { .. } | Entry::Config { .. });
            match entry {
                Entry::System { .. } if !begun => window.told.push(entry),
                _ => window.conversation.push(entry),
            }
        }
        window
    }

    /// The window's last entry, by its id; none for a window with none.
    #[must_use]
    pub fn last(&self) -> Option<&str> {
        self.conversation.last().or(self.told.last()).map(Entry::id)
    }
}
