//! The machine the host runs on, as a piece of work reaches it — a thread's call, a client's read: its whole tree
//! mounted writable at the root of a namespace of Beak's, as a shell reaches every file of its machine, but what the
//! host keeps for itself, which nothing reaches through it; the threads shown, read alone from the volume, where the
//! host lays them out; and its programs run in the namespace, held to what it reaches.

use std::path::{Path, PathBuf};
use std::time::Duration;

use beak::{CommandEvent, Exec, FileSystem, Kernel, LabelView, Namespace, Spec, Stdio};
use bytes::{Bytes, BytesMut};
use tokio_util::sync::CancellationToken;

use crate::paths::Base;

/// The machine as the host gives it: the home folder of the person it runs for and their shell; their own ignore file,
/// git's `core.excludesFile`; what the host keeps for itself, by absolute paths, which nothing reaches through it; and
/// the folder the threads are shown in, where the host lays them out.
#[derive(Debug, Clone, Default)]
pub struct Host {
    pub home: Option<PathBuf>,
    pub shell: Option<PathBuf>,
    pub excludes: Option<PathBuf>,
    pub hidden: Vec<PathBuf>,
    pub threads: Option<PathBuf>,
}

/// What shows the threads: the kind of a thread's directory, and the label each carries of its project.
#[derive(Debug, Clone, Copy)]
pub struct ThreadsView<'a> {
    pub kind: &'a str,
    pub label: &'a str,
}

/// The machine, opened for a piece of work.
#[derive(Debug)]
pub struct Machine {
    pub(crate) namespace: Namespace,
    pub(crate) kernel: Kernel,
    pub base: Base,
    pub(crate) excludes: Option<PathBuf>,
}

/// Why a program did not run to its end.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("no program {0} on the machine")]
    MissingProgram(String),
    #[error("it ran past its {0:?}")]
    TimedOut(Duration),
    #[error("it wrote more than {0} bytes")]
    OutputTooLarge(usize),
    #[error("it was stopped")]
    Stopped,
    #[error(transparent)]
    Beak(#[from] beak::Error),
}

/// A program to run to its end: its arguments, the program first, a bare name looked for along the machine's `PATH`;
/// the folder it runs in; the variables set over the machine's own; how long it may run; and how much of its output is
/// taken, past which it is ended.
#[derive(Debug, Clone)]
pub struct Program {
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub set: Vec<(String, String)>,
    pub timeout: Duration,
    pub output_max: usize,
}

impl Program {
    /// `argv` run in `cwd`, for 30 seconds at most, its output taken to 16 MiB.
    #[must_use]
    pub fn new(argv: Vec<String>, cwd: impl Into<PathBuf>) -> Self {
        Self {
            argv,
            cwd: cwd.into(),
            set: Vec::new(),
            timeout: Duration::from_secs(30),
            output_max: 16 << 20,
        }
    }
}

/// How a program ran: how it ended, what it wrote to its output, whole, and the head and tail of its errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    pub exit: beak::Exit,
    pub output: Bytes,
    pub errors: Bytes,
}

impl Machine {
    /// The machine `host` gives, opened for work whose paths start at `base`: its tree mounted writable, what the host
    /// keeps hidden, and the threads shown where the host lays them out, where `threads` says how.
    ///
    /// # Errors
    /// Beak's failure to make or mount the namespace.
    pub fn open(kernel: &Kernel, host: &Host, base: Base, threads: Option<ThreadsView<'_>>) -> beak::Result<Self> {
        let namespace = Namespace::new()?;
        namespace.mount("", &FileSystem::Host("/"), true)?;
        for place in &host.hidden {
            namespace.mount(&inside(place), &FileSystem::Inaccessible, false)?;
        }
        if let (Some(view), Some(folder)) = (threads, &host.threads) {
            let label = FileSystem::Label {
                kernel,
                kind: view.kind,
                label: view.label,
                view: LabelView::AnyValue,
                under: b"",
            };
            namespace.mount(&inside(folder), &label, false)?;
        }
        Ok(Self {
            namespace,
            kernel: kernel.clone(),
            base,
            excludes: host.excludes.clone(),
        })
    }

    /// The namespace's path of the place `given` names, as the model writes it.
    #[must_use]
    pub fn path(&self, given: &str) -> String {
        inside(&self.base.resolve(given))
    }

    /// The namespace's `path` as the model reads it.
    #[must_use]
    pub fn shown(&self, path: &str) -> String {
        self.base.shown(&Path::new("/").join(path))
    }

    /// The namespace the machine is reached through.
    #[must_use]
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }

    /// Runs `program` to its end at `time`, held to what the namespace reaches, recorded under `under`: its output
    /// taken whole as it comes, its errors kept; ended where it runs past its time, writes past what is taken, or
    /// `cancelled` says stop.
    ///
    /// # Errors
    /// `MissingProgram`, `TimedOut`, `OutputTooLarge`, `Stopped`; Beak's failure to start it.
    pub async fn run(&self, time: i64, under: &[u8], program: &Program, cancelled: &CancellationToken) -> Result<Ran, RunError> {
        let spec = Spec {
            set: program.set.clone(),
            stdio: Stdio::Coprocess,
            ..Spec::new(Exec::Argv(program.argv.clone()), program.cwd.to_string_lossy())
        };
        let (command, mut events) = self.kernel.spawn(time, under, &self.namespace, &spec).map_err(|error| {
            if error.status == beak::Status::MissingProgram {
                RunError::MissingProgram(program.argv.first().cloned().unwrap_or_default())
            } else {
                RunError::Beak(error)
            }
        })?;
        command.close_input()?;
        let mut output = BytesMut::new();
        let deadline = tokio::time::sleep(program.timeout);
        tokio::pin!(deadline);
        let exit = loop {
            tokio::select! {
                event = events.next() => match event {
                    Some(CommandEvent::Output(bytes)) => {
                        if output.len() + bytes.len() > program.output_max {
                            command.signal(beak::Signal::Kill)?;
                            return Err(RunError::OutputTooLarge(program.output_max));
                        }
                        output.extend_from_slice(&bytes);
                    }
                    Some(CommandEvent::Title(_)) => {}
                    Some(CommandEvent::Exit(exit)) => break exit,
                    None => return Err(RunError::Stopped),
                },
                () = &mut deadline => {
                    command.signal(beak::Signal::Kill)?;
                    return Err(RunError::TimedOut(program.timeout));
                }
                () = cancelled.cancelled() => {
                    command.signal(beak::Signal::Kill)?;
                    return Err(RunError::Stopped);
                }
            }
        };
        Ok(Ran {
            exit,
            output: output.freeze(),
            errors: command.output()?,
        })
    }
}

/// The namespace's path of the absolute `place`: the place without its root.
pub(crate) fn inside(place: &Path) -> String {
    place.to_string_lossy().trim_start_matches('/').to_owned()
}
