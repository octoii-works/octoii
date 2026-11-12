//! `terminal`: the model's terminals in its thread, beside the person's, which the person sees and may take over — a
//! command line that does not end by itself, a server or a watcher, run in one of its own, held to what the thread
//! may write as the shell's commands are; what a terminal shows, read; keys typed to one of its own; one closed. A
//! command line that ends of itself is told to the thread, as a sub-thread's work in the background is: how it ended
//! and what its screen showed, in the thread's inbox, which takes it in at the next step or begins a turn with it.

use std::time::Duration;

use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::Deserialize;
use siphon::machine::Base;
use siphon::tools::{Context, Effect, Refusal, Told, Tool};
use siphon_runtime::Runtime;
use tokio::sync::broadcast;

use crate::{Event, Running, Shown, Size, Terminals};

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

/// `terminal`, on the terminals.
#[derive(Debug, Clone)]
pub struct TerminalTool {
    pub terminals: Terminals,
}

impl Tool for TerminalTool {
    const NAME: &'static str = "terminal";
    const DESCRIPTION: &'static str = "Work the thread's terminals, which the person sees beside the chat.\n- open: run a command that does not exit by itself (a server, a watcher) in a new terminal in the workspace, sandboxed as shell commands are. Returns the terminal's number and its screen after a moment. When the command exits you get a message with how it ended and its last screen; do not poll for it.\n- read: return what a terminal shows, the person's terminals included.\n- write: type into a terminal you opened (`\\u0003` for Ctrl-C, `\\r` for Enter); returns the screen after.\n- close: close a terminal you opened, ending its command.\nFor a command that exits by itself, use shell. Link a terminal in your reply as :terminal{id=<number>}, or as ::terminal{id=<number>} on its own line to show its card with its last lines.";
    type Args = Action;

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
        let words = match action {
            Action::Open { command, .. } => self.open(context, command).await?,
            Action::Read { terminal } => {
                let shown = self.numbered(&context.thread, terminal).await?;
                let text = self.terminals.text(&shown.id).await.map_err(|error| Refusal(error.to_string()))?;
                format!(
                    "<system-reminder>terminal {terminal}: {}</system-reminder>\n{}",
                    what(&shown),
                    screen(&text)
                )
            }
            Action::Write { terminal, text } => {
                let shown = self.own(&context.thread, terminal).await?;
                self.terminals
                    .write(&shown.id, text.into_bytes().into())
                    .await
                    .map_err(|error| Refusal(error.to_string()))?;
                tokio::time::sleep(SETTLE).await;
                let text = self.terminals.text(&shown.id).await.unwrap_or_default();
                format!(
                    "<system-reminder>terminal {terminal}: {}</system-reminder>\n{}",
                    what(&shown),
                    screen(&text)
                )
            }
            Action::Close { terminal } => {
                let shown = self.own(&context.thread, terminal).await?;
                self.terminals.close(&shown.id);
                format!("<system-reminder>terminal {terminal} closed</system-reminder>")
            }
        };
        Ok(Told::from(vec![InputContent::text(words)]))
    }
}

impl TerminalTool {
    /// Runs `command` in a terminal of the thread's own: its screen after a moment, or, where it ended by then, how.
    /// Where it runs on, its end is told to the thread when it comes.
    async fn open(&self, context: &Context, command: String) -> Result<String, Refusal> {
        let mut events = self.terminals.events();
        let running = Running::Line {
            line: command.clone(),
            writes: context.writes_of(),
            set: context.variables(),
        };
        let id = self
            .terminals
            .open(&context.thread, context.base.workspace.clone(), SIZE, running)
            .await
            .map_err(|error| Refusal(error.to_string()))?;
        let number = self
            .terminals
            .of_thread(&context.thread)
            .await
            .into_iter()
            .find(|shown| shown.id == id)
            .map_or(0, |shown| shown.number);
        if let Ok(Some(ended)) = tokio::time::timeout(SETTLE, end_of(&mut events, &id)).await {
            return Ok(ended_words(&ended));
        }
        let text = self.terminals.text(&id).await.unwrap_or_default();
        let runtime = context.runtime.clone();
        tokio::spawn(async move {
            if let Some(ended) = end_of(&mut events, &id).await {
                let _ = tell(&runtime, &ended).await;
            }
        });
        Ok(format!(
            "<system-reminder>terminal {number} runs it; you are told when it ends</system-reminder>\n{}",
            screen(&text)
        ))
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

/// A command line that ended of itself: its terminal's number in its thread, the line, how it ended, and its screen.
#[derive(Debug, Clone)]
struct Ended {
    thread: String,
    number: u32,
    line: String,
    exit: beak::Exit,
    screen: String,
}

/// How the command line of the terminal `id` ended of itself, as `events` tell it; none where it was closed first, or
/// the terminals are gone.
async fn end_of(events: &mut broadcast::Receiver<Event>, id: &str) -> Option<Ended> {
    loop {
        match events.recv().await {
            Ok(Event::Ended {
                id: ended,
                thread,
                number,
                line,
                exit,
                screen,
            }) if ended == id => {
                return Some(Ended {
                    thread,
                    number,
                    line,
                    exit,
                    screen,
                });
            }
            Ok(Event::Closed { id: closed }) if closed == id => return None,
            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(broadcast::error::RecvError::Closed) => return None,
        }
    }
}

/// Tells the thread that a command line it ran ended, in its inbox.
async fn tell(runtime: &Runtime, ended: &Ended) -> anyhow::Result<()> {
    siphon::agent::deliver(runtime, &ended.thread, ACTOR, vec![InputContent::text(ended_words(ended))]).await
}

/// A command line's end as the model is told it: the note, then its last screen.
fn ended_words(ended: &Ended) -> String {
    let how = match ended.exit {
        beak::Exit::Code(code) => format!("exit {code}"),
        beak::Exit::Signal(signal) => format!("ended by signal {signal}"),
    };
    format!(
        "<system-reminder>terminal {}, which ran `{}`, ended: {how}</system-reminder>\n{}",
        ended.number,
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
