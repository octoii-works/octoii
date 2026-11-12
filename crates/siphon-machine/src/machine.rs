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
/// git's `core.excludesFile`; what the host keeps for itself, by absolute paths, which nothing reaches through it; the
/// folder the threads are shown in, where the host lays them out; the folder a thread of no folder works in; the folder
/// of the person's memory, a folder for each project; and the folder of the threads' scratchpads, in the system's
/// temporary folder, a folder for each thread.
#[derive(Debug, Clone, Default)]
pub struct Host {
    pub home: Option<PathBuf>,
    pub shell: Option<PathBuf>,
    pub excludes: Option<PathBuf>,
    /// The machine's temporary folders, which work held to its workspace writes besides it.
    pub temporary: Vec<PathBuf>,
    pub hidden: Vec<PathBuf>,
    pub threads: Option<PathBuf>,
    /// The folder a thread of no folder of its own works in, no project's.
    pub workspace: Option<PathBuf>,
    pub memory: Option<PathBuf>,
    pub scratchpads: Option<PathBuf>,
}

impl Host {
    /// The folder a thread works in: `given`, its own, else the folder of the threads of none, else the person's home,
    /// else the machine's root.
    #[must_use]
    pub fn workspace_of(&self, given: Option<&str>) -> PathBuf {
        given
            .map(PathBuf::from)
            .or_else(|| self.workspace.clone())
            .or_else(|| self.home.clone())
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    /// The folder of the person's memory of the project at `project`: a folder named after its path, as a path is
    /// spelt with each separator a `-`.
    #[must_use]
    pub fn memory_of(&self, project: &Path) -> Option<PathBuf> {
        let name = project.to_string_lossy().replace(['/', '\\', ':'], "-");
        self.memory.as_ref().map(|memory| memory.join(name))
    }

    /// The places beside its workspace that the thread `thread`'s work writes as freely as it: the person's memory, the
    /// thread's scratchpad and the machine's temporary folders.
    #[must_use]
    pub fn places_of(&self, thread: &str) -> Vec<PathBuf> {
        let mut places: Vec<PathBuf> = self.memory.iter().cloned().collect();
        places.extend(self.scratchpad_of(thread));
        places.extend(self.temporary.iter().cloned());
        places
    }

    /// The scratchpad of the thread `thread`: a folder for what its work leaves that is not to be kept.
    #[must_use]
    pub fn scratchpad_of(&self, thread: &str) -> Option<PathBuf> {
        self.scratchpads.as_ref().map(|scratchpads| scratchpads.join(thread))
    }
}

/// Where work on the machine may write: anywhere; or within the places given alone — a thread's workspace and the
/// machine's temporary folders — the rest read alone, as the system's sandbox keeps a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Writes {
    Anywhere,
    Within(Vec<PathBuf>),
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
    pub errors: Errors,
}

/// Where a program's errors go: kept apart, their head and tail told after it ends; or into its output, as a shell's
/// `2>&1` sends them, one stream in the order written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Errors {
    #[default]
    Kept,
    WithOutput,
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
            errors: Errors::Kept,
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
    /// The machine `host` gives, opened for work whose paths start at `base`: its tree mounted, written as `writes`
    /// says, what the host keeps hidden, and the threads shown where the host lays them out, where `threads` says how.
    ///
    /// # Errors
    /// Beak's failure to make or mount the namespace.
    pub fn open(kernel: &Kernel, host: &Host, base: Base, writes: &Writes, threads: Option<ThreadsView<'_>>) -> beak::Result<Self> {
        let namespace = Namespace::new()?;
        namespace.mount("", &FileSystem::Host("/"), *writes == Writes::Anywhere)?;
        if let Writes::Within(places) = writes {
            for place in places {
                namespace.mount(&inside(place), &FileSystem::Host(&place.to_string_lossy()), true)?;
            }
        }
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

    /// Binds the host's folder `place` back, writable, over what hides it, as `mount --bind` does: an extension's own
    /// folder, among what the host keeps for itself, reached by the extension's own programs alone.
    ///
    /// # Errors
    /// Beak's failure to mount it.
    pub fn bind(&self, place: &Path) -> beak::Result<()> {
        self.namespace
            .mount(&inside(place), &FileSystem::Host(&place.to_string_lossy()), true)
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
            stdio: match program.errors {
                Errors::Kept => Stdio::Coprocess,
                Errors::WithOutput => Stdio::Pipe,
            },
            ..Spec::new(Exec::Argv(program.argv.clone()), program.cwd.to_string_lossy())
        };
        let (command, mut events) = self.spawn(time, under, &spec)?;
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
        let errors = match program.errors {
            Errors::Kept => command.output()?,
            Errors::WithOutput => Bytes::new(),
        };
        Ok(Ran {
            exit,
            output: output.freeze(),
            errors,
        })
    }
}

impl Machine {
    /// Starts the command `spec` describes at `time`, held to what the namespace reaches, recorded under `under`.
    pub(crate) fn spawn(&self, time: i64, under: &[u8], spec: &Spec) -> Result<(beak::Command, beak::CommandEvents), RunError> {
        self.kernel.spawn(time, under, &self.namespace, spec).map_err(|error| {
            if error.status == beak::Status::MissingProgram {
                let program = match &spec.exec {
                    Exec::Argv(argv) => argv.first().cloned().unwrap_or_default(),
                    Exec::Shell(line) => line.clone(),
                };
                RunError::MissingProgram(program)
            } else {
                RunError::Beak(error)
            }
        })
    }
}

/// The namespace's path of the absolute `place`: the place without its root.
pub(crate) fn inside(place: &Path) -> String {
    place.to_string_lossy().trim_start_matches('/').to_owned()
}
