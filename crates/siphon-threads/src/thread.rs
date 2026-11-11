//! Where a thread is kept: a directory `siphon.thread:<id>` at the volume's root, labelled so the label index finds
//! threads by project, by parent and by how the person keeps them, annotated with how it runs and how it stands; in
//! it its inbox, its log and its main process.

use beak::layout;
use bytes::Bytes;

/// The kind of a thread's directory.
pub const KIND: &str = "siphon.thread";

/// The label of the project a thread belongs to, by its folder; empty for none. Every thread carries each of its
/// labels, empty where it has no value, so the index answers "of none" as it answers "of this one".
pub const PROJECT_LABEL: &str = "siphon.project";
/// The label of the thread that delegated this one, by its id; empty for a thread of its own.
pub const PARENT_LABEL: &str = "siphon.parent";
/// The labels of how the person keeps it: `true`, or empty for not.
pub const PINNED_LABEL: &str = "siphon.pinned";
pub const ARCHIVED_LABEL: &str = "siphon.archived";

/// The annotation of the call of its parent that delegated it.
pub const PARENT_CALL_ANNOTATION: &str = "siphon.parent_call";

/// The key of a thread's main process.
pub const MAIN_KEY: &str = "main";

/// The key of a thread's compaction, a process under its main process: one at a time.
pub const COMPACTION_KEY: &str = "compact";

/// The key of a thread's resume, a process under its main process that sleeps till the thread goes on.
pub const RESUME_KEY: &str = "resume";

/// The kind of a thread's turn, a process under its main process — which a stop of the thread asks to stop, and
/// nothing else beside it.
pub const TURN_KIND: &str = "siphon.turn";

/// The directory of the thread `id`.
#[must_use]
pub fn path(id: &str) -> Bytes {
    layout::child(b"", KIND.as_bytes(), id.as_bytes())
}

/// The id of the thread whose directory, or anything in it, is at `path`; none for a path outside every thread.
#[must_use]
pub fn id_of(path: &[u8]) -> Option<&str> {
    let rest = path.strip_prefix(KIND.as_bytes())?.strip_prefix(b":")?;
    let end = rest.iter().position(|&byte| byte == b'/').unwrap_or(rest.len());
    std::str::from_utf8(&rest[..end]).ok()
}

/// The thread's main process.
#[must_use]
pub fn main(id: &str) -> Bytes {
    layout::process(&path(id), MAIN_KEY.as_bytes())
}

/// `name` within the thread `id`'s directory.
#[must_use]
pub fn within(id: &str, name: &str) -> Bytes {
    [&path(id)[..], b"/", name.as_bytes()].concat().into()
}

/// A flag as a label holds it.
pub(crate) fn flag(on: bool) -> Bytes {
    if on { Bytes::from_static(b"true") } else { Bytes::new() }
}

/// Adds to `changes` the turn of the thread `id` asked to stop, where one runs — none of what runs beside it.
///
/// # Errors
/// The volume's failure.
pub fn stop_turn(snapshot: &beak::Snapshot, changes: &mut siphon_runtime::Changes, id: &str) -> beak::Result<()> {
    let main = main(id);
    let under = beak::Range {
        prefix: layout::PROCESS_KIND,
        limit: usize::MAX,
        ..beak::Range::default()
    };
    for child in snapshot.list(&main, &under)?.unwrap_or_default() {
        let path: Bytes = [&main[..], b"/", &child.name[..]].concat().into();
        let directory = snapshot.stat(&path)?.and_then(|entry| entry.directory);
        let kind = directory.as_ref().and_then(|directory| directory.label(layout::PROGRAM_LABEL));
        if kind.is_some_and(|kind| kind == TURN_KIND.as_bytes()) {
            changes.signal(path, beak::Signal::Terminate, Some(Bytes::from_static(b"stopped")));
        }
    }
    Ok(())
}
