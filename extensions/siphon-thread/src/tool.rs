//! The `thread` tool, by which a thread's model works with the person's other threads, as a shell's user starts and
//! signals other sessions — only where the person asks for another thread: starts one of its own, not a sub-thread, in
//! the thread's folder with its model, its mode and its effort, a message its first input; sends a thread a message,
//! which reaches its inbox as a sub-thread's task does, steering it where it runs; or stops a thread's turn. A message
//! says which thread it comes from. Where the model asks to be told, a process beside the thread's main process waits
//! on the turn that takes the message, then tells the thread what came of it, as a task delegated in the background
//! is told. No thread is sent a message by one whose mode reaches less far, which would reach further through it; a
//! sub-thread works with no other thread.

use std::future::Future;

use beak::layout;
use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use siphon_machine::paths::Base;
use siphon_runtime::{Changes, Ending, Program, Run};
use siphon_threads::inbox::{self, Inbound, Joins, ReplyTo};
use siphon_threads::{Config, Mode, NewThread, Threads, thread};
use siphon_tools::{Context, Effect, Refusal, Setting, Told, Tool};

use siphon_agent::ThreadProgram;
use siphon_agent::waiting::{DelegatedProgram, Waiting, directory_of, label};

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

    fn exec(&self, _: &Context, _: ThreadAction) -> impl Future<Output = Result<Told, Refusal>> + Send {
        std::future::ready(Err(Refusal::new("the thread tool runs as a program of its own")))
    }

    fn start(
        &self,
        changes: &mut Changes,
        parent: &Bytes,
        key: &str,
        action: &ThreadAction,
        setting: &Setting<'_>,
    ) -> Option<serde_json::Result<()>> {
        let input = ThreadToolInput {
            thread: setting.thread.to_owned(),
            action: action.clone(),
        };
        Some(changes.start::<ThreadToolProgram>(parent.clone(), key, &input).map(|_| ()))
    }

    fn finished(&self, outcome: &beak::Outcome) -> Told {
        let said = |bytes: Option<&[u8]>| {
            bytes
                .map(|bytes| serde_json::from_slice::<String>(bytes).unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned()))
                .unwrap_or_default()
        };
        let text = match outcome {
            beak::Outcome::Completed(result) | beak::Outcome::Stopped { result, .. } => said(result.as_deref()),
            beak::Outcome::Failed { error, .. } => said(error.as_deref()),
        };
        Told::from(vec![InputContent::text(text)])
    }
}

/// What a `thread` call's process is started with: the thread that calls, and what it does.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadToolInput {
    pub thread: String,
    pub action: ThreadAction,
}

/// The program of a `thread` call: does what it asks in one commit, and ends with what came of it.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadToolProgram;

impl Program for ThreadToolProgram {
    const KIND: &'static str = "siphon.thread-call";
    type Input = ThreadToolInput;
    type Checkpoint = ();
    type Output = String;

    fn run(&self, run: &mut Run<'_>, input: ThreadToolInput, _: Option<()>) -> anyhow::Result<Ending<(), String>> {
        let own = input.thread.as_str();
        let Some(directory) = directory_of(run, own)? else {
            return Ok(Ending::Fail("the thread is gone".into()));
        };
        if label(&directory, thread::PARENT_LABEL).is_some() {
            return Ok(Ending::Fail("a sub-thread works with no other thread".into()));
        }
        match input.action {
            ThreadAction::Start { message, title, report } => {
                let started = start(run, own, &directory, title)?;
                send(run, own, &started, &message, report)?;
                Ok(Ending::Done(format!(
                    "[the thread {started} started with your message{}]\n[name it to the person as :thread{{id={started}}}]",
                    told_note(report)
                )))
            }
            ThreadAction::Send { thread, message, report } => {
                if let Some(refused) = refused(run, own, &directory, &thread)? {
                    return Ok(Ending::Fail(refused));
                }
                let mut revived = Changes::new();
                Threads::<ThreadProgram>::revive(run.snapshot(), &mut revived, &thread)?;
                run.changes.extend(revived);
                send(run, own, &thread, &message, report)?;
                Ok(Ending::Done(format!("[the thread {thread} has your message{}]", told_note(report))))
            }
            ThreadAction::Stop { thread } => {
                if thread == own {
                    return Ok(Ending::Fail("a thread stops not itself: end your reply".into()));
                }
                if directory_of(run, &thread)?.is_none() {
                    return Ok(Ending::Fail(format!("no thread {thread}")));
                }
                let mut stopping = Changes::new();
                thread::stop_turn(run.snapshot(), &mut stopping, &thread)?;
                run.changes.extend(stopping);
                Ok(Ending::Done(format!("[the thread {thread} was asked to stop its turn]")))
            }
        }
    }
}

/// What a call that asked to be told is answered of it.
fn told_note(report: bool) -> &'static str {
    if report {
        "; you will be told by a message how the turn that takes it ends: do not wait for it or poll it"
    } else {
        ""
    }
}

/// Why the thread `own`, of the directory `directory`, may not send `target` a message: it is itself, it is not there,
/// or its mode reaches further than `own`'s; none where it may.
fn refused(run: &Run<'_>, own: &str, directory: &beak::Directory, target: &str) -> anyhow::Result<Option<String>> {
    if target == own {
        return Ok(Some("a thread sends itself no message".into()));
    }
    let Some(theirs) = directory_of(run, target)? else {
        return Ok(Some(format!("no thread {target}")));
    };
    let reach = |config: &Config| match config.mode.unwrap_or_default() {
        Mode::Default => 0,
        Mode::Auto => 1,
        Mode::FullAccess => 2,
    };
    if reach(&Config::of(&theirs)?) > reach(&Config::of(directory)?) {
        return Ok(Some(format!(
            "the thread {target} runs in a mode that reaches further than yours: the person must message it"
        )));
    }
    Ok(None)
}

/// Starts a thread of the person's like `own`, whose directory is `directory` — its folder, its model, its mode and its
/// effort — titled `title`, with the run's commit; its id.
fn start(run: &mut Run<'_>, own: &str, directory: &beak::Directory, title: Option<String>) -> anyhow::Result<String> {
    let config = Config::of(directory)?;
    let new = NewThread {
        actor: own.to_owned(),
        config: Config { title, ..config },
        project: label(directory, thread::PROJECT_LABEL),
        parent: None,
    };
    let started = run.new_id()?;
    let entry = run.new_id()?;
    Threads::<ThreadProgram>::made(&mut run.changes, &started, &entry, &new)?;
    Ok(started)
}

/// Puts `message` from the thread `own` in the inbox of the thread `target`, with the run's commit, saying whose it is;
/// where it asks to be told, the process that waits on it started beside `own`'s main process.
fn send(run: &mut Run<'_>, own: &str, target: &str, message: &str, report: bool) -> anyhow::Result<()> {
    let delivery = run.new_id()?;
    let waiter = format!("told-{delivery}");
    let reply_to = report.then(|| ReplyTo {
        process: String::from_utf8_lossy(&layout::process(&thread::main(own), waiter.as_bytes())).into_owned(),
        started: None,
        key: delivery.clone(),
    });
    let inbound = Inbound::Input {
        actor: own.to_owned(),
        content: vec![
            InputContent::text(format!("[a message from the thread {own}]")),
            InputContent::text(message.to_owned()),
        ],
        reply_to,
        joins: Joins::Step,
    };
    inbox::put(&mut run.changes, target, &delivery, &inbound)?;
    if report {
        let waiting = Waiting {
            thread: own.to_owned(),
            child: target.to_owned(),
            task: delivery,
        };
        run.changes.start::<DelegatedProgram>(thread::main(own), &waiter, &waiting)?;
    }
    Ok(())
}
