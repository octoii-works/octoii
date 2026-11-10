//! A snapshot of the kernel's volume, read from any thread at once, seeing no later commit: what each read gives copied
//! out as owned values, Beak's let go of as it is read.

use beak_sys as sys;
use bytes::Bytes;

use crate::Result;
use crate::ffi::{copied, copied_maybe, lent, maybe, object, of_i64, of_u64};
use crate::kernel::{Keep, buffer};

/// The snapshot as Beak holds it.
struct Raw(*mut sys::beak_snapshot);

// SAFETY: a snapshot is read from any thread at once, as its objects never change.
unsafe impl Send for Raw {}
// SAFETY: as above.
unsafe impl Sync for Raw {}

impl Drop for Raw {
    fn drop(&mut self) {
        // SAFETY: the snapshot Beak gave, let go of once.
        unsafe { sys::beak_snapshot_free(self.0) };
    }
}

/// The volume as one commit left it.
pub struct Snapshot(Raw);

impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Snapshot").field(&self.seq()).finish()
    }
}

/// What an entry names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Directory,
    File,
}

/// A directory's own: its labels, its annotations, the commit that made it and the last that changed it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Directory {
    pub labels: Vec<(Bytes, Bytes)>,
    pub annotations: Vec<(Bytes, Bytes)>,
    pub created_seq: u64,
    pub changed_seq: u64,
}

impl Directory {
    /// The value of the label `name`.
    #[must_use]
    pub fn label(&self, name: &[u8]) -> Option<&Bytes> {
        self.labels.iter().find(|(each, _)| each == name).map(|(_, value)| value)
    }

    /// The value of the annotation `name`.
    #[must_use]
    pub fn annotation(&self, name: &[u8]) -> Option<&Bytes> {
        self.annotations.iter().find(|(each, _)| each == name).map(|(_, value)| value)
    }
}

/// An entry: its name, what it names, a file's length and mode, when it goes, what it holds, and a directory's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: Bytes,
    pub kind: EntryKind,
    pub length: u64,
    pub executable: bool,
    pub append_only: bool,
    pub sealed: bool,
    pub expires_at: Option<i64>,
    pub size_bytes: u64,
    pub size_objects: u64,
    pub directory: Option<Directory>,
}

/// # Safety
/// `ptr` holds `len` items Beak gave, alive for the call.
unsafe fn items<'a, T>(ptr: *const T, len: usize) -> &'a [T] {
    // SAFETY: as the caller promises.
    if len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(ptr, len) }
    }
}

fn names(names: &sys::beak_names) -> Vec<(Bytes, Bytes)> {
    // SAFETY: Beak gave the names.
    unsafe { items(names.items, names.len) }
        .iter()
        .map(|named| (copied(&named.name), copied(&named.value)))
        .collect()
}

fn kind(kind: sys::beak_kind) -> EntryKind {
    match kind {
        sys::beak_kind::BEAK_KIND_DIRECTORY => EntryKind::Directory,
        sys::beak_kind::BEAK_KIND_FILE => EntryKind::File,
    }
}

impl Entry {
    fn of(entry: &sys::beak_entry) -> Self {
        // SAFETY: an entry's directory is null, or one Beak gave with it.
        let directory = unsafe { entry.directory.as_ref() }.map(|directory| Directory {
            labels: names(&directory.labels),
            annotations: names(&directory.annotations),
            created_seq: directory.created_seq,
            changed_seq: directory.changed_seq,
        });
        Self {
            name: copied(&entry.name),
            kind: kind(entry.kind),
            length: entry.length,
            executable: entry.executable,
            append_only: entry.append_only,
            sealed: entry.sealed,
            expires_at: of_i64(entry.expires_at),
            size_bytes: entry.size_bytes,
            size_objects: entry.size_objects,
            directory,
        }
    }
}

/// Which entries of a directory a page reads.
#[derive(Debug, Clone, Default)]
pub struct Range<'a> {
    pub prefix: &'a [u8],
    pub after: Option<&'a [u8]>,
    pub reverse: bool,
    pub limit: usize,
}

impl Range<'_> {
    pub(crate) fn lower(&self) -> sys::beak_range {
        sys::beak_range {
            prefix: lent(self.prefix),
            after: maybe(self.after),
            reverse: self.reverse,
            limit: self.limit,
        }
    }
}

/// What the label index is asked.
#[derive(Debug, Clone, Default)]
pub struct Query<'a> {
    pub kind: &'a [u8],
    pub name: &'a [u8],
    pub value: Option<&'a [u8]>,
    pub under: &'a [u8],
    pub after: Option<&'a [u8]>,
}

/// A directory the label index found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: Bytes,
    pub value: Bytes,
    pub entry: Entry,
}

/// Where a process is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Runnable,
    Running,
    Sleeping,
    Exiting,
    Zombie,
}

/// Why a process that would run is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    RestartLimit,
    MissingProgram,
    UnknownVersion,
}

/// How a process ended, and what with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Completed(Option<Bytes>),
    Failed { error: Option<Bytes>, reason: Option<Bytes> },
    Stopped { result: Option<Bytes>, reason: Option<Bytes> },
}

/// A process's record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub kind: Bytes,
    pub version: u32,
    pub created_seq: u64,
    pub state: ProcessState,
    pub frozen: bool,
    pub priority: u8,
    pub checkpoint: Option<Bytes>,
    pub waits_until: Option<i64>,
    pub outcome: Option<Outcome>,
    /// The stop asked of it: its reason and when.
    pub term: Option<(Option<Bytes>, i64)>,
    pub held: Option<Held>,
    pub restarts: u32,
    pub attempt: u32,
    pub namespace: Option<Bytes>,
    pub usage: Vec<(Bytes, u64)>,
    pub started_at: Option<i64>,
    pub run_since: Option<i64>,
    pub ended_at: Option<i64>,
}

impl Record {
    fn of(record: &sys::beak_record) -> Self {
        use sys::beak_outcome_status as Status;
        let (result, error, reason) = (
            copied_maybe(&record.result),
            copied_maybe(&record.error),
            copied_maybe(&record.reason),
        );
        let outcome = match record.outcome {
            Status::BEAK_OUTCOME_STATUS_NONE => None,
            Status::BEAK_OUTCOME_STATUS_COMPLETED => Some(Outcome::Completed(result)),
            Status::BEAK_OUTCOME_STATUS_FAILED => Some(Outcome::Failed { error, reason }),
            Status::BEAK_OUTCOME_STATUS_STOPPED => Some(Outcome::Stopped { result, reason }),
        };
        let state = match record.state {
            sys::beak_process_state::BEAK_PROCESS_STATE_RUNNABLE => ProcessState::Runnable,
            sys::beak_process_state::BEAK_PROCESS_STATE_RUNNING => ProcessState::Running,
            sys::beak_process_state::BEAK_PROCESS_STATE_SLEEPING => ProcessState::Sleeping,
            sys::beak_process_state::BEAK_PROCESS_STATE_EXITING => ProcessState::Exiting,
            sys::beak_process_state::BEAK_PROCESS_STATE_ZOMBIE => ProcessState::Zombie,
        };
        let held = match record.held {
            sys::beak_held::BEAK_HELD_NO => None,
            sys::beak_held::BEAK_HELD_RESTART_LIMIT => Some(Held::RestartLimit),
            sys::beak_held::BEAK_HELD_MISSING_PROGRAM => Some(Held::MissingProgram),
            sys::beak_held::BEAK_HELD_UNKNOWN_VERSION => Some(Held::UnknownVersion),
        };
        // SAFETY: Beak gave the usage with the record.
        let usage = unsafe { items(record.usage.items, record.usage.len) }
            .iter()
            .map(|used| (copied(&used.name), used.value))
            .collect();
        Self {
            kind: copied(&record.kind),
            version: record.version,
            created_seq: record.created_seq,
            state,
            frozen: record.frozen,
            priority: record.priority,
            checkpoint: copied_maybe(&record.checkpoint),
            waits_until: of_i64(record.waits_until),
            outcome,
            term: record.terminating.then(|| (copied_maybe(&record.term_reason), record.term_at)),
            held,
            restarts: record.restarts,
            attempt: record.attempt,
            namespace: copied_maybe(&record.namespace_),
            usage,
            started_at: of_i64(record.started_at),
            run_since: of_i64(record.run_since),
            ended_at: of_i64(record.ended_at),
        }
    }
}

/// Where a selector looks: directly in a directory, keyed from a key on where it has one; or anywhere under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    In { directory: Bytes, from: Option<Bytes> },
    Under(Bytes),
}

/// The directories a selector picks: of its kinds, carrying its labels, where it looks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub kinds: Vec<Bytes>,
    pub labels: Vec<(Bytes, Bytes)>,
    pub scope: Scope,
}

impl Selector {
    pub(crate) fn lower(&self, keep: &mut Keep) -> sys::beak_selector {
        let kinds: Vec<sys::beak_bytes> = self.kinds.iter().map(|kind| lent(kind)).collect();
        let labels: Vec<sys::beak_pair> = self
            .labels
            .iter()
            .map(|(name, value)| sys::beak_pair {
                name: lent(name),
                value: lent(value),
            })
            .collect();
        let (scope, directory, from) = match &self.scope {
            Scope::In { directory, from } => (sys::beak_scope::BEAK_SCOPE_IN, directory, from.as_deref()),
            Scope::Under(directory) => (sys::beak_scope::BEAK_SCOPE_UNDER, directory, None),
        };
        sys::beak_selector {
            kinds: keep.slice(kinds),
            kind_count: self.kinds.len(),
            labels: keep.slice(labels),
            label_count: self.labels.len(),
            scope,
            directory: lent(directory),
            from: maybe(from),
        }
    }
}

/// A change a commit made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Changed {
    Deleted {
        path: Bytes,
    },
    Written {
        path: Bytes,
        kind: EntryKind,
        labels: Vec<(Bytes, Bytes)>,
        labels_before: Option<Vec<(Bytes, Bytes)>>,
    },
}

/// What a commit changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSet {
    pub seq: u64,
    pub time: i64,
    pub changes: Vec<Changed>,
}

/// The feed read on from a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    pub sets: Vec<ChangeSet>,
    pub through: u64,
    pub oldest: Option<u64>,
    pub latest: u64,
    pub missed: bool,
}

impl Snapshot {
    pub(crate) fn of(raw: *mut sys::beak_snapshot) -> Self {
        Self(Raw(raw))
    }

    /// The commit it is of.
    #[must_use]
    pub fn seq(&self) -> u64 {
        // SAFETY: the snapshot is open.
        unsafe { sys::beak_snapshot_seq(self.0.0) }
    }

    /// The entry at `path`; `None` where nothing has the name.
    ///
    /// # Errors
    /// A path through a file; the volume's failure.
    pub fn stat(&self, path: &[u8]) -> Result<Option<Entry>> {
        // SAFETY: the call is given what it asks; the entry is read, then let go of once.
        match object(|out| unsafe { sys::beak_snapshot_stat(self.0.0, lent(path), out) }) {
            Ok(entry) => unsafe {
                let read = Entry::of(&*entry);
                sys::beak_entry_free(entry);
                Ok(Some(read))
            },
            Err(err) if err.status == crate::Status::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// A page of the directory at `path`; `None` where there is no such directory.
    ///
    /// # Errors
    /// A path through a file; the volume's failure.
    pub fn list(&self, path: &[u8], range: &Range<'_>) -> Result<Option<Vec<Entry>>> {
        let lowered = range.lower();
        // SAFETY: the call is given what it asks; the entries are read, then let go of once.
        match object(|out| unsafe { sys::beak_snapshot_list(self.0.0, lent(path), &raw const lowered, out) }) {
            Ok(entries) => unsafe {
                let read = items((*entries).items, (*entries).len).iter().map(Entry::of).collect();
                sys::beak_entries_free(entries);
                Ok(Some(read))
            },
            Err(err) if err.status == crate::Status::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// The bytes of the file at `path` from `offset`, at most `length`; `None` where there is no such name.
    ///
    /// # Errors
    /// A path naming no file; the volume's failure.
    pub fn read(&self, path: &[u8], offset: u64, length: u64) -> Result<Option<Bytes>> {
        // SAFETY: the call is given what it asks.
        match buffer(|out| unsafe { sys::beak_snapshot_read(self.0.0, lent(path), offset, length, out) }) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(err) if err.status == crate::Status::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// The directories `query` asks for, `limit` at most.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn find(&self, query: &Query<'_>, limit: usize) -> Result<Vec<Found>> {
        let lowered = sys::beak_query {
            kind: lent(query.kind),
            name: lent(query.name),
            value: maybe(query.value),
            under: lent(query.under),
            after: maybe(query.after),
        };
        // SAFETY: the call is given what it asks; what it found is read, then let go of once.
        let found = object(|out| unsafe { sys::beak_snapshot_find(self.0.0, &raw const lowered, limit, out) })?;
        // SAFETY: as above.
        unsafe {
            let read = items((*found).items, (*found).len)
                .iter()
                .map(|found| Found {
                    path: copied(&found.path),
                    value: copied(&found.value),
                    entry: Entry::of(&found.entry),
                })
                .collect();
            sys::beak_found_list_free(found);
            Ok(read)
        }
    }

    /// The record of the process at `path`; `None` where no process is there.
    ///
    /// # Errors
    /// A damaged record; the volume's failure.
    pub fn record(&self, path: &[u8]) -> Result<Option<Record>> {
        // SAFETY: the call is given what it asks; the record is read, then let go of once.
        match object(|out| unsafe { sys::beak_snapshot_record(self.0.0, lent(path), out) }) {
            Ok(record) => unsafe {
                let read = Record::of(&*record);
                sys::beak_record_free(record);
                Ok(Some(read))
            },
            Err(err) if err.status == crate::Status::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// The changes of the commits after `after`, `limit` change sets read at most, those `filter` meets — every one
    /// where it is empty.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn feed(&self, after: u64, limit: usize, filter: &[Selector]) -> Result<Feed> {
        let mut keep = Keep::default();
        let selectors: Vec<sys::beak_selector> = filter.iter().map(|selector| selector.lower(&mut keep)).collect();
        // SAFETY: the call is given what it asks; the feed is read, then let go of once.
        let feed = object(|out| unsafe { sys::beak_snapshot_feed(self.0.0, after, limit, selectors.as_ptr(), selectors.len(), out) })?;
        // SAFETY: as above.
        unsafe {
            let sets = items((*feed).sets, (*feed).set_count)
                .iter()
                .map(|set| ChangeSet {
                    seq: set.seq,
                    time: set.time,
                    changes: items(set.changes, set.change_count).iter().map(changed).collect(),
                })
                .collect();
            let read = Feed {
                sets,
                through: (*feed).through,
                oldest: of_u64((*feed).oldest),
                latest: (*feed).latest,
                missed: (*feed).missed,
            };
            sys::beak_feed_free(feed);
            Ok(read)
        }
    }
}

fn changed(changed: &sys::beak_changed) -> Changed {
    if changed.deleted {
        return Changed::Deleted {
            path: copied(&changed.path),
        };
    }
    Changed::Written {
        path: copied(&changed.path),
        kind: kind(changed.kind),
        labels: names(&changed.labels),
        labels_before: changed.had_labels.then(|| names(&changed.labels_before)),
    }
}
