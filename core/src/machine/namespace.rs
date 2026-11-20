//! The machine the host runs on, as a piece of work reaches it — a thread's call, a client's read: its whole tree
//! mounted writable at the root of a namespace of Beak's, as a shell reaches every file of its machine, but what the
//! host keeps for itself, which nothing reaches through it; the threads shown, read alone from the volume, where the
//! host lays them out; and its programs run in the namespace, held to what it reaches.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use octoii_siphon_lib::{Command, CommandEvent, CommandEvents, Exec, Namespace, RuntimeHandle, Signal, Spec, Stdio};
use tokio_util::sync::CancellationToken;

use crate::machine::commands::Launcher;
use crate::machine::paths::Base;

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

    /// The places of the machine's own beside its workspace that the thread `thread`'s work writes as freely as it:
    /// the thread's scratchpad and the machine's temporary folders.
    #[must_use]
    pub fn places_of(&self, thread: &str) -> Vec<PathBuf> {
        let mut places: Vec<PathBuf> = self.scratchpad_of(thread).into_iter().collect();
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
    #[error("command output at offsets {from}..{through} was lost")]
    OutputLost { from: u64, through: u64 },
    #[error("it was stopped")]
    Stopped,
    #[error(transparent)]
    Beak(#[from] octoii_siphon_lib::Error),
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

    fn spec(&self) -> Spec {
        let mut spec = Spec::new(Exec::Argv(self.argv.clone()), self.cwd.to_string_lossy());
        spec.set = (!self.set.is_empty()).then(|| self.set.clone());
        spec.stdio = Some(match self.errors {
            Errors::Kept => Stdio::Coprocess,
            Errors::WithOutput => {
                // Keep the prefix so the collector can recover any skipped events within its limit.
                spec.head_bytes = Some(self.output_max);
                spec.tail_bytes = Some(0);
                Stdio::Pipe
            }
        });
        spec
    }
}

/// How a program ran: how it ended, what it wrote to its output, whole, and the head and tail of its errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    pub exit: octoii_siphon_lib::Exit,
    pub output: Bytes,
    pub errors: Bytes,
}

impl Machine {
    /// The machine `host` gives, opened for work whose paths start at `base`: its tree mounted, written as `writes`
    /// says, what the host keeps hidden, and the threads shown where the host lays them out, where `threads` says how.
    ///
    /// # Errors
    /// Beak's failure to make or mount the namespace.
    pub fn open(
        runtime: &RuntimeHandle,
        host: &Host,
        base: Base,
        writes: &Writes,
        threads: Option<ThreadsView<'_>>,
    ) -> octoii_siphon_lib::Result<Self> {
        let namespace = Namespace::new()?;
        namespace.mount_host("", "/", *writes == Writes::Anywhere)?;
        if let Writes::Within(places) = writes {
            for place in places {
                namespace.mount_host(&inside(place), &place.to_string_lossy(), true)?;
            }
        }
        for place in &host.hidden {
            namespace.hide(&inside(place))?;
        }
        if let (Some(view), Some(folder)) = (threads, &host.threads) {
            namespace.mount_labels(runtime, &inside(folder), view.kind, view.label, b"")?;
        }
        Ok(Self {
            namespace,
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
    pub fn bind(&self, place: &Path) -> octoii_siphon_lib::Result<()> {
        self.namespace.mount_host(&inside(place), &place.to_string_lossy(), true)
    }

    /// The namespace the machine is reached through.
    #[must_use]
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }

    /// Runs `program` with `commands` in the namespace. Collects its output until it exits, exceeds its limits, or
    /// `cancelled` says stop.
    ///
    /// # Errors
    /// A launch failure, broken output, or the program was stopped.
    pub async fn run(&self, commands: &dyn Launcher, program: &Program, cancelled: &CancellationToken) -> Result<Ran, RunError> {
        if cancelled.is_cancelled() {
            return Err(RunError::Stopped);
        }
        let (command, events) = self.spawn_with(commands, &program.spec())?;
        command.close_input()?;
        let result = tokio::select! {
            ran = collect(&command, events, program) => ran,
            () = tokio::time::sleep(program.timeout) => Err(RunError::TimedOut(program.timeout)),
            () = cancelled.cancelled() => Err(RunError::Stopped),
        };
        if result.is_err() {
            let _ = command.signal(Signal::Kill);
        }
        result
    }

    pub(super) fn spawn_with(&self, commands: &dyn Launcher, spec: &Spec) -> Result<(Command, CommandEvents), RunError> {
        commands.spawn(&self.namespace, spec).map_err(|error| {
            if error.status() == octoii_siphon_lib::Status::MissingProgram {
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

async fn collect(command: &Command, mut events: CommandEvents, program: &Program) -> Result<Ran, RunError> {
    let mut output = BytesMut::new();
    let exit = loop {
        match events.next().await? {
            Some(CommandEvent::Output(chunk)) => {
                if chunk.offset != output.len() as u64 {
                    return Err(RunError::OutputLost {
                        from: output.len() as u64,
                        through: chunk.offset,
                    });
                }
                if chunk.bytes.len() > program.output_max - output.len() {
                    return Err(RunError::OutputTooLarge(program.output_max));
                }
                output.extend_from_slice(&chunk.bytes);
            }
            Some(CommandEvent::Gap { through, .. }) if program.errors == Errors::WithOutput => {
                recover_output(command, &mut output, through, program.output_max)?;
            }
            Some(CommandEvent::Gap { from, through }) => return Err(RunError::OutputLost { from, through }),
            Some(CommandEvent::Title(_)) => {}
            Some(CommandEvent::Exit(exit)) => break exit,
            None => return Err(RunError::Stopped),
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

fn recover_output(command: &Command, output: &mut BytesMut, through: u64, limit: usize) -> Result<(), RunError> {
    let through_len = usize::try_from(through).map_err(|_| RunError::OutputTooLarge(limit))?;
    if through_len > limit {
        return Err(RunError::OutputTooLarge(limit));
    }
    while output.len() < through_len {
        let from = output.len() as u64;
        let chunk = command.read_output_at(from)?;
        if chunk.offset != from || chunk.bytes.is_empty() {
            return Err(RunError::OutputLost { from, through });
        }
        let remaining = through_len - output.len();
        output.extend_from_slice(&chunk.bytes[..chunk.bytes.len().min(remaining)]);
    }
    Ok(())
}

/// The namespace's path of the absolute `place`: the place without its root.
#[must_use]
pub fn inside(place: &Path) -> String {
    place.to_string_lossy().trim_start_matches('/').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_pipe_recovers_retained_output_when_its_live_reader_is_behind() {
        let runtime = crate::testing::memory_runtime().expect("started");
        let commands = crate::testing::commands(&runtime);
        let machine = commands
            .open(
                &Host::default(),
                Base {
                    workspace: "/".into(),
                    home: None,
                },
                &Writes::Anywhere,
            )
            .expect("a namespace");
        let program = Program {
            errors: Errors::WithOutput,
            ..Program::new(
                vec![
                    "sh".into(),
                    "-c".into(),
                    "dd if=/dev/zero bs=65536 count=64 2>/dev/null && printf end".into(),
                ],
                "/",
            )
        };
        let (command, events) = commands.spawn(machine.namespace(), &program.spec()).expect("launched");
        let count = 4 << 20;
        let ran = tokio::time::timeout(Duration::from_secs(10), async {
            while command.read_output_at(count as u64).expect("retained output").bytes != b"end"[..] {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            collect(&command, events, &program).await.expect("complete output")
        })
        .await
        .expect("output arrives without a live reader");
        assert_eq!(ran.exit, octoii_siphon_lib::Exit::Code(0));
        assert_eq!(ran.output.len(), count + 3);
        assert!(ran.output[..count].iter().all(|&byte| byte == 0));
        assert_eq!(&ran.output[count..], b"end");
    }
}
