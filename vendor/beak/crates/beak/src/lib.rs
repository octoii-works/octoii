//! Beak for Rust: the kernel, its volume, its namespaces and its commands as safe objects over Beak's C ABI, what
//! happens to each a stream of its own. A `Beak` holds the completion queue every object is bound to and the task that
//! routes it, on the Tokio runtime it is made on; objects made from it tell their own.

mod command;
mod ffi;
mod kernel;
mod namespace;
mod queue;
mod snapshot;

use std::sync::Arc;

use beak_sys as sys;

pub use command::{Command, CommandEvent, CommandEvents, Kept, Program as Exec, Spec, Stdio};
pub use kernel::{Change, Ending, Event, Events, Kernel, Mail, Options, Program, Run, Wait};
pub use namespace::{Dirent, FileKind, FileSystem, LabelView, Namespace, Stat};
pub use snapshot::{
    ChangeSet, Changed, Directory, Entry, EntryKind, Feed, Found, Held, Outcome, ProcessState, Query, Range, Record, Scope, Selector,
    Snapshot,
};

/// How a call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    NotFound,
    Refused,
    Invalid,
    Exists,
    ReadOnly,
    Denied,
    Unsupported,
    MissingProgram,
    Closed,
    Failed,
    Panicked,
}

/// A call's failure: its status, and the reason it gave — a refusal's own name, as `stale`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{status:?}: {reason}")]
pub struct Error {
    pub status: Status,
    pub reason: String,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A signal, as Unix's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Hangup,
    Interrupt,
    Terminate,
    Kill,
    Stop,
    Continue,
}

impl Signal {
    fn of(signal: sys::beak_signal) -> Self {
        match signal {
            sys::beak_signal::BEAK_SIGNAL_HANGUP => Self::Hangup,
            sys::beak_signal::BEAK_SIGNAL_INTERRUPT => Self::Interrupt,
            sys::beak_signal::BEAK_SIGNAL_TERMINATE => Self::Terminate,
            sys::beak_signal::BEAK_SIGNAL_KILL => Self::Kill,
            sys::beak_signal::BEAK_SIGNAL_STOP => Self::Stop,
            sys::beak_signal::BEAK_SIGNAL_CONTINUE => Self::Continue,
        }
    }

    fn raw(self) -> sys::beak_signal {
        match self {
            Self::Hangup => sys::beak_signal::BEAK_SIGNAL_HANGUP,
            Self::Interrupt => sys::beak_signal::BEAK_SIGNAL_INTERRUPT,
            Self::Terminate => sys::beak_signal::BEAK_SIGNAL_TERMINATE,
            Self::Kill => sys::beak_signal::BEAK_SIGNAL_KILL,
            Self::Stop => sys::beak_signal::BEAK_SIGNAL_STOP,
            Self::Continue => sys::beak_signal::BEAK_SIGNAL_CONTINUE,
        }
    }
}

/// How a command ended: the code it exited with, or the signal that ended it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    Signal(i32),
}

impl Exit {
    fn of(exit: sys::beak_exit) -> Self {
        if exit.signalled {
            Self::Signal(exit.value)
        } else {
            Self::Code(exit.value)
        }
    }
}

/// Beak, on the Tokio runtime it is made on: the queue every object made from it is bound to.
#[derive(Clone)]
pub struct Beak {
    router: Arc<queue::Router>,
}

impl std::fmt::Debug for Beak {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Beak")
    }
}

impl Beak {
    /// Beak, routing what it tells on the current Tokio runtime.
    ///
    /// # Errors
    /// Where the queue cannot be opened.
    ///
    /// # Panics
    /// Outside a Tokio runtime.
    pub fn new() -> Result<Self> {
        Ok(Self {
            router: queue::Router::start()?,
        })
    }
}
