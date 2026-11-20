//! A program run beside the host as long as it is needed, talked to over its standard streams as a coprocess is: what
//! is written to its input taken by Beak at once, what it writes to its output read as it tells it, ending as it does,
//! and what it writes to its errors kept, its head and tail, to be read back where it could not be talked to. Once its
//! streams are all let go of, its input is closed, and it is killed past a grace.

use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};
use std::time::Duration;

use bytes::Bytes;
use futures::Stream;
use octoii_siphon_lib::{Command, CommandEvent, CommandEvents, Exec, Spec, Status, Stdio};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::time::Sleep;
use tokio_util::io::StreamReader;

use crate::machine::commands::Commands;
use crate::machine::namespace::{Machine, RunError};

/// The input slice limit imposed by Siphon's command queue.
const INPUT_MAX: usize = 64 << 10;
/// The queue offers no writable notification, so retry after a short delay when it fills.
const INPUT_RETRY: Duration = Duration::from_millis(1);

/// The variables a coprocess runs with: these over the machine's own, as a child inherits its parent's; or these
/// alone, as `execve` is given its whole environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Variables {
    Over(Vec<(String, String)>),
    Only(Vec<(String, String)>),
}

/// A program running beside the host: its standard input, output and errors, as a child's are.
#[derive(Debug)]
pub struct Coprocess {
    pub stdin: Stdin,
    pub stdout: Stdout,
    pub stderr: Stderr,
}

/// A command's input, queued in order. Shutdown closes the input once the queued bytes are written.
#[derive(Debug)]
pub struct Stdin {
    command: Arc<Command>,
    retry: Option<Pin<Box<Sleep>>>,
}

/// What a coprocess writes to its output, read as it tells it, ending as it does.
pub struct Stdout {
    told: StreamReader<Pin<Box<dyn Stream<Item = io::Result<Bytes>> + Send>>, Bytes>,
    command: Arc<Command>,
}

/// What a coprocess writes to its errors, kept.
#[derive(Debug)]
pub struct Stderr(Arc<Command>);

impl Commands {
    /// Starts a host-owned coprocess with `variables` in `cwd`, held to what `machine` reaches.
    ///
    /// # Errors
    /// `MissingProgram`; Beak's failure to start it.
    pub fn coprocess(&self, machine: &Machine, argv: Vec<String>, cwd: &Path, variables: Variables) -> Result<Coprocess, RunError> {
        let (set, clear) = match variables {
            Variables::Over(set) => (set, false),
            Variables::Only(set) => (set, true),
        };
        let spec = Spec {
            set: Some(set),
            clear: Some(clear),
            stdio: Some(Stdio::Coprocess),
            ..Spec::new(Exec::Argv(argv), cwd.to_string_lossy())
        };
        let (command, events) = machine.spawn_with(self, &spec)?;
        let command = Arc::new(command);
        Ok(Coprocess {
            stdin: Stdin::new(Arc::clone(&command)),
            stdout: Stdout {
                told: StreamReader::new(told(events)),
                command: Arc::clone(&command),
            },
            stderr: Stderr(command),
        })
    }
}

/// What a command writes to its output, as it tells it, ending as it does.
fn told(events: CommandEvents) -> Pin<Box<dyn Stream<Item = io::Result<Bytes>> + Send>> {
    Box::pin(futures::stream::unfold(Some((events, 0)), |state| async move {
        let (mut events, offset) = state?;
        loop {
            let error = match events.next().await {
                Ok(Some(CommandEvent::Output(chunk))) if chunk.offset == offset => {
                    let next = offset + chunk.bytes.len() as u64;
                    return Some((Ok(chunk.bytes), Some((events, next))));
                }
                Ok(Some(CommandEvent::Output(chunk))) => RunError::OutputLost {
                    from: offset,
                    through: chunk.offset,
                },
                Ok(Some(CommandEvent::Gap { from, through })) => RunError::OutputLost { from, through },
                Ok(Some(CommandEvent::Title(_))) => continue,
                Ok(Some(CommandEvent::Exit(_)) | None) => return None,
                Err(error) => error.into(),
            };
            return Some((Err(io::Error::other(error)), None));
        }
    }))
}

impl std::fmt::Debug for Stdout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Stdout").field(&self.command).finish()
    }
}

impl Stdin {
    /// Wraps a command's input for asynchronous writes with queue backpressure.
    #[must_use]
    pub fn new(command: Arc<Command>) -> Self {
        Self { command, retry: None }
    }
}

impl AsyncWrite for Stdin {
    fn poll_write(mut self: Pin<&mut Self>, context: &mut Context<'_>, bytes: &[u8]) -> Poll<io::Result<usize>> {
        let bytes = &bytes[..bytes.len().min(INPUT_MAX)];
        if bytes.is_empty() {
            return Poll::Ready(Ok(0));
        }
        loop {
            if let Some(retry) = &mut self.retry {
                ready!(retry.as_mut().poll(context));
                self.retry = None;
            }
            match self.command.write(bytes) {
                Ok(()) => return Poll::Ready(Ok(bytes.len())),
                Err(error) if error.status() == Status::WouldBlock => self.retry = Some(Box::pin(tokio::time::sleep(INPUT_RETRY))),
                Err(error) => return Poll::Ready(Err(io::Error::other(error))),
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.command.close_input().map_err(io::Error::other))
    }
}

impl AsyncRead for Stdout {
    fn poll_read(mut self: Pin<&mut Self>, context: &mut Context<'_>, buffer: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.told).poll_read(context, buffer)
    }
}

impl Stderr {
    /// What it wrote to its errors, as Beak keeps it: its first bytes and its newest.
    ///
    /// # Errors
    /// None that a command gives.
    pub fn kept(&self) -> octoii_siphon_lib::Result<Bytes> {
        self.0.output()
    }
}
