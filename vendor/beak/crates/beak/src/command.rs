//! A command the kernel started, recorded in its volume: what it tells a stream of its own; what is written to it taken
//! at once; its group signalled, its terminal sized and drawn, what it keeps read.

use beak_sys as sys;
use bytes::Bytes;
use tokio::sync::mpsc;

use crate::ffi::{check, copied, lent, object};
use crate::kernel::{Kernel, buffer};
use crate::namespace::Namespace;
use crate::queue::Told;
use crate::{Exit, Result, Signal};

/// The command as Beak holds it.
struct Raw(*mut sys::beak_command);

// SAFETY: a command is used from any thread at once, behind its own locks.
unsafe impl Send for Raw {}
// SAFETY: as above.
unsafe impl Sync for Raw {}

impl Drop for Raw {
    fn drop(&mut self) {
        // SAFETY: the command Beak gave, let go of once.
        unsafe { sys::beak_command_free(self.0) };
    }
}

/// A command. Let go of while it runs, it is hung up on, or its input closed, and killed past a grace.
pub struct Command(Raw);

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Command").field(&self.group()).finish()
    }
}

/// What a command runs: a program and its arguments, or a command line the machine's shell runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Program {
    Argv(Vec<String>),
    Shell(String),
}

/// How a command's streams are wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stdio {
    Pipe,
    Coprocess,
    Channel,
    Terminal { rows: u16, cols: u16, scrollback_lines: usize },
}

/// What a command keeps of what it writes, for a reader: its first bytes and its newest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kept {
    pub head_bytes: usize,
    pub tail_bytes: usize,
}

impl Default for Kept {
    fn default() -> Self {
        Self {
            head_bytes: 16 * 1024,
            tail_bytes: 16 * 1024,
        }
    }
}

/// A command to start.
#[derive(Debug, Clone)]
pub struct Spec {
    pub program: Program,
    pub cwd: String,
    pub set: Vec<(String, String)>,
    pub clear: bool,
    pub stdio: Stdio,
    pub kept: Kept,
    /// A log of all it writes, and how much of it the log keeps.
    pub log: Option<(String, u64)>,
}

impl Spec {
    /// `program` run in `cwd` through pipes, with the machine's variables.
    #[must_use]
    pub fn new(program: Program, cwd: impl Into<String>) -> Self {
        Self {
            program,
            cwd: cwd.into(),
            set: Vec::new(),
            clear: false,
            stdio: Stdio::Pipe,
            kept: Kept::default(),
            log: None,
        }
    }

    /// Calls `call` with the spec as the ABI takes it, what it points into alive for the call.
    fn lowered<T>(&self, call: impl FnOnce(&sys::beak_spec) -> T) -> T {
        let (kind, argv, line): (_, Vec<sys::beak_bytes>, &str) = match &self.program {
            Program::Argv(argv) => (
                sys::beak_program_kind::BEAK_PROGRAM_KIND_ARGV,
                argv.iter().map(|arg| lent(arg.as_bytes())).collect(),
                "",
            ),
            Program::Shell(line) => (sys::beak_program_kind::BEAK_PROGRAM_KIND_SHELL, Vec::new(), line.as_str()),
        };
        let set: Vec<sys::beak_pair> = self
            .set
            .iter()
            .map(|(name, value)| sys::beak_pair {
                name: lent(name.as_bytes()),
                value: lent(value.as_bytes()),
            })
            .collect();
        let (stdio, rows, cols, scrollback_lines) = match self.stdio {
            Stdio::Pipe => (sys::beak_stdio::BEAK_STDIO_PIPE, 0, 0, 0),
            Stdio::Coprocess => (sys::beak_stdio::BEAK_STDIO_COPROCESS, 0, 0, 0),
            Stdio::Channel => (sys::beak_stdio::BEAK_STDIO_CHANNEL, 0, 0, 0),
            Stdio::Terminal {
                rows,
                cols,
                scrollback_lines,
            } => (sys::beak_stdio::BEAK_STDIO_TERMINAL, rows, cols, scrollback_lines),
        };
        let (log_path, log_bytes_max) = self.log.as_ref().map_or(("", 0), |(path, max)| (path.as_str(), *max));
        let lowered = sys::beak_spec {
            program: kind,
            argv: argv.as_ptr(),
            argc: argv.len(),
            line: lent(line.as_bytes()),
            cwd: lent(self.cwd.as_bytes()),
            set: set.as_ptr(),
            set_count: set.len(),
            clear: self.clear,
            stdio,
            rows,
            cols,
            scrollback_lines,
            head_bytes: self.kept.head_bytes,
            tail_bytes: self.kept.tail_bytes,
            log_path: lent(log_path.as_bytes()),
            log_bytes_max,
        };
        call(&lowered)
    }
}

/// What a command tells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandEvent {
    Output(Bytes),
    Title(String),
    Exit(Exit),
}

/// What a command tells, as it tells it; its end last.
#[derive(Debug)]
pub struct CommandEvents(mpsc::UnboundedReceiver<Told>);

impl CommandEvents {
    /// What it tells next; `None` after its end.
    pub async fn next(&mut self) -> Option<CommandEvent> {
        loop {
            let event = match self.0.recv().await? {
                Told::Output(bytes) => CommandEvent::Output(bytes),
                Told::Title(title) => CommandEvent::Title(title),
                Told::Exit(exit) => CommandEvent::Exit(exit),
                _ => continue,
            };
            return Some(event);
        }
    }
}

impl Kernel {
    /// Starts the command `spec` describes at `time` in `namespace`, recorded under the directory `under`.
    ///
    /// # Errors
    /// `MissingProgram` where the machine has none of the name; `Unsupported` for what the system cannot hold it to.
    pub fn spawn(&self, time: i64, under: &[u8], namespace: &Namespace, spec: &Spec) -> Result<(Command, CommandEvents)> {
        let (tag, told) = self.beak.router.bind();
        let raw = spec.lowered(|lowered| {
            // SAFETY: the call is given what it asks, alive for it.
            object(|out| unsafe {
                sys::beak_kernel_spawn(
                    self.raw(),
                    time,
                    lent(under),
                    namespace.raw(),
                    lowered,
                    self.beak.router.raw.0,
                    tag,
                    out,
                )
            })
        })?;
        Ok((Command(Raw(raw)), CommandEvents(told)))
    }

    /// Starts the command `spec` describes at `time` for the process at `process`, in the namespace it names.
    ///
    /// # Errors
    /// `NotFound` where no process is there, or it names no namespace registered; as `spawn`.
    pub fn spawn_for(&self, time: i64, process: &[u8], spec: &Spec) -> Result<(Command, CommandEvents)> {
        let (tag, told) = self.beak.router.bind();
        let raw = spec.lowered(|lowered| {
            // SAFETY: as above.
            object(|out| unsafe { sys::beak_kernel_spawn_for(self.raw(), time, lent(process), lowered, self.beak.router.raw.0, tag, out) })
        })?;
        Ok((Command(Raw(raw)), CommandEvents(told)))
    }
}

impl Command {
    fn raw(&self) -> *const sys::beak_command {
        self.0.0
    }

    /// The id of its group.
    #[must_use]
    pub fn group(&self) -> u32 {
        // SAFETY: the command is open.
        unsafe { sys::beak_command_group(self.raw()) }
    }

    /// Takes `bytes` to write to its input, never waiting.
    ///
    /// # Errors
    /// `Closed` where its input was closed.
    pub fn write(&self, bytes: &[u8]) -> Result<()> {
        // SAFETY: as above.
        check(unsafe { sys::beak_command_write(self.raw(), lent(bytes)) })
    }

    /// Closes its input once what was taken is written.
    ///
    /// # Errors
    /// None that a command gives.
    pub fn close_input(&self) -> Result<()> {
        // SAFETY: as above.
        check(unsafe { sys::beak_command_close_input(self.raw()) })
    }

    /// Sends `signal` to its group.
    ///
    /// # Errors
    /// None that a command gives.
    pub fn signal(&self, signal: Signal) -> Result<()> {
        // SAFETY: as above.
        check(unsafe { sys::beak_command_signal(self.raw(), signal.raw()) })
    }

    /// Sizes its terminal.
    ///
    /// # Errors
    /// `Invalid` for a command with no terminal.
    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        // SAFETY: as above.
        check(unsafe { sys::beak_command_resize(self.raw(), rows, cols) })
    }

    /// What it keeps, as a reader is shown it.
    ///
    /// # Errors
    /// None that a command gives.
    pub fn output(&self) -> Result<Bytes> {
        // SAFETY: as above.
        buffer(|out| unsafe { sys::beak_command_output(self.raw(), out) })
    }

    /// What it keeps from `cursor` on: where the bytes start, and the bytes.
    ///
    /// # Errors
    /// None that a command gives.
    pub fn since(&self, cursor: u64) -> Result<(u64, Bytes)> {
        let mut since = sys::beak_since {
            from: 0,
            bytes: crate::ffi::empty(),
        };
        // SAFETY: as above; the bytes are copied, then let go of once.
        check(unsafe { sys::beak_command_since(self.raw(), cursor, &raw mut since) })?;
        let bytes = copied(&since.bytes);
        // SAFETY: as above.
        unsafe { sys::beak_buffer_free(&raw mut since.bytes) };
        Ok((since.from, bytes))
    }

    /// The sequences that draw its terminal again whole.
    ///
    /// # Errors
    /// `Invalid` for a command with no terminal.
    pub fn screen(&self) -> Result<Bytes> {
        // SAFETY: as above.
        buffer(|out| unsafe { sys::beak_command_screen(self.raw(), out) })
    }

    /// What its terminal shows, as text.
    ///
    /// # Errors
    /// `Invalid` for a command with no terminal.
    pub fn screen_text(&self) -> Result<String> {
        // SAFETY: as above.
        Ok(String::from_utf8_lossy(&buffer(|out| unsafe { sys::beak_command_screen_text(self.raw(), out) })?).into_owned())
    }

    /// The title the program on its terminal set.
    ///
    /// # Errors
    /// `Invalid` for a command with no terminal.
    pub fn title(&self) -> Result<String> {
        // SAFETY: as above.
        Ok(String::from_utf8_lossy(&buffer(|out| unsafe { sys::beak_command_title(self.raw(), out) })?).into_owned())
    }
}
