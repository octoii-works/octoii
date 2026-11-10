//! The kernel: a volume on the local machine. Its events come as a stream; its changes are committed a list at a time;
//! its processes' phases are the host's, pulled as runs and finished with what they changed and how they ended.

use std::any::Any;
use std::path::PathBuf;
use std::sync::Arc;

use beak_sys as sys;
use bytes::Bytes;
use tokio::sync::mpsc;

use crate::ffi::{check, given, lent, maybe, maybe_i64, maybe_u64, object, of_i64, taken};
use crate::queue::Completion;
use crate::snapshot::{Selector, Snapshot};
use crate::{Beak, Result, Signal};

/// The kernel as Beak holds it, let go of as its last holder goes.
pub(crate) struct Raw(pub(crate) *mut sys::beak_kernel);

// SAFETY: a kernel is used from any thread at once, its parts behind their own locks.
unsafe impl Send for Raw {}
// SAFETY: as above.
unsafe impl Sync for Raw {}

impl Drop for Raw {
    fn drop(&mut self) {
        // SAFETY: the kernel Beak gave, let go of once.
        unsafe { sys::beak_kernel_free(self.0) };
    }
}

/// How a kernel opens: the directory its volume is kept in — memory, gone with it, where `None` — and the time.
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub directory: Option<PathBuf>,
    pub time: i64,
}

/// What runs the processes of a kind: their version, whether a stop handler runs, and how a phase that throws runs
/// again — `retries` times, waiting twice as long each time from the least delay to the most.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Program {
    pub version: u32,
    pub stop_handler: bool,
    pub retries: u32,
    pub retry_delay_min_ms: i64,
    pub retry_delay_max_ms: i64,
}

/// What the kernel tells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A commit was made.
    Committed(u64),
    /// A request a process made, for its handler: `retold` where it was made before the volume opened.
    Request {
        process: Bytes,
        key: Bytes,
        name: Bytes,
        payload: Bytes,
        started: u64,
        retold: bool,
    },
    /// A request cut off: its host answers it now, with what it has.
    Interrupted { process: Bytes, key: Bytes },
    /// A group a charge took past its limit.
    Limit { process: Bytes, name: Bytes },
    /// A process, and every one under it, sent a signal.
    Signalled { process: Bytes, signal: Signal },
}

/// What the kernel tells, as it tells it.
#[derive(Debug)]
pub struct Events(mpsc::UnboundedReceiver<Completion>);

impl Events {
    /// What it tells next; `None` once the kernel is let go of.
    pub async fn next(&mut self) -> Option<Event> {
        loop {
            let event = match self.0.recv().await? {
                Completion::Committed(seq) => Event::Committed(seq),
                Completion::Request {
                    process,
                    key,
                    name,
                    payload,
                    started,
                    retold,
                } => Event::Request {
                    process,
                    key,
                    name,
                    payload,
                    started,
                    retold,
                },
                Completion::Interrupted { process, key } => Event::Interrupted { process, key },
                Completion::Limit { process, name } => Event::Limit { process, name },
                Completion::Signalled { process, signal } => Event::Signalled { process, signal },
                Completion::Output(_) | Completion::Title(_) | Completion::Exit(_) => continue,
            };
            return Some(event);
        }
    }
}

/// Mail, or a request: its key, its name and the host's payload.
#[derive(Debug, Clone, Copy)]
pub struct Mail<'a> {
    pub key: &'a [u8],
    pub name: &'a [u8],
    pub payload: &'a [u8],
}

/// A change a commit makes.
#[derive(Debug, Clone, Copy)]
pub enum Change<'a> {
    Mkdir {
        path: &'a [u8],
    },
    Write {
        path: &'a [u8],
        bytes: &'a [u8],
    },
    WriteAt {
        path: &'a [u8],
        offset: u64,
        bytes: &'a [u8],
    },
    Append {
        path: &'a [u8],
        bytes: &'a [u8],
        append_only: bool,
    },
    Truncate {
        path: &'a [u8],
        length: u64,
    },
    Chmod {
        path: &'a [u8],
        executable: bool,
    },
    Seal {
        path: &'a [u8],
    },
    Rename {
        from: &'a [u8],
        to: &'a [u8],
    },
    Delete {
        path: &'a [u8],
    },
    Expire {
        path: &'a [u8],
        at: Option<i64>,
    },
    /// Labels and annotations set, each to its value, or removed where it has none.
    Meta {
        path: &'a [u8],
        labels: &'a [(&'a [u8], Option<&'a [u8]>)],
        annotations: &'a [(&'a [u8], Option<&'a [u8]>)],
    },
    Start {
        parent: &'a [u8],
        key: &'a [u8],
        kind: &'a [u8],
        version: u32,
        input: &'a [u8],
        checkpoint: &'a [u8],
        priority: u8,
        namespace: Option<&'a [u8]>,
    },
    Request {
        process: &'a [u8],
        mail: Mail<'a>,
        started: Option<u64>,
    },
    Send {
        process: &'a [u8],
        mail: Mail<'a>,
        started: Option<u64>,
    },
    Answer {
        process: &'a [u8],
        mail: Mail<'a>,
        started: Option<u64>,
    },
    Signal {
        process: &'a [u8],
        signal: Signal,
        reason: Option<&'a [u8]>,
        started: Option<u64>,
    },
    Charge {
        process: &'a [u8],
        counters: &'a [(&'a [u8], u64)],
    },
    Retry {
        process: &'a [u8],
    },
    Reap {
        process: &'a [u8],
    },
    Remove {
        path: &'a [u8],
        reason: Option<&'a [u8]>,
    },
    ForceRemove {
        path: &'a [u8],
        reason: Option<&'a [u8]>,
    },
}

/// What a call's lowered arguments point into, kept till the call returns.
#[derive(Default)]
pub(crate) struct Keep(Vec<Box<dyn Any>>);

impl Keep {
    /// `items`, kept: where they lie.
    pub(crate) fn slice<T: 'static>(&mut self, items: Vec<T>) -> *const T {
        let boxed = items.into_boxed_slice();
        let at = boxed.as_ptr();
        self.0.push(Box::new(boxed));
        at
    }

    pub(crate) fn one<T: 'static>(&mut self, item: T) -> *const T {
        self.slice(vec![item])
    }
}

fn mail(mail: Mail<'_>) -> sys::beak_mail {
    sys::beak_mail {
        key: lent(mail.key),
        name: lent(mail.name),
        payload: lent(mail.payload),
    }
}

fn names(keep: &mut Keep, changes: &[(&[u8], Option<&[u8]>)]) -> (*const sys::beak_name_change, usize) {
    let lowered: Vec<sys::beak_name_change> = changes
        .iter()
        .map(|(name, value)| sys::beak_name_change {
            name: lent(name),
            value: maybe(*value),
        })
        .collect();
    (keep.slice(lowered), changes.len())
}

impl Change<'_> {
    /// The change as the ABI takes it, what it points into kept in `keep`.
    #[allow(clippy::too_many_lines)]
    fn lower(&self, keep: &mut Keep) -> sys::beak_change {
        use sys::beak_change__bindgen_ty_1 as Body;
        use sys::beak_change_Tag as Tag;
        let (tag, body) = match *self {
            Change::Mkdir { path } => (
                Tag::BEAK_CHANGE_MKDIR,
                Body {
                    MKDIR: sys::beak_change_Mkdir_Body { path: lent(path) },
                },
            ),
            Change::Write { path, bytes } => (
                Tag::BEAK_CHANGE_WRITE,
                Body {
                    WRITE: sys::beak_change_Write_Body {
                        path: lent(path),
                        bytes: lent(bytes),
                    },
                },
            ),
            Change::WriteAt { path, offset, bytes } => (
                Tag::BEAK_CHANGE_WRITE_AT,
                Body {
                    WRITE_AT: sys::beak_change_WriteAt_Body {
                        path: lent(path),
                        offset,
                        bytes: lent(bytes),
                    },
                },
            ),
            Change::Append { path, bytes, append_only } => (
                Tag::BEAK_CHANGE_APPEND,
                Body {
                    APPEND: sys::beak_change_Append_Body {
                        path: lent(path),
                        bytes: lent(bytes),
                        append_only,
                    },
                },
            ),
            Change::Truncate { path, length } => (
                Tag::BEAK_CHANGE_TRUNCATE,
                Body {
                    TRUNCATE: sys::beak_change_Truncate_Body { path: lent(path), length },
                },
            ),
            Change::Chmod { path, executable } => (
                Tag::BEAK_CHANGE_CHMOD,
                Body {
                    CHMOD: sys::beak_change_Chmod_Body {
                        path: lent(path),
                        executable,
                    },
                },
            ),
            Change::Seal { path } => (
                Tag::BEAK_CHANGE_SEAL,
                Body {
                    SEAL: sys::beak_change_Seal_Body { path: lent(path) },
                },
            ),
            Change::Rename { from, to } => (
                Tag::BEAK_CHANGE_RENAME,
                Body {
                    RENAME: sys::beak_change_Rename_Body {
                        from: lent(from),
                        to: lent(to),
                    },
                },
            ),
            Change::Delete { path } => (
                Tag::BEAK_CHANGE_DELETE,
                Body {
                    DELETE: sys::beak_change_Delete_Body { path: lent(path) },
                },
            ),
            Change::Expire { path, at } => (
                Tag::BEAK_CHANGE_EXPIRE,
                Body {
                    EXPIRE: sys::beak_change_Expire_Body {
                        path: lent(path),
                        at: maybe_i64(at),
                    },
                },
            ),
            Change::Meta { path, labels, annotations } => {
                let (labels, label_count) = names(keep, labels);
                let (annotations, annotation_count) = names(keep, annotations);
                (
                    Tag::BEAK_CHANGE_META,
                    Body {
                        META: sys::beak_change_Meta_Body {
                            path: lent(path),
                            labels,
                            label_count,
                            annotations,
                            annotation_count,
                        },
                    },
                )
            }
            Change::Start {
                parent,
                key,
                kind,
                version,
                input,
                checkpoint,
                priority,
                namespace,
            } => (
                Tag::BEAK_CHANGE_START,
                Body {
                    START: sys::beak_change_Start_Body {
                        parent: lent(parent),
                        key: lent(key),
                        kind: lent(kind),
                        version,
                        input: lent(input),
                        checkpoint: lent(checkpoint),
                        priority,
                        namespace_: maybe(namespace),
                    },
                },
            ),
            Change::Request {
                process,
                mail: asked,
                started,
            } => (
                Tag::BEAK_CHANGE_REQUEST,
                Body {
                    REQUEST: sys::beak_change_Request_Body {
                        process: lent(process),
                        mail: mail(asked),
                        started: maybe_u64(started),
                    },
                },
            ),
            Change::Send {
                process,
                mail: sent,
                started,
            } => (
                Tag::BEAK_CHANGE_SEND,
                Body {
                    SEND: sys::beak_change_Send_Body {
                        process: lent(process),
                        mail: mail(sent),
                        started: maybe_u64(started),
                    },
                },
            ),
            Change::Answer {
                process,
                mail: answer,
                started,
            } => (
                Tag::BEAK_CHANGE_ANSWER,
                Body {
                    ANSWER: sys::beak_change_Answer_Body {
                        process: lent(process),
                        mail: mail(answer),
                        started: maybe_u64(started),
                    },
                },
            ),
            Change::Signal {
                process,
                signal,
                reason,
                started,
            } => (
                Tag::BEAK_CHANGE_SIGNAL,
                Body {
                    SIGNAL: sys::beak_change_Signal_Body {
                        process: lent(process),
                        signal: signal.raw(),
                        reason: maybe(reason),
                        started: maybe_u64(started),
                    },
                },
            ),
            Change::Charge { process, counters } => {
                let lowered: Vec<sys::beak_counter> = counters
                    .iter()
                    .map(|(name, value)| sys::beak_counter {
                        name: lent(name),
                        value: *value,
                    })
                    .collect();
                (
                    Tag::BEAK_CHANGE_CHARGE,
                    Body {
                        CHARGE: sys::beak_change_Charge_Body {
                            process: lent(process),
                            counters: keep.slice(lowered),
                            count: counters.len(),
                        },
                    },
                )
            }
            Change::Retry { process } => (
                Tag::BEAK_CHANGE_RETRY,
                Body {
                    RETRY: sys::beak_change_Retry_Body { process: lent(process) },
                },
            ),
            Change::Reap { process } => (
                Tag::BEAK_CHANGE_REAP,
                Body {
                    REAP: sys::beak_change_Reap_Body { process: lent(process) },
                },
            ),
            Change::Remove { path, reason } => (
                Tag::BEAK_CHANGE_REMOVE,
                Body {
                    REMOVE: sys::beak_change_Remove_Body {
                        path: lent(path),
                        reason: maybe(reason),
                    },
                },
            ),
            Change::ForceRemove { path, reason } => (
                Tag::BEAK_CHANGE_FORCE_REMOVE,
                Body {
                    FORCE_REMOVE: sys::beak_change_ForceRemove_Body {
                        path: lent(path),
                        reason: maybe(reason),
                    },
                },
            ),
        };
        sys::beak_change {
            tag,
            __bindgen_anon_1: body,
        }
    }
}

fn lowered(changes: &[Change<'_>], keep: &mut Keep) -> Vec<sys::beak_change> {
    changes.iter().map(|change| change.lower(keep)).collect()
}

/// A run for the host to start: the phase of the process at `path`, the one the commit `started` started — or its stop
/// handler — from its input and checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub path: Bytes,
    pub started: u64,
    pub stop_handler: bool,
    pub kind: Bytes,
    pub version: u32,
    pub input: Bytes,
    pub checkpoint: Bytes,
}

/// What a process waits for, the first met ending the wait: its children, by their keys, every one ending; directories
/// changing from the commit given, `None` for one not there; a directory a selector picks; a deadline.
#[derive(Debug, Clone, Copy, Default)]
pub struct Wait<'a> {
    pub children: &'a [&'a [u8]],
    pub changes: &'a [(&'a [u8], Option<u64>)],
    pub selected: Option<&'a Selector>,
    pub deadline: Option<i64>,
}

/// How a phase, or a stop handler, ended.
#[derive(Debug, Clone, Copy)]
pub enum Ending<'a> {
    Next(&'a [u8]),
    Wait { checkpoint: &'a [u8], wait: Wait<'a> },
    Done(&'a [u8]),
    Fail(&'a [u8]),
    Threw(&'a [u8]),
    Stopped(Option<&'a [u8]>),
}

impl Ending<'_> {
    fn lower(&self, keep: &mut Keep) -> sys::beak_ending {
        use sys::beak_ending__bindgen_ty_1 as Body;
        use sys::beak_ending_Tag as Tag;
        let (tag, body) = match *self {
            Ending::Next(checkpoint) => (
                Tag::BEAK_ENDING_NEXT,
                Body {
                    NEXT: sys::beak_ending_Next_Body {
                        checkpoint: lent(checkpoint),
                    },
                },
            ),
            Ending::Wait { checkpoint, wait } => {
                let children: Vec<sys::beak_bytes> = wait.children.iter().map(|key| lent(key)).collect();
                let changes: Vec<sys::beak_version> = wait
                    .changes
                    .iter()
                    .map(|(path, seq)| sys::beak_version {
                        path: lent(path),
                        changed_seq: maybe_u64(*seq),
                    })
                    .collect();
                let selected = wait.selected.map_or(std::ptr::null(), |selector| {
                    let lowered = selector.lower(keep);
                    keep.one(lowered)
                });
                let lowered = sys::beak_wait {
                    children: keep.slice(children),
                    child_count: wait.children.len(),
                    changes: keep.slice(changes),
                    change_count: wait.changes.len(),
                    selected,
                    deadline: maybe_i64(wait.deadline),
                };
                (
                    Tag::BEAK_ENDING_WAIT,
                    Body {
                        WAIT: sys::beak_ending_Wait_Body {
                            checkpoint: lent(checkpoint),
                            wait: lowered,
                        },
                    },
                )
            }
            Ending::Done(result) => (
                Tag::BEAK_ENDING_DONE,
                Body {
                    DONE: sys::beak_ending_Done_Body { result: lent(result) },
                },
            ),
            Ending::Fail(error) => (
                Tag::BEAK_ENDING_FAIL,
                Body {
                    FAIL: sys::beak_ending_Fail_Body { error: lent(error) },
                },
            ),
            Ending::Threw(message) => (
                Tag::BEAK_ENDING_THREW,
                Body {
                    THREW: sys::beak_ending_Threw_Body { message: lent(message) },
                },
            ),
            Ending::Stopped(result) => (
                Tag::BEAK_ENDING_STOPPED,
                Body {
                    STOPPED: sys::beak_ending_Stopped_Body { result: maybe(result) },
                },
            ),
        };
        sys::beak_ending {
            tag,
            __bindgen_anon_1: body,
        }
    }
}

/// A kernel.
#[derive(Clone)]
pub struct Kernel {
    pub(crate) raw: Arc<Raw>,
    pub(crate) beak: Beak,
}

impl std::fmt::Debug for Kernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Kernel")
    }
}

impl Kernel {
    /// Opens a kernel as `options` say, and what it tells: first every request still waiting, retold.
    ///
    /// # Errors
    /// The volume's failure, as another holding its directory.
    pub fn open(beak: &Beak, options: &Options) -> Result<(Self, Events)> {
        let directory = options
            .directory
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let lowered = sys::beak_kernel_options {
            directory: lent(directory.as_bytes()),
            time: options.time,
        };
        let (tag, completion) = beak.router.bind();
        // SAFETY: the call is given what it asks, alive for it.
        let raw = object(|out| unsafe { sys::beak_kernel_open(&raw const lowered, beak.router.raw.0, tag, out) })?;
        let kernel = Self {
            raw: Arc::new(Raw(raw)),
            beak: beak.clone(),
        };
        Ok((kernel, Events(completion)))
    }

    pub(crate) fn raw(&self) -> *const sys::beak_kernel {
        self.raw.0
    }

    /// Registers the program of `kind`.
    ///
    /// # Errors
    /// None that a kernel gives.
    pub fn register(&self, kind: &[u8], program: Program) -> Result<()> {
        let lowered = sys::beak_program {
            version: program.version,
            stop_handler: program.stop_handler,
            retries: program.retries,
            retry_delay_min_ms: program.retry_delay_min_ms,
            retry_delay_max_ms: program.retry_delay_max_ms,
        };
        // SAFETY: the call is given what it asks, alive for it.
        check(unsafe { sys::beak_kernel_register(self.raw(), lent(kind), &raw const lowered) })
    }

    /// Commits `changes` at `time`, all or none: its sequence number.
    ///
    /// # Errors
    /// The first change that cannot apply, by its status and reason; the commit's failure.
    pub fn commit(&self, time: i64, changes: &[Change<'_>]) -> Result<u64> {
        let mut keep = Keep::default();
        let lowered = lowered(changes, &mut keep);
        // SAFETY: the call is given what it asks, alive for it.
        given(|seq| unsafe { sys::beak_kernel_commit(self.raw(), time, lowered.as_ptr(), lowered.len(), seq) })
    }

    /// The runs to start at `time`.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn runnable(&self, time: i64) -> Result<Vec<Run>> {
        // SAFETY: the call is given where to put the runs.
        let runs = object(|out| unsafe { sys::beak_kernel_runnable(self.raw(), time, out) })?;
        // SAFETY: Beak gave the runs; they are read, then let go of once.
        unsafe {
            let items = if (*runs).len == 0 {
                &[][..]
            } else {
                std::slice::from_raw_parts((*runs).items, (*runs).len)
            };
            let out = items
                .iter()
                .map(|run| Run {
                    path: crate::ffi::copied(&run.path),
                    started: run.started,
                    stop_handler: run.stop_handler,
                    kind: crate::ffi::copied(&run.kind),
                    version: run.version,
                    input: crate::ffi::copied(&run.input),
                    checkpoint: crate::ffi::copied(&run.checkpoint),
                })
                .collect();
            sys::beak_runs_free(runs);
            Ok(out)
        }
    }

    /// Finishes `run` at `time`: `changes` and its `ending` in one commit. Where a change cannot apply, the run ends as
    /// thrown, nothing of the changes made.
    ///
    /// # Errors
    /// `Refused` with `stale` for a run of a process started again since, `not-admitted` for one not running.
    pub fn finish(&self, time: i64, run: &Run, ending: &Ending<'_>, changes: &[Change<'_>]) -> Result<u64> {
        let mut keep = Keep::default();
        let name = sys::beak_run_name {
            path: lent(&run.path),
            started: run.started,
            stop_handler: run.stop_handler,
            kind: lent(&run.kind),
        };
        let ending = ending.lower(&mut keep);
        let lowered = lowered(changes, &mut keep);
        // SAFETY: the call is given what it asks, alive for it.
        given(|seq| unsafe {
            sys::beak_kernel_finish(
                self.raw(),
                time,
                &raw const name,
                &raw const ending,
                lowered.as_ptr(),
                lowered.len(),
                seq,
            )
        })
    }

    /// When the kernel is next due; `None` for never.
    ///
    /// # Errors
    /// None that a kernel gives.
    pub fn next_due(&self) -> Result<Option<i64>> {
        // SAFETY: the call is given where to put the time.
        Ok(of_i64(given(|out| unsafe { sys::beak_kernel_next_due(self.raw(), out) })?))
    }

    /// A new id made at `time`, as text.
    ///
    /// # Errors
    /// Past the last id there is.
    pub fn new_id(&self, time: i64) -> Result<String> {
        // SAFETY: the call is given where to put the id.
        let id: sys::beak_id = given(|out| unsafe { sys::beak_kernel_new_id(self.raw(), time, out) })?;
        Ok(String::from_utf8_lossy(&id.text).into_owned())
    }

    /// Makes every commit so far survive a power loss.
    ///
    /// # Errors
    /// The device's failure.
    pub fn sync(&self) -> Result<()> {
        // SAFETY: the kernel is open.
        check(unsafe { sys::beak_kernel_sync(self.raw()) })
    }

    /// The bytes the volume keeps on its device.
    ///
    /// # Errors
    /// None that a kernel gives.
    pub fn usage(&self) -> Result<u64> {
        // SAFETY: the call is given where to put the count.
        given(|out| unsafe { sys::beak_kernel_usage(self.raw(), out) })
    }

    /// Gives back what no snapshot still held reaches: the objects kept and those forgotten.
    ///
    /// # Errors
    /// The device's failure.
    pub fn collect(&self) -> Result<(u64, u64)> {
        // SAFETY: the call is given where to put the counts.
        let collected: sys::beak_collected = given(|out| unsafe { sys::beak_kernel_collect(self.raw(), out) })?;
        Ok((collected.kept, collected.forgotten))
    }

    /// The volume as the last commit left it.
    ///
    /// # Errors
    /// None that a kernel gives.
    pub fn snapshot(&self) -> Result<Snapshot> {
        // SAFETY: the call is given where to put the snapshot.
        let raw = object(|out| unsafe { sys::beak_kernel_snapshot(self.raw(), out) })?;
        Ok(Snapshot::of(raw))
    }
}

/// The bytes a call gives into a buffer.
pub(crate) fn buffer(call: impl FnOnce(*mut sys::beak_buffer) -> sys::beak_status) -> Result<Bytes> {
    let mut out = crate::ffi::empty();
    check(call(&raw mut out))?;
    Ok(taken(out))
}
