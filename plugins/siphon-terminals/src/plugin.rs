//! The terminals as a plugin of Siphon's: owned by a task started on the machine's kernel, and their methods and
//! topics — a terminal opened in its thread's workspace, typed to, sized and closed; a thread's terminals as they
//! change, and a terminal as it writes, each piece once.

use serde_json::{Value, json};
use siphon::{Builder, Caller, Full, Lagged, Plugin, Subscription};
use siphon_protocol::{Error, Nothing, Scope};
use tokio::sync::broadcast::error::RecvError;

use crate::protocol::{self, Cells, Method, TerminalListed, TerminalsEvent, TerminalsTopic};
use crate::{Event, Running, Size, TerminalTool, Terminals};

/// The terminals, as a host adds them.
#[derive(Debug, Clone, Copy, Default)]
pub struct TerminalsPlugin;

impl Plugin for TerminalsPlugin {
    fn name(&self) -> &'static str {
        "terminals"
    }

    fn build(self: Box<Self>, builder: &mut Builder) {
        let terminals = Terminals::start(builder.kernel.clone(), builder.machine.clone());
        let closing = terminals.clone();
        builder.on_thread_deleted(move |thread| closing.close_thread(thread));
        let tool = TerminalTool {
            terminals: terminals.clone(),
        };
        builder.tools(|tools| tools.with(tool));
        let doing = terminals.clone();
        builder.methods(protocol::set(), Method::read, move |context: Caller, method| {
            let terminals = doing.clone();
            async move { answered(&terminals, &context, method).await }
        });
        builder.topics(Scope::TerminalsOperate, move |subscription: Subscription, topic: TerminalsTopic| {
            let terminals = terminals.clone();
            async move { told(&terminals, &subscription, topic).await }
        });
    }
}

fn size(cells: Cells) -> Size {
    Size {
        rows: cells.rows,
        cols: cells.cols,
    }
}

/// Does `method` on `terminals`, for the client of `context`.
async fn answered(terminals: &Terminals, context: &Caller, method: Method) -> Result<Value, Error> {
    match method {
        Method::Open(open) => {
            let listed = context
                .siphon
                .threads
                .listed(&open.thread)
                .map_err(|error| refused("no_such_thread", &error))?;
            let cwd = context.siphon.machine.workspace_of(listed.config.workspace.as_deref());
            let terminal = terminals
                .open(&open.thread, cwd, size(open.size), Running::Shell)
                .await
                .map_err(|error| failed(&error))?;
            return Ok(json!(protocol::TerminalOpened { terminal }));
        }
        Method::Write(write) => terminals
            .write(&write.terminal, write.data.into_bytes().into())
            .await
            .map_err(|error| failed(&error))?,
        Method::Resize(resize) => {
            terminals
                .resize(&resize.terminal, size(resize.size))
                .await
                .map_err(|error| failed(&error))?;
        }
        Method::Close(of) => terminals.close(&of.terminal),
        Method::Text(of) => {
            let text = terminals.text(&of.terminal).await.map_err(|error| failed(&error))?;
            return Ok(json!(protocol::TerminalText {
                text: text.trim_end().to_owned()
            }));
        }
    }
    Ok(json!(Nothing {}))
}

fn refused(code: &str, error: &impl std::fmt::Display) -> Error {
    Error {
        code: code.to_owned(),
        message: error.to_string(),
    }
}

fn failed(error: &crate::Error) -> Error {
    let code = match &error {
        crate::Error::NoSuchTerminal => "no_such_terminal",
        _ => "failed",
    };
    refused(code, &error)
}

/// Tells `topic` to `subscription`'s client till it is let go.
async fn told(terminals: &Terminals, subscription: &Subscription, topic: TerminalsTopic) -> Result<(), Lagged> {
    match topic {
        TerminalsTopic::Terminals { thread } => listed_told(terminals, subscription, &thread)
            .await
            .map_err(|Full| Lagged(json!({ "type": "terminals", "thread": thread }))),
        TerminalsTopic::Terminal { terminal, from } => {
            let mut at = from;
            let told = output_told(terminals, subscription, &terminal, &mut at).await;
            told.map_err(|Full| Lagged(json!({ "type": "terminal", "terminal": terminal, "from": at })))
        }
    }
}

/// The terminals of `thread`, each time they change.
async fn listed_told(terminals: &Terminals, subscription: &Subscription, thread: &str) -> Result<(), Full> {
    let sink = &subscription.sink;
    let mut events = terminals.events();
    loop {
        let shown = terminals.of_thread(thread).await;
        let listed = shown
            .into_iter()
            .map(|shown| TerminalListed {
                id: shown.id,
                number: shown.number,
                size: Cells {
                    rows: shown.size.rows,
                    cols: shown.size.cols,
                },
                title: shown.title,
                line: shown.line,
            })
            .collect();
        sink.offer(&TerminalsEvent::Terminals {
            subscription: sink.id().to_owned(),
            terminals: listed,
        })?;
        loop {
            let event = tokio::select! {
                () = subscription.stop.cancelled() => return Ok(()),
                event = events.recv() => event,
            };
            match event {
                Err(RecvError::Closed) => return Ok(()),
                Ok(Event::Changed { thread: of }) if of == thread => break,
                Err(RecvError::Lagged(_)) => break,
                Ok(_) => {}
            }
        }
    }
}

/// What the terminal `terminal` writes from `at` — its screen drawn first, where `at` is none or what lay past it is
/// no longer kept — `at` moved past each piece once it is sent. A terminal gone ends it.
async fn output_told(terminals: &Terminals, subscription: &Subscription, terminal: &str, at: &mut Option<u64>) -> Result<(), Full> {
    let sink = &subscription.sink;
    let mut events = terminals.events();
    loop {
        let Some(from) = *at else {
            let Ok(screen) = terminals.screen(terminal).await else {
                return Ok(());
            };
            let drawn = TerminalsEvent::Screen {
                subscription: sink.id().to_owned(),
                through: screen.through,
            };
            sink.offer_with(&drawn, &screen.sequences)?;
            *at = Some(screen.through);
            continue;
        };
        let Ok(chunk) = terminals.read_at(terminal, from).await else {
            return Ok(());
        };
        if chunk.offset > from {
            // What lay between is kept no longer: the screen is drawn again.
            *at = None;
            continue;
        }
        if !chunk.bytes.is_empty() {
            let piece = TerminalsEvent::Output {
                subscription: sink.id().to_owned(),
                offset: chunk.offset,
            };
            sink.offer_with(&piece, &chunk.bytes)?;
            *at = Some(chunk.offset + chunk.bytes.len() as u64);
            continue;
        }
        loop {
            let event = tokio::select! {
                () = subscription.stop.cancelled() => return Ok(()),
                event = events.recv() => event,
            };
            match event {
                Err(RecvError::Closed) => return Ok(()),
                Ok(Event::Wrote { id, written }) if id == terminal && written > from => break,
                Ok(Event::Changed { .. }) | Err(RecvError::Lagged(_)) => break,
                Ok(_) => {}
            }
        }
    }
}
