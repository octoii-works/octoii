//! A window of a thread's conversation: what its model is sent, from the last compaction on, as the log holds it — the
//! system entries the window begins with; the compaction's recap, which stands for every entry before those it keeps;
//! the input it keeps and the entries it keeps from its `from` on, as they stand before it; then every entry after the
//! window's beginning. The whole log where nothing was compacted. A window begins where the thread's directory says, by
//! its `siphon.window_from` annotation, and at the log's beginning where it says nothing; what a window begins with is
//! never changed within it, so a provider's cache of it holds.

use beak::Snapshot;
use octoii_ai::vocabulary::ReasoningEffort;

use crate::log::{self, Cursor, Entry, Error};
use crate::thread;

/// The annotation on a thread's directory that holds where its window begins in its log — the first entry its
/// compaction keeps — as a reader's cursor, as JSON; none for a window that begins with the log.
pub const FROM_ANNOTATION: &str = "siphon.window_from";

/// A window of a thread's conversation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Window {
    /// The system entries it begins with, before any other.
    pub told: Vec<Entry>,
    /// The recap its compaction's model wrote; none for the window the log begins.
    pub recap: Option<String>,
    /// How hard its model reasons as it begins, where its compaction says.
    pub effort: Option<ReasoningEffort>,
    /// How many entries its conversation begins with that its compaction carried from before it: each was sent in an
    /// earlier window, so what a reply among them says of its request is of that window.
    pub carried: usize,
    /// The rest, in the order the model is sent it.
    pub conversation: Vec<Entry>,
}

impl Window {
    /// The window the log of the thread `id` ends in, as `snapshot` left it.
    ///
    /// # Errors
    /// The volume's failure; a line that is no entry, or an annotation not as Siphon writes it.
    pub fn read(snapshot: &Snapshot, id: &str) -> Result<Self, Error> {
        let read = log::read_after(snapshot, id, from(snapshot, id)?.as_ref(), u64::MAX)?;
        let entries = read
            .lines
            .iter()
            .map(|line| log::entry_of(line))
            .collect::<serde_json::Result<Vec<_>>>()?;
        Ok(Self::of(entries))
    }

    /// The window of `entries`, the log from where the window begins: from its last compaction where it holds one —
    /// the input it keeps and the entries from its `from` on carried, but their system entries, which the window's own
    /// stand for — the system entries that begin it its own, among the changes of how the thread runs that came with
    /// them, which the conversation holds where they stand.
    #[must_use]
    pub fn of(mut entries: Vec<Entry>) -> Self {
        let mut window = Self::default();
        let compacted = entries.iter().rposition(|entry| matches!(entry, Entry::Compaction { .. }));
        let after = match compacted {
            Some(at) => {
                let after = entries.split_off(at + 1);
                if let Some(Entry::Compaction {
                    kept,
                    from,
                    effort,
                    response,
                    ..
                }) = entries.pop()
                {
                    let start = entries.iter().position(|entry| entry.id() == from).unwrap_or(entries.len());
                    window.conversation = entries
                        .into_iter()
                        .enumerate()
                        .filter(|(at, entry)| {
                            (*at >= start || Some(entry.id()) == kept.as_deref()) && !matches!(entry, Entry::System { .. })
                        })
                        .map(|(_, entry)| entry)
                        .collect();
                    window.carried = window.conversation.len();
                    window.recap = Some(response.words());
                    window.effort = effort;
                }
                after
            }
            None => entries,
        };
        let mut begun = false;
        for entry in after {
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

    /// Whether the window begins here: it is told nothing yet, so what it is told next — and the tools it declares —
    /// begins it.
    #[must_use]
    pub fn begins(&self) -> bool {
        self.told.is_empty()
            && !self.conversation[self.carried..]
                .iter()
                .any(|entry| matches!(entry, Entry::System { .. }))
    }
}

/// Where the window of the thread `id` begins in its log, as its directory says; none for its log's beginning.
///
/// # Errors
/// The volume's failure; an annotation not as Siphon writes it.
pub fn from(snapshot: &Snapshot, id: &str) -> Result<Option<Cursor>, Error> {
    let directory = snapshot.stat(&thread::path(id))?.and_then(|entry| entry.directory);
    Ok(directory
        .and_then(|directory| directory.annotation(FROM_ANNOTATION.as_bytes()).cloned())
        .map(|value| serde_json::from_slice(&value))
        .transpose()?)
}
