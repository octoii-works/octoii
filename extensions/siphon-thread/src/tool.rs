//! The `thread` tool, by which a thread's model works with the person's other threads, as a shell's user starts and
//! signals other sessions — only where the person asks for another thread: starts one of its own, not a sub-thread, in
//! the thread's folder with its model, its mode and its effort, a message its first input; sends a thread a message,
//! which reaches its inbox as a sub-thread's task does, steering it where it runs; or stops a thread's turn. A message
//! says which thread it comes from. Where the model asks to be told, a process beside the thread's main process waits
//! on the turn that takes the message, then tells the thread what came of it, as a task delegated in the background
//! is told. No thread is sent a message by one whose mode reaches less far, which would reach further through it; a
//! sub-thread works with no other thread.

use std::future::Future;

use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use siphon::machine::paths::Base;
use siphon::threads::inbox::Joins;
use siphon::tools::{Context, Effect, Plan, Refusal, Report, Spawned, Told, Tool};

/// What the model does with the person's other threads.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ThreadAction {
    /// Starts a thread of the person's, in this folder, with your model and mode, `message` its first input.
    Start {
        /// What the new thread is to do: the whole of it, as it sees nothing of this conversation.
        message: String,
        /// A few words the person knows the thread by.
        title: Option<String>,
        /// Be told, by a message, how the turn that takes `message` ends.
        #[serde(default)]
        report: bool,
    },
    /// Sends a thread a message, which it takes at its next step, or begins a turn with.
    Send {
        /// The thread's id.
        thread: String,
        message: String,
        /// Be told, by a message, how the turn that takes `message` ends.
        #[serde(default)]
        report: bool,
    },
    /// Stops the turn a thread works on.
    Stop {
        /// The thread's id.
        thread: String,
    },
}

/// `thread`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadTool;

impl Tool for ThreadTool {
    const NAME: &'static str = "thread";
    const REACHES_THREADS: bool = true;
    const DESCRIPTION: &'static str = "Works with the person's other threads, which they see in their list: start makes a new thread in this folder with your model and mode, `message` its first input; send gives a thread a message, which it takes at its next step or begins a turn with; stop ends the turn a thread works on. Use it only when the person asks for a new thread, or names one to message or stop; never for a sub-agent, delegating or working on things at once. A new thread sees nothing of this conversation: give `message` all it needs. With `report`, you are told by a message how the turn that takes it ends; do not wait for it or poll it. Name a thread in your reply as :thread{id=<its id>}, or ::thread{id=<its id>} for its card.";
    type Args = ThreadAction;

    fn effect(&self, _: &ThreadAction, _: &Base) -> Effect {
        Effect::Delegates
    }

    fn exec(&self, context: &Context, action: ThreadAction) -> impl Future<Output = Result<Told, Refusal>> + Send {
        std::future::ready(planned(context, action))
    }
}

/// What a `thread` call does, planned for its caller to do with the call's end, and what it tells.
fn planned(context: &Context, action: ThreadAction) -> Result<Told, Refusal> {
    let own = context.thread.as_str();
    let mut plan = Plan::default();
    let text = match action {
        ThreadAction::Start { message, title, report } => {
            let started = plan.spawn(&context.runtime, title, Spawned::Thread)?;
            plan.deliver(&context.runtime, &started, from(own, message), Joins::Step, reported(report))?;
            format!(
                "[the thread {started} started with your message{}]\n[name it to the person as :thread{{id={started}}}]",
                told_note(report)
            )
        }
        ThreadAction::Send { thread, message, report } => {
            plan.deliver(&context.runtime, &thread, from(own, message), Joins::Step, reported(report))?;
            format!("[the thread {thread} has your message{}]", told_note(report))
        }
        ThreadAction::Stop { thread } => {
            plan.stop(&thread);
            format!("[the thread {thread} was asked to stop its turn]")
        }
    };
    Ok(Told {
        plan,
        ..Told::from(vec![InputContent::text(text)])
    })
}

/// `message` as the thread `own` sends it: saying whose it is.
fn from(own: &str, message: String) -> Vec<InputContent> {
    vec![
        InputContent::text(format!("[a message from the thread {own}]")),
        InputContent::text(message),
    ]
}

/// Who is told how the turn that takes a message ends: the calling thread, by a message, where it asked; no one else.
fn reported(report: bool) -> Report {
    if report { Report::Inbox } else { Report::None }
}

/// What a call that asked to be told is answered of it.
fn told_note(report: bool) -> &'static str {
    if report {
        "; you will be told by a message how the turn that takes it ends: do not wait for it or poll it"
    } else {
        ""
    }
}
