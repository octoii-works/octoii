//! A program run beside the host as long as it is needed, talked to over its standard streams as a coprocess is: what
//! is written to its input taken by Beak at once, what it writes to its output read as it tells it, ending as it does,
//! and what it writes to its errors kept, its head and tail, to be read back where it could not be talked to. Once its
//! streams are all let go of, its input is closed, and it is killed past a grace.

use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use beak::{Command, CommandEvent, CommandEvents, Exec, Spec, Stdio};
use bytes::Bytes;
use futures::Stream;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::io::StreamReader;

use crate::machine::{Machine, RunError};

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

/// What a coprocess is given to read, written as it is given; shut down, its input closed once what was taken is
/// written.
#[derive(Debug)]
pub struct Stdin(Arc<Command>);

/// What a coprocess writes to its output, read as it tells it, ending as it does.
pub struct Stdout {
    told: StreamReader<Pin<Box<dyn Stream<Item = io::Result<Bytes>> + Send>>, Bytes>,
    command: Arc<Command>,
}

/// What a coprocess writes to its errors, kept.
#[derive(Debug)]
pub struct Stderr(Arc<Command>);

impl Machine {
    /// Starts `argv` in `cwd` as a coprocess at `time` with `variables`, held to what the namespace reaches, recorded
    /// under `under`.
    ///
    /// # Errors
    /// `MissingProgram`; Beak's failure to start it.
    pub fn coprocess(&self, time: i64, under: &[u8], argv: Vec<String>, cwd: &Path, variables: Variables) -> Result<Coprocess, RunError> {
        let (set, clear) = match variables {
            Variables::Over(set) => (set, false),
            Variables::Only(set) => (set, true),
        };
        let spec = Spec {
            set,
            clear,
            stdio: Stdio::Coprocess,
            ..Spec::new(Exec::Argv(argv), cwd.to_string_lossy())
        };
        let (command, events) = self.spawn(time, under, &spec)?;
        let command = Arc::new(command);
        Ok(Coprocess {
            stdin: Stdin(Arc::clone(&command)),
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
    Box::pin(futures::stream::unfold(events, |mut events| async move {
        loop {
            match events.next().await? {
                CommandEvent::Output(bytes) => return Some((Ok(bytes), events)),
                CommandEvent::Title(_) => {}
                CommandEvent::Exit(_) => return None,
            }
        }
    }))
}

impl std::fmt::Debug for Stdout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Stdout").field(&self.command).finish()
    }
}

impl AsyncWrite for Stdin {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, bytes: &[u8]) -> Poll<io::Result<usize>> {
        Poll::Ready(self.0.write(bytes).map(|()| bytes.len()).map_err(io::Error::other))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.0.close_input().map_err(io::Error::other))
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
    pub fn kept(&self) -> beak::Result<Bytes> {
        self.0.output()
    }
}
