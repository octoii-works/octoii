//! How the person keeps a thread, as a mail client marks a message — pinned, archived, snoozed, seen, settled — on its
//! directory alone, never in its log: none of it changes the conversation. Pinned and archived are labels, so the index
//! finds the threads kept either way; the times are annotations; a snooze is a name in the directory that goes at its
//! time, so the journal tells of its end as it ends.

use beak::{Directory, Entry};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use siphon_runtime::{Changes, Named};

use crate::thread::{self, ARCHIVED_LABEL, PINNED_LABEL};

/// The name in a thread's directory that is there while it is snoozed, and goes when the snooze ends.
pub const SNOOZE_NAME: &str = "siphon.snooze";

const SEEN_UNTIL: &str = "siphon.seen_until";
const SETTLED_AT: &str = "siphon.settled_at";
const UNSETTLED_AT: &str = "siphon.unsettled_at";

/// How the person keeps a thread; times in milliseconds since the epoch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marks {
    pub pinned: bool,
    pub archived: bool,
    /// Till when it is snoozed, kept out of their threads till then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snoozed_until: Option<i64>,
    /// Till when, by the thread's own times, the person has seen what it brought them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen_until: Option<i64>,
    /// When they put it with what is done: settled till something comes of it after.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settled_at: Option<i64>,
    /// When they brought it back from what is done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsettled_at: Option<i64>,
}

/// A change of how the person keeps a thread: each mark given in place of the one it had, each left out kept; a snooze
/// till a time past ends it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarksChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snoozed_until: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen_until: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settled_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsettled_at: Option<i64>,
}

fn time(directory: &Directory, name: &str) -> Option<i64> {
    directory
        .annotation(name.as_bytes())
        .and_then(|value| std::str::from_utf8(value).ok()?.parse().ok())
}

impl Marks {
    /// The marks `directory` holds, and the snooze `snooze` shows where the thread is snoozed.
    #[must_use]
    pub fn of(directory: &Directory, snooze: Option<&Entry>) -> Self {
        let flagged = |name: &str| directory.label(name.as_bytes()).is_some_and(|value| value == "true");
        Self {
            pinned: flagged(PINNED_LABEL),
            archived: flagged(ARCHIVED_LABEL),
            snoozed_until: snooze.and_then(|entry| entry.expires_at),
            seen_until: time(directory, SEEN_UNTIL),
            settled_at: time(directory, SETTLED_AT),
            unsettled_at: time(directory, UNSETTLED_AT),
        }
    }
}

impl MarksChange {
    /// Adds what makes this change of the thread `id`, snoozed now as `snoozed`, to `changes`, at `now`.
    pub(crate) fn add(&self, changes: &mut Changes, id: &str, snoozed: bool, now: i64) {
        let flag = |name: &'static str, on: bool| (Bytes::from_static(name.as_bytes()), Some(thread::flag(on)));
        let labels: Vec<Named> = [
            self.pinned.map(|on| flag(PINNED_LABEL, on)),
            self.archived.map(|on| flag(ARCHIVED_LABEL, on)),
        ]
        .into_iter()
        .flatten()
        .collect();
        let time = |name: &'static str, at: i64| (Bytes::from_static(name.as_bytes()), Some(Bytes::from(at.to_string())));
        let annotations: Vec<Named> = [
            self.seen_until.map(|at| time(SEEN_UNTIL, at)),
            self.settled_at.map(|at| time(SETTLED_AT, at)),
            self.unsettled_at.map(|at| time(UNSETTLED_AT, at)),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !labels.is_empty() || !annotations.is_empty() {
            changes.meta(thread::path(id), labels, annotations);
        }
        if let Some(until) = self.snoozed_until {
            let snooze = thread::within(id, SNOOZE_NAME);
            if until > now {
                changes.write(snooze.clone(), Bytes::new()).expire(snooze, Some(until));
            } else if snoozed {
                changes.delete(snooze);
            }
        }
    }
}
