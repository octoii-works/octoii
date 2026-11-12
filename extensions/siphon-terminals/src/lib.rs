//! The terminals of the threads, where the person works on the machine beside the model, as tmux's server keeps its
//! panes: each the person's login shell in its thread's folder — or a command line the model runs in it, held to what
//! its thread may write — on a terminal Beak keeps, which answers its program's queries, keeps the title it sets and
//! draws itself again whole. Each is numbered in its thread as tmux numbers windows, the lowest number free; sized as
//! its views ask; gone as its program ends; closed with its thread. One task owns them all and does what it is asked in
//! the order asked, telling whoever listens as a thread's terminals come, take titles and go, as each writes, and as a
//! command line the model ran ends of itself, with what its screen showed.

mod extension;
pub mod protocol;
mod tool;

pub use extension::TerminalsExtension;
pub use tool::{ACTOR, Action, TerminalTool};

use std::path::{Path, PathBuf};

use beak::{Command, CommandEvent, Exec, Kernel, Spec, Stdio};
use bytes::Bytes;
use siphon::machine::paths::Base;
use siphon::machine::{Host, Machine, Writes};
use tokio::sync::{broadcast, mpsc, oneshot};

/// How many lines of what scrolls off a terminal's screen it keeps, drawn again with it.
const SCROLLBACK_LINES: usize = 3000;
/// How much of a terminal's output Beak keeps for a reader that fell behind, past which its screen is drawn again.
const KEPT_BYTES: usize = 1 << 20;
/// How many events a listener may fall behind by.
const EVENTS_MAX: usize = 1024;

/// A terminal's size, in cells: a row and a column at least.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub rows: u16,
    pub cols: u16,
}

/// A terminal as a list of a thread's shows it: its id, its number in its thread, its size, its title, and the command
/// line it runs where it runs one rather than the person's shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub id: String,
    pub number: u32,
    pub size: Size,
    pub title: String,
    pub line: Option<String>,
}

/// What a terminal runs: the person's login shell, or a command line in it, held to what it may write, with the
/// variables set over the machine's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Running {
    Shell,
    Line {
        line: String,
        writes: Writes,
        set: Vec<(String, String)>,
    },
}

/// What the terminals tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The thread's terminals changed: one came, took a title, or went.
    Changed { thread: String },
    /// The terminal wrote; it has written `written` bytes in all.
    Wrote { id: String, written: u64 },
    /// The terminal is gone: closed, or its program ended.
    Closed { id: String },
    /// A command line a terminal ran ended of itself, not closed: how, and what its screen showed as it did.
    Ended {
        id: String,
        thread: String,
        number: u32,
        line: String,
        exit: beak::Exit,
        screen: String,
    },
}

/// Why a terminal was not opened or reached.
#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("no such terminal")]
    NoSuchTerminal,
    #[error("a terminal is a row and a column at least")]
    EmptySize,
    #[error("the host knows no shell")]
    NoShell,
    #[error("the terminals are gone")]
    Gone,
    #[error(transparent)]
    Beak(#[from] beak::Error),
}

type Reply<T> = oneshot::Sender<Result<T, Error>>;

#[derive(Debug)]
enum Message {
    Open {
        thread: String,
        cwd: PathBuf,
        size: Size,
        running: Running,
        reply: Reply<String>,
    },
    Write {
        id: String,
        bytes: Bytes,
        reply: Reply<()>,
    },
    Resize {
        id: String,
        size: Size,
        reply: Reply<bool>,
    },
    Screen {
        id: String,
        reply: Reply<beak::Screen>,
    },
    Text {
        id: String,
        reply: Reply<String>,
    },
    ReadAt {
        id: String,
        offset: u64,
        reply: Reply<beak::Chunk>,
    },
    List {
        thread: String,
        reply: oneshot::Sender<Vec<Shown>>,
    },
    Close {
        id: String,
    },
    CloseThread {
        thread: String,
    },
    Told {
        id: String,
        event: CommandEvent,
    },
}

/// The terminals, reached through the task that owns them.
#[derive(Debug, Clone)]
pub struct Terminals {
    messages: mpsc::UnboundedSender<Message>,
    events: broadcast::Sender<Event>,
}

impl Terminals {
    /// The terminals of the machine `host` gives, on `kernel`, owned by a task of their own on the current runtime
    /// till every handle is let go of.
    #[must_use]
    pub fn start(kernel: Kernel, host: Host) -> Self {
        let (messages, inbox) = mpsc::unbounded_channel();
        let (events, _) = broadcast::channel(EVENTS_MAX);
        let owner = Owner {
            kernel,
            host,
            terminals: Vec::new(),
            events: events.clone(),
            messages: messages.downgrade(),
        };
        tokio::spawn(owner.serve(inbox));
        Self { messages, events }
    }

    /// What the terminals tell, from now on.
    #[must_use]
    pub fn events(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    async fn ask<T>(&self, message: impl FnOnce(Reply<T>) -> Message) -> Result<T, Error> {
        let (reply, answer) = oneshot::channel();
        self.messages.send(message(reply)).map_err(|_| Error::Gone)?;
        answer.await.map_err(|_| Error::Gone)?
    }

    /// Opens a terminal for the thread `thread`, `size` big, in `cwd`, running `running`. Its id.
    ///
    /// # Errors
    /// `EmptySize`, `NoShell`; Beak's failure to start it.
    pub async fn open(&self, thread: &str, cwd: PathBuf, size: Size, running: Running) -> Result<String, Error> {
        let thread = thread.to_owned();
        self.ask(|reply| Message::Open {
            thread,
            cwd,
            size,
            running,
            reply,
        })
        .await
    }

    /// Keys the person typed, and what they pasted, to the terminal's program.
    ///
    /// # Errors
    /// `NoSuchTerminal`; Beak's failure.
    pub async fn write(&self, id: &str, bytes: Bytes) -> Result<(), Error> {
        let id = id.to_owned();
        self.ask(|reply| Message::Write { id, bytes, reply }).await
    }

    /// Sizes the terminal to `size`: whether it changed.
    ///
    /// # Errors
    /// `NoSuchTerminal`, `EmptySize`; Beak's failure.
    pub async fn resize(&self, id: &str, size: Size) -> Result<bool, Error> {
        let id = id.to_owned();
        self.ask(|reply| Message::Resize { id, size, reply }).await
    }

    /// The terminal's screen drawn again whole, with the offset in its output it stands at.
    ///
    /// # Errors
    /// `NoSuchTerminal`; Beak's failure.
    pub async fn screen(&self, id: &str) -> Result<beak::Screen, Error> {
        let id = id.to_owned();
        self.ask(|reply| Message::Screen { id, reply }).await
    }

    /// What the terminal shows, as text.
    ///
    /// # Errors
    /// `NoSuchTerminal`; Beak's failure.
    pub async fn text(&self, id: &str) -> Result<String, Error> {
        let id = id.to_owned();
        self.ask(|reply| Message::Text { id, reply }).await
    }

    /// What the terminal wrote from `offset` on, as Beak keeps it.
    ///
    /// # Errors
    /// `NoSuchTerminal`; Beak's failure.
    pub async fn read_at(&self, id: &str, offset: u64) -> Result<beak::Chunk, Error> {
        let id = id.to_owned();
        self.ask(|reply| Message::ReadAt { id, offset, reply }).await
    }

    /// The terminals of the thread `thread`, in the order they were opened.
    pub async fn of_thread(&self, thread: &str) -> Vec<Shown> {
        let (reply, answer) = oneshot::channel();
        let thread = thread.to_owned();
        if self.messages.send(Message::List { thread, reply }).is_err() {
            return Vec::new();
        }
        answer.await.unwrap_or_default()
    }

    /// Closes the terminal `id`, Beak hanging up on its shell.
    pub fn close(&self, id: &str) {
        let _ = self.messages.send(Message::Close { id: id.to_owned() });
    }

    /// Closes every terminal of the thread `thread`, as it goes.
    pub fn close_thread(&self, thread: &str) {
        let _ = self.messages.send(Message::CloseThread { thread: thread.to_owned() });
    }
}

/// A terminal as its owner keeps it.
struct Terminal {
    id: String,
    thread: String,
    number: u32,
    size: Size,
    title: String,
    line: Option<String>,
    written: u64,
    command: Command,
    /// The machine its shell was started in, kept as long as it runs.
    _machine: Machine,
}

/// The task that owns the terminals.
struct Owner {
    kernel: Kernel,
    host: Host,
    terminals: Vec<Terminal>,
    events: broadcast::Sender<Event>,
    /// Where each terminal's own task tells what it did; weak, so the owner ends as every handle goes.
    messages: mpsc::WeakUnboundedSender<Message>,
}

impl Owner {
    async fn serve(mut self, mut inbox: mpsc::UnboundedReceiver<Message>) {
        while let Some(message) = inbox.recv().await {
            self.handle(message);
        }
    }

    fn handle(&mut self, message: Message) {
        match message {
            Message::Open {
                thread,
                cwd,
                size,
                running,
                reply,
            } => {
                let _ = reply.send(self.open(&thread, &cwd, size, running));
            }
            Message::Write { id, bytes, reply } => {
                let _ = reply.send(self.find(&id).and_then(|terminal| Ok(terminal.command.write(&bytes)?)));
            }
            Message::Resize { id, size, reply } => {
                let _ = reply.send(self.resize(&id, size));
            }
            Message::Screen { id, reply } => {
                let _ = reply.send(self.find(&id).and_then(|terminal| Ok(terminal.command.screen()?)));
            }
            Message::Text { id, reply } => {
                let _ = reply.send(self.find(&id).and_then(|terminal| Ok(terminal.command.screen_text()?)));
            }
            Message::ReadAt { id, offset, reply } => {
                let _ = reply.send(self.find(&id).and_then(|terminal| Ok(terminal.command.read_output_at(offset)?)));
            }
            Message::List { thread, reply } => {
                let shown = self
                    .terminals
                    .iter()
                    .filter(|terminal| terminal.thread == thread)
                    .map(shown)
                    .collect();
                let _ = reply.send(shown);
            }
            Message::Close { id } => self.close(&id),
            Message::CloseThread { thread } => {
                let ids: Vec<String> = self
                    .terminals
                    .iter()
                    .filter(|terminal| terminal.thread == thread)
                    .map(|terminal| terminal.id.clone())
                    .collect();
                for id in ids {
                    self.close(&id);
                }
            }
            Message::Told { id, event } => self.told(&id, event),
        }
    }

    fn find(&self, id: &str) -> Result<&Terminal, Error> {
        self.terminals
            .iter()
            .find(|terminal| terminal.id == id)
            .ok_or(Error::NoSuchTerminal)
    }

    fn changed(&self, thread: &str) {
        let _ = self.events.send(Event::Changed { thread: thread.to_owned() });
    }

    fn open(&mut self, thread: &str, cwd: &Path, size: Size, running: Running) -> Result<String, Error> {
        if size.rows == 0 || size.cols == 0 {
            return Err(Error::EmptySize);
        }
        let shell = self.host.shell.clone().ok_or(Error::NoShell)?.to_string_lossy().into_owned();
        let base = Base {
            workspace: cwd.to_owned(),
            home: self.host.home.clone(),
        };
        let (argv, writes, set, line) = match running {
            Running::Shell => (vec![shell, "-l".into()], Writes::Anywhere, Vec::new(), None),
            Running::Line { line, writes, set } => (vec![shell, "-l".into(), "-c".into(), line.clone()], writes, set, Some(line)),
        };
        let machine = Machine::open(&self.kernel, &self.host, base, &writes, None)?;
        let spec = Spec {
            set: [("TERM".into(), "xterm-256color".into()), ("COLORTERM".into(), "truecolor".into())]
                .into_iter()
                .chain(set)
                .collect(),
            stdio: Stdio::Terminal {
                rows: size.rows,
                cols: size.cols,
                scrollback_lines: SCROLLBACK_LINES,
            },
            tail_bytes: KEPT_BYTES,
            ..Spec::new(Exec::Argv(argv), cwd.to_string_lossy())
        };
        let now = now_ms();
        let (command, mut events) = self.kernel.spawn(now, b"", machine.namespace(), &spec)?;
        let id = self.kernel.new_id(now)?;
        let taken: Vec<u32> = self
            .terminals
            .iter()
            .filter(|terminal| terminal.thread == thread)
            .map(|terminal| terminal.number)
            .collect();
        // Of one more number than are taken, one is free.
        let number = (1..).take(taken.len() + 1).find(|number| !taken.contains(number)).unwrap_or(1);
        let messages = self.messages.clone();
        let told = id.clone();
        tokio::spawn(async move {
            while let Some(event) = events.next().await {
                let Some(messages) = messages.upgrade() else { return };
                let _ = messages.send(Message::Told { id: told.clone(), event });
            }
        });
        self.terminals.push(Terminal {
            id: id.clone(),
            thread: thread.to_owned(),
            number,
            size,
            title: String::new(),
            line,
            written: 0,
            command,
            _machine: machine,
        });
        self.changed(thread);
        Ok(id)
    }

    fn resize(&mut self, id: &str, size: Size) -> Result<bool, Error> {
        if size.rows == 0 || size.cols == 0 {
            return Err(Error::EmptySize);
        }
        let terminal = self
            .terminals
            .iter_mut()
            .find(|terminal| terminal.id == id)
            .ok_or(Error::NoSuchTerminal)?;
        if terminal.size == size {
            return Ok(false);
        }
        terminal.command.resize(size.rows, size.cols)?;
        terminal.size = size;
        Ok(true)
    }

    fn close(&mut self, id: &str) {
        let Some(at) = self.terminals.iter().position(|terminal| terminal.id == id) else {
            return;
        };
        // Let go of, Beak hangs up on a shell still running, and reaps it.
        let terminal = self.terminals.remove(at);
        let _ = self.events.send(Event::Closed { id: terminal.id.clone() });
        self.changed(&terminal.thread);
    }

    fn told(&mut self, id: &str, event: CommandEvent) {
        let Some(terminal) = self.terminals.iter_mut().find(|terminal| terminal.id == id) else {
            return;
        };
        match event {
            CommandEvent::Output(bytes) => {
                terminal.written += bytes.len() as u64;
                let _ = self.events.send(Event::Wrote {
                    id: id.to_owned(),
                    written: terminal.written,
                });
            }
            CommandEvent::Title(title) => {
                terminal.title = title;
                let thread = terminal.thread.clone();
                self.changed(&thread);
            }
            CommandEvent::Exit(exit) => {
                if let Some(line) = terminal.line.clone() {
                    let screen = terminal.command.screen_text().unwrap_or_default();
                    let _ = self.events.send(Event::Ended {
                        id: id.to_owned(),
                        thread: terminal.thread.clone(),
                        number: terminal.number,
                        line,
                        exit,
                        screen,
                    });
                }
                self.close(id);
            }
        }
    }
}

fn shown(terminal: &Terminal) -> Shown {
    Shown {
        id: terminal.id.clone(),
        number: terminal.number,
        size: terminal.size,
        title: terminal.title.clone(),
        line: terminal.line.clone(),
    }
}

fn now_ms() -> i64 {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
}
