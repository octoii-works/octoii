//! `terminal`: the model's terminals in its thread, beside the person's, which the person sees and may take over — a
//! command line that does not end by itself, a server or a watcher, run in one of its own, held to what the thread
//! may write as the shell's commands are; what a terminal shows, read; keys typed to one of its own; one closed. A
//! command line that ends of itself is told to the thread, as a sub-thread's work in the background is: how it ended
//! and what its screen showed, in the thread's inbox, which takes it in at the next step or begins a turn with it.

use std::time::Duration;

use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use siphon::machine::Base;
use siphon::tools::{Context, Effect, Refusal, Told, Tool};
use tokio::sync::broadcast;

use crate::{Error, Event, Running, Shown, Size, TerminalReference, Terminals};

/// How big a terminal the model opens is.
const SIZE: Size = Size { rows: 30, cols: 120 };
/// How long a command line opened or keys typed are given to show something before the screen is read.
const SETTLE: Duration = Duration::from_millis(1500);
/// Whose the input that tells a command line ended is.
pub const ACTOR: &str = "terminal";

/// What the model does with the terminals of its thread.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    /// Run a command in a new terminal.
    Open {
        /// The command to run.
        command: String,
        /// Run outside the sandbox, once the person approves.
        #[serde(default)]
        escalate: bool,
        /// Why it must write outside the sandbox, in one sentence for the person.
        justification: Option<String>,
    },
    /// Return what a terminal shows, the person's terminals included.
    Read {
        /// The terminal's number.
        terminal: u32,
    },
    /// Type into a terminal you opened.
    Write {
        /// The terminal's number.
        terminal: u32,
        /// Keys as text: `\u0003` for Ctrl-C, `\r` for Enter.
        text: String,
    },
    /// Close a terminal you opened, ending its command.
    Close {
        /// The terminal's number.
        terminal: u32,
    },
}

/// What a terminal call observed or requested. Its state is a snapshot when observed, not a promise that the program
/// will keep running. A screen is the terminal's rendered text, not a complete command log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TerminalResult {
    Running {
        terminal: TerminalReference,
        /// The terminal's screen as text, without model-facing escaping.
        screen: String,
    },
    Ended {
        terminal: TerminalReference,
        ended: TerminalEnd,
        /// The terminal's last screen as text, not the command's complete output.
        screen: String,
    },
    Closed {
        terminal: TerminalReference,
    },
    /// The close request was submitted to the terminal's owner.
    CloseRequested {
        terminal: TerminalReference,
    },
    /// The screen or ending could not be read; this does not say whether the program still runs.
    ScreenUnavailable {
        terminal: TerminalReference,
        error: String,
    },
}

/// How a terminal's command ended of itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalEnd {
    Exited { code: i32 },
    Signalled { signal: i32 },
}

/// `terminal`, on the terminals.
#[derive(Debug, Clone)]
pub struct TerminalTool {
    pub terminals: Terminals,
}

impl Tool for TerminalTool {
    const NAME: &'static str = "terminal";
    const DESCRIPTION: &'static str = "Work the thread's terminals, which the person sees beside the chat.\n- open: run a command that does not exit by itself (a server, a watcher) in a new terminal in the workspace, sandboxed as shell commands are. Returns the terminal's number and its screen after a moment. When the command exits you get a message with how it ended and its last screen; do not poll for it.\n- read: return what a terminal shows, the person's terminals included.\n- write: type into a terminal you opened (`\\u0003` for Ctrl-C, `\\r` for Enter); returns the screen after.\n- close: close a terminal you opened, ending its command.\nFor a command that exits by itself, use shell. Link a terminal in your reply as :terminal{id=<number>}, or as ::terminal{id=<number>} on its own line to show its card with its last lines.";
    type Args = Action;

    fn output_schema() -> Option<serde_json::Value> {
        Some(schemars::schema_for!(TerminalResult).to_value())
    }

    fn effect(&self, action: &Action, _: &Base) -> Effect {
        match action {
            Action::Open {
                command,
                escalate,
                justification,
            } => Effect::Runs {
                line: command.clone(),
                outside_sandbox: *escalate,
                justification: justification.clone(),
            },
            Action::Write { text, .. } => Effect::Runs {
                line: text.clone(),
                outside_sandbox: false,
                justification: None,
            },
            Action::Read { .. } | Action::Close { .. } => Effect::Reads,
        }
    }

    async fn exec(&self, context: &Context, action: Action) -> Result<Told, Refusal> {
        let (words, result) = match action {
            Action::Open { command, .. } => self.open(context, command).await?,
            Action::Read { terminal } => {
                let shown = self.numbered(&context.thread, terminal).await?;
                let text = self.terminals.text(&shown.id).await.map_err(|error| Refusal(error.to_string()))?;
                let words = format!(
                    "<system-reminder>terminal {terminal}: {}</system-reminder>\n{}",
                    what(&shown),
                    screen(&text)
                );
                (
                    words,
                    TerminalResult::Running {
                        terminal: reference(shown),
                        screen: text,
                    },
                )
            }
            Action::Write { terminal, text } => {
                let mut events = self.terminals.events();
                let shown = self.own(&context.thread, terminal).await?;
                self.terminals
                    .write(&shown.id, text.into_bytes().into())
                    .await
                    .map_err(|error| Refusal(error.to_string()))?;
                tokio::time::sleep(SETTLE).await;
                let text = self.terminals.text(&shown.id).await;
                let words = format!(
                    "<system-reminder>terminal {terminal}: {}</system-reminder>\n{}",
                    what(&shown),
                    screen(text.as_deref().unwrap_or_default())
                );
                let result = observed(reference(shown), text, &mut events);
                (words, result)
            }
            Action::Close { terminal } => {
                let shown = self.own(&context.thread, terminal).await?;
                self.terminals.close(&shown.id);
                (
                    format!("<system-reminder>terminal {terminal} closed</system-reminder>"),
                    TerminalResult::CloseRequested {
                        terminal: reference(shown),
                    },
                )
            }
        };
        Ok(Told {
            content: vec![InputContent::text(words)],
            structured_content: Some(serde_json::json!(result)),
            ..Told::default()
        })
    }
}

impl TerminalTool {
    /// Runs `command` in a terminal of the thread's own: its screen after a moment, or, where it ended by then, how.
    /// Where it runs on, its end is told to the thread when it comes.
    async fn open(&self, context: &Context, command: String) -> Result<(String, TerminalResult), Refusal> {
        let mut events = self.terminals.events();
        let mut states = events.resubscribe();
        let running = Running::Line {
            line: command.clone(),
            writes: context.writes_of(),
            set: context.variables(),
        };
        let terminal = self
            .terminals
            .open(&context.thread, context.base.workspace.clone(), SIZE, running)
            .await
            .map_err(|error| Refusal(error.to_string()))?;
        if let Ok(Some(Completion::Ended(ended))) = tokio::time::timeout(SETTLE, end_of(&mut events, &terminal.id)).await {
            return Ok((ended_words(&ended), ended.into_result()));
        }
        let text = self.terminals.text(&terminal.id).await;
        let words = format!(
            "<system-reminder>terminal {} runs it; you are told when it ends</system-reminder>\n{}",
            terminal.number,
            screen(text.as_deref().unwrap_or_default())
        );
        let id = terminal.id.clone();
        let result = observed(terminal, text, &mut states);
        let volume = context.volume();
        if !matches!(&result, TerminalResult::Closed { .. }) {
            tokio::spawn(async move {
                if let Some(Completion::Ended(ended)) = end_of(&mut events, &id).await {
                    let _ = tell(&volume, &ended).await;
                }
            });
        }
        Ok((words, result))
    }

    /// The terminal numbered `number` in `thread`.
    async fn numbered(&self, thread: &str, number: u32) -> Result<Shown, Refusal> {
        self.terminals
            .of_thread(thread)
            .await
            .into_iter()
            .find(|shown| shown.number == number)
            .ok_or_else(|| Refusal(format!("no terminal {number} in this thread")))
    }

    /// The terminal numbered `number` in `thread`, which the model opened: the person's are theirs to type to.
    async fn own(&self, thread: &str, number: u32) -> Result<Shown, Refusal> {
        let shown = self.numbered(thread, number).await?;
        if shown.line.is_none() {
            return Err(Refusal(format!(
                "terminal {number} is the person's: read it, but type only to one you opened"
            )));
        }
        Ok(shown)
    }
}

fn reference(shown: Shown) -> TerminalReference {
    TerminalReference {
        id: shown.id,
        number: shown.number,
    }
}

/// The terminal at the instant its screen was read. A terminal gone by then is described by its lifecycle event,
/// which carries its last screen where its command ended of itself.
fn observed(terminal: TerminalReference, screen: Result<String, Error>, events: &mut broadcast::Receiver<Event>) -> TerminalResult {
    match screen {
        Ok(screen) => TerminalResult::Running { terminal, screen },
        Err(Error::NoSuchTerminal) => loop {
            match events.try_recv() {
                Ok(event) => match Completion::of(event, &terminal.id) {
                    Some(Completion::Ended(ended)) => return ended.into_result(),
                    Some(Completion::Closed) => return TerminalResult::Closed { terminal },
                    None => {}
                },
                Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                Err(error) => {
                    return TerminalResult::ScreenUnavailable {
                        terminal,
                        error: format!("the terminal is gone; its end could not be read: {error}"),
                    };
                }
            }
        },
        Err(error) => TerminalResult::ScreenUnavailable {
            terminal,
            error: error.to_string(),
        },
    }
}

/// A command line that ended of itself: its terminal's number in its thread, the line, how it ended, and its screen.
#[derive(Debug, Clone)]
struct Ended {
    thread: String,
    terminal: TerminalReference,
    line: String,
    exit: siphon::machine::Exit,
    screen: String,
}

impl Ended {
    fn into_result(self) -> TerminalResult {
        let ended = match self.exit {
            siphon::machine::Exit::Code(code) => TerminalEnd::Exited { code },
            siphon::machine::Exit::Signal(signal) => TerminalEnd::Signalled { signal },
        };
        TerminalResult::Ended {
            terminal: self.terminal,
            ended,
            screen: self.screen,
        }
    }
}

enum Completion {
    Ended(Ended),
    Closed,
}

impl Completion {
    fn of(event: Event, id: &str) -> Option<Self> {
        match event {
            Event::Ended {
                id: ended,
                thread,
                number,
                line,
                exit,
                screen,
            } if ended == id => Some(Self::Ended(Ended {
                thread,
                terminal: TerminalReference { id: ended, number },
                line,
                exit,
                screen,
            })),
            Event::Closed { id: closed } if closed == id => Some(Self::Closed),
            _ => None,
        }
    }
}

/// How the terminal `id` ended, as `events` tell it; none where the terminals are gone.
async fn end_of(events: &mut broadcast::Receiver<Event>, id: &str) -> Option<Completion> {
    loop {
        match events.recv().await {
            Ok(event) => {
                if let Some(ended) = Completion::of(event, id) {
                    return Some(ended);
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => return None,
        }
    }
}

/// Tells the thread that a command line it ran ended, in its inbox.
async fn tell(volume: &siphon::volume::Volume, ended: &Ended) -> anyhow::Result<()> {
    siphon::agent::deliver(volume, &ended.thread, ACTOR, vec![InputContent::text(ended_words(ended))]).await
}

/// A command line's end as the model is told it: the note, then its last screen.
fn ended_words(ended: &Ended) -> String {
    let how = match ended.exit {
        siphon::machine::Exit::Code(code) => format!("exit {code}"),
        siphon::machine::Exit::Signal(signal) => format!("ended by signal {signal}"),
    };
    format!(
        "<system-reminder>terminal {}, which ran `{}`, ended: {how}</system-reminder>\n{}",
        ended.terminal.number,
        ended.line,
        screen(&ended.screen)
    )
}

/// What a terminal runs, as the model is told it: its command line, or its title for the person's shell.
fn what(shown: &Shown) -> String {
    match &shown.line {
        Some(line) => format!("`{line}`"),
        None if shown.title.is_empty() => "the person's shell".into(),
        None => format!("the person's shell, {}", shown.title),
    }
}

/// A screen's text without the blank lines at its end.
fn screen(text: &str) -> std::borrow::Cow<'_, str> {
    siphon::reminder::escaped(text.trim_end())
}
