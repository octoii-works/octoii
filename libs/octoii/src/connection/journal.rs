//! Where a subscription stands in the volume's journal: the commits it has read past, so that after each commit it
//! reads what changed since — every path a commit wrote or deleted — and tells anew only what those paths touch, as
//! `inotify` wakes a reader for what it watches alone. Where the journal no longer holds every commit since, its
//! reader is told so, and reads its topic whole again.

use beak::Snapshot;
use bytes::Bytes;

/// How many change sets a reader takes from the journal at a time.
const SETS_AT_ONCE: usize = 256;

/// A reader of the journal, standing after the commit `seen`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Followed {
    seen: u64,
}

/// What the commits since a reader last read changed: the paths, or that the journal no longer holds them all.
pub(crate) enum Since {
    Paths(Vec<Bytes>),
    Missed,
}

impl Followed {
    /// A reader standing where `snapshot` stands.
    pub(crate) fn at(snapshot: &Snapshot) -> Self {
        Self { seen: snapshot.seq() }
    }

    /// What the commits after where it stood, through `snapshot`'s, changed — it standing after them.
    ///
    /// # Errors
    /// The volume's failure.
    pub(crate) fn since(&mut self, snapshot: &Snapshot) -> beak::Result<Since> {
        let mut paths = Vec::new();
        while self.seen < snapshot.seq() {
            let journal = snapshot.journal(self.seen, SETS_AT_ONCE, &[])?;
            if journal.missed {
                self.seen = snapshot.seq();
                return Ok(Since::Missed);
            }
            for set in journal.change_sets {
                paths.extend(set.changes.into_iter().map(|changed| match changed {
                    beak::Changed::Deleted { path } | beak::Changed::Written { path, .. } => path,
                }));
            }
            if journal.through <= self.seen {
                break;
            }
            self.seen = journal.through;
        }
        self.seen = self.seen.max(snapshot.seq());
        Ok(Since::Paths(paths))
    }
}

/// Whether `path` is `at`, or lies under it.
pub(crate) fn is_at_or_under(path: &[u8], at: &[u8]) -> bool {
    path.strip_prefix(at).is_some_and(|rest| rest.is_empty() || rest.starts_with(b"/"))
}
