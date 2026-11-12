//! Sub-threads: the `delegate` tool, by which a thread's model hands a task to a thread of its own, as a parent forks a
//! child — only where the person asks it to delegate, or to work on things at once. A new sub-thread is made in the
//! thread's folder, with its model, its mode and its effort, the task its first input; a task for one made before goes
//! to its inbox, steering it where it runs. The call waits till the turn that took the task ends, then gives what the
//! sub-thread said last; several calls in one reply run at once. One in the background is answered at once, and a
//! process beside the thread's main process — of no turn, so a stop of the turn reaches it not, as an interrupt reaches
//! the foreground alone — waits on the sub-thread instead, then delivers what came of it to the thread's inbox, as `at`
//! mails a job's output to whoever queued it. A sub-thread is a thread like any other, which the person follows and
//! steers; it delegates no further, its work on a task stops as the call that waits on it does, and it goes with its
//! thread.

use std::future::Future;

use beak::layout;
use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use siphon_machine::paths::Base;
use siphon_runtime::{Changes, Ending, Program, Run, Wait};
use siphon_threads::inbox::{self, Ended, Inbound, Joins, ReplyTo};
use siphon_threads::{Config, NewThread, Parent, Threads, thread};
use siphon_tools::{Context, Effect, Refusal, Setting, Told, Tool};

use crate::thread::ThreadProgram;

/// The arguments of `delegate`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DelegateArgs {
    /// What the sub-thread is to do: the whole of it, as a new one sees nothing else.
    pub task: String,
    /// A few words the person knows a new sub-thread by.
    pub title: Option<String>,
    /// The id of a sub-thread of yours, running or finished, to give this task to: it keeps what it did. Omit to start
    /// a new one.
    pub thread: Option<String>,
    /// Run it in the background and be answered at once: you are told when the sub-thread finishes, by a message. Do
    /// not wait for it, poll it or ask after its progress.
    #[serde(default)]
    pub background: bool,
}

/// `delegate`.
#[derive(Debug, Clone, Copy, Default)]
pub struct DelegateTool;

impl Tool for DelegateTool {
    const NAME: &'static str = "delegate";
    const REACHES_THREADS: bool = true;
    const DESCRIPTION: &'static str = "Hands a task to a sub-thread of your own, which works on it with the same tools in this folder. Call it only when the person asks you to delegate work, to use sub-agents, or to work on several things at once — never on your own initiative, and never for a new thread the person asks for, which `thread` starts. A new sub-thread sees nothing of this conversation: give `task` all it needs. You wait for it to finish and are given what it said last, several calls in one reply running at once; or, with `background`, you are answered at once and told when it finishes, for work that does not hold up yours. Every answer names the sub-thread, which `thread` gives more to do. What it says it did is not proof: look at what it changed before you report it done. The person sees each sub-thread, and can follow and steer it.";
    type Args = DelegateArgs;

    fn effect(&self, _: &DelegateArgs, _: &Base) -> Effect {
        Effect::Delegates
    }

    fn exec(&self, _: &Context, _: DelegateArgs) -> impl Future<Output = Result<Told, Refusal>> + Send {
        std::future::ready(Err(Refusal::new("a delegation runs as a program of its own")))
    }

    fn start(
        &self,
        changes: &mut Changes,
        parent: &Bytes,
        key: &str,
        args: &DelegateArgs,
        setting: &Setting<'_>,
    ) -> Option<serde_json::Result<()>> {
        let input = DelegateInput {
            thread: setting.thread.to_owned(),
            args: args.clone(),
        };
        Some(changes.start::<DelegateProgram>(parent.clone(), key, &input).map(|_| ()))
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

/// What a delegating call's process is started with: the thread that delegates, and the call's arguments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateInput {
    pub thread: String,
    pub args: DelegateArgs,
}

/// A task given to a sub-thread, waited on: the thread that gave it, the sub-thread, and the task, by its id in the
/// sub-thread's inbox, which is also the key of the mail that tells how the turn that took it ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiting {
    pub thread: String,
    pub child: String,
    pub task: String,
}

/// The program of a delegating call: gives the task to its sub-thread, then waits on it till the turn that took the
/// task ends, and ends with what it said last — or how its work ended short; under a stop, stops its work on the task.
#[derive(Debug, Clone, Copy, Default)]
pub struct DelegateProgram;

impl Program for DelegateProgram {
    const KIND: &'static str = "siphon.delegate";
    const STOP_HANDLER: bool = true;
    type Input = DelegateInput;
    type Checkpoint = Waiting;
    type Output = String;

    fn run(&self, run: &mut Run<'_>, input: DelegateInput, waiting: Option<Waiting>) -> anyhow::Result<Ending<Waiting, String>> {
        match (waiting, run.is_stop_handler()) {
            (None, true) => Ok(Ending::Stopped(Some("stopped before the task was given".into()))),
            (None, false) => give(run, &input.thread, &input.args),
            (Some(waiting), true) => stop(run, &waiting),
            (Some(waiting), false) => {
                let child = &waiting.child;
                match ended_of(run, &waiting)? {
                    Some(Ended::Replied { said }) => Ok(Ending::Done(format!(
                        "{said}\n[the sub-thread {child} {}]",
                        how(Some(&Ended::Replied { said: String::new() }))
                    ))),
                    Some(ended) => Ok(Ending::Fail(format!("[the sub-thread {child} {}]", how(Some(&ended))))),
                    None if gone(run, &waiting)? => Ok(Ending::Fail(format!("[the sub-thread {child} {}]", how(None)))),
                    None => Ok(wait_on(waiting)),
                }
            }
        }
    }
}

/// Waits on the task `waiting` names till the mail that tells how it ended comes, or the sub-thread goes.
fn wait_on(waiting: Waiting) -> Ending<Waiting, String> {
    let wait = Wait::new().mail(waiting.task.clone()).change(thread::path(&waiting.child));
    Ending::Wait(waiting, wait)
}

/// How the sub-thread of `waiting` ended its work on the task, as its mail tells, taken with the run's commit; none
/// while it has not.
fn ended_of(run: &mut Run<'_>, waiting: &Waiting) -> anyhow::Result<Option<Ended>> {
    let path = layout::mail(run.path(), waiting.task.as_bytes());
    let Some(_) = run.snapshot().stat(&path)? else { return Ok(None) };
    let payload = run.snapshot().read(&layout::payload(&path), 0, u64::MAX)?.unwrap_or_default();
    run.take_mail(&waiting.task);
    Ok(Some(serde_json::from_slice(&payload)?))
}

/// Whether the sub-thread of `waiting` went, deleted.
fn gone(run: &Run<'_>, waiting: &Waiting) -> anyhow::Result<bool> {
    Ok(run.snapshot().stat(&thread::path(&waiting.child))?.is_none())
}

/// How a sub-thread's work ended, as `ended` says — none, deleted before it did — said of it.
fn how(ended: Option<&Ended>) -> String {
    match ended {
        Some(Ended::Replied { .. }) => "finished: what it said last is here, and its whole work is in it".to_owned(),
        Some(Ended::Failed { reason }) => format!("failed: {reason}"),
        Some(Ended::Stopped) => "was stopped by the person before it finished".to_owned(),
        Some(Ended::Withdrawn) => "had the task taken back before it began it".to_owned(),
        None => "was deleted before it finished".to_owned(),
    }
}

/// The directory of the thread `id`, as the run's snapshot holds it.
pub(crate) fn directory_of(run: &Run<'_>, id: &str) -> anyhow::Result<Option<beak::Directory>> {
    Ok(run.snapshot().stat(&thread::path(id))?.and_then(|entry| entry.directory))
}

pub(crate) fn label(directory: &beak::Directory, name: &str) -> Option<String> {
    directory
        .label(name.as_bytes())
        .filter(|value| !value.is_empty())
        .map(|value| String::from_utf8_lossy(value).into_owned())
}

/// Gives the task to the sub-thread `args` names, or to a new one, with the run's commit; then waits on it, or, in the
/// background, starts what waits on it and ends with where the task went. A sub-thread is refused, as it delegates no
/// further.
fn give(run: &mut Run<'_>, own: &str, args: &DelegateArgs) -> anyhow::Result<Ending<Waiting, String>> {
    let Some(directory) = directory_of(run, own)? else {
        return Ok(Ending::Fail("the thread is gone".into()));
    };
    if label(&directory, thread::PARENT_LABEL).is_some() {
        return Ok(Ending::Fail("a sub-thread delegates no further: do the task yourself".into()));
    }
    let child = match &args.thread {
        Some(named) => {
            let parent = directory_of(run, named)?.and_then(|child| label(&child, thread::PARENT_LABEL));
            if parent.as_deref() != Some(own) {
                return Ok(Ending::Fail(format!("the thread {named} is no sub-thread of yours")));
            }
            let mut revived = Changes::new();
            Threads::<ThreadProgram>::revive(run.snapshot(), &mut revived, named)?;
            run.changes.extend(revived);
            named.clone()
        }
        None => make(run, own, &directory, args.title.clone())?,
    };
    let task = run.new_id()?;
    let background = format!("delegated-{task}");
    let reply_to = if args.background {
        ReplyTo {
            process: String::from_utf8_lossy(&layout::process(&thread::main(own), background.as_bytes())).into_owned(),
            started: None,
            key: task.clone(),
        }
    } else {
        ReplyTo {
            process: String::from_utf8_lossy(run.path()).into_owned(),
            started: run.snapshot().record(run.path())?.map(|record| record.created_seq),
            key: task.clone(),
        }
    };
    let inbound = Inbound::Input {
        actor: own.to_owned(),
        content: vec![InputContent::text(args.task.clone())],
        reply_to: Some(reply_to),
        joins: Joins::Step,
    };
    inbox::put(&mut run.changes, &child, &task, &inbound)?;
    let waiting = Waiting {
        thread: own.to_owned(),
        child: child.clone(),
        task,
    };
    if !args.background {
        return Ok(wait_on(waiting));
    }
    run.changes.start::<DelegatedProgram>(thread::main(own), &background, &waiting)?;
    Ok(Ending::Done(format!(
        "[the sub-thread {child} works on it in the background: you will be told when it finishes, by a message with what it said last]\n[do not wait, poll or ask after its progress, and do not do its work or work on what it works on; work on what does not hang on it, or tell the person what you started and end your reply]\n[to give it more to do or change what it does, call delegate with the thread {child}]"
    )))
}

/// Makes a sub-thread of the thread `own`, whose directory is `directory`, with the run's commit: its model, its
/// folder, its mode and its effort the thread's, titled `title`, its parent the call; its id.
fn make(run: &mut Run<'_>, own: &str, directory: &beak::Directory, title: Option<String>) -> anyhow::Result<String> {
    let config = Config::of(directory)?;
    let call = run.path().rsplit(|&byte| byte == b':').next().unwrap_or_default();
    let new = NewThread {
        actor: own.to_owned(),
        config: Config { title, ..config },
        project: label(directory, thread::PROJECT_LABEL),
        parent: Some(Parent {
            thread: own.to_owned(),
            call: String::from_utf8_lossy(call).into_owned(),
        }),
    };
    let child = run.new_id()?;
    let entry = run.new_id()?;
    Threads::<ThreadProgram>::made(&mut run.changes, &child, &entry, &new)?;
    Ok(child)
}

/// Stops the sub-thread's work on the task: the task taken back where it waits in the inbox, the turn that runs asked
/// to stop otherwise; and ends stopped.
fn stop(run: &mut Run<'_>, waiting: &Waiting) -> anyhow::Result<Ending<Waiting, String>> {
    let task = inbox::file(&waiting.child, &waiting.task);
    if run.snapshot().stat(&task)?.is_some() {
        run.changes.delete(task);
    } else {
        let mut stopping = Changes::new();
        thread::stop_turn(run.snapshot(), &mut stopping, &waiting.child)?;
        run.changes.extend(stopping);
    }
    Ok(Ending::Stopped(Some("stopped, and the sub-thread's work on it with it".into())))
}

/// The program of a task given in the background — delegated to a sub-thread, or a message sent to another thread to be
/// told of — beside its thread's main process: waits on the thread that took it till the turn that took it ends, then
/// ends, what came of it delivered to the thread's inbox in the same commit.
#[derive(Debug, Clone, Copy, Default)]
pub struct DelegatedProgram;

impl Program for DelegatedProgram {
    const KIND: &'static str = "siphon.delegated";
    type Input = Waiting;
    type Checkpoint = ();
    type Output = ();

    fn run(&self, run: &mut Run<'_>, waiting: Waiting, _: Option<()>) -> anyhow::Result<Ending<(), ()>> {
        let ended = match ended_of(run, &waiting)? {
            Some(ended) => Some(ended),
            None if gone(run, &waiting)? => None,
            None => {
                let wait = Wait::new().mail(waiting.task.clone()).change(thread::path(&waiting.child));
                return Ok(Ending::Wait((), wait));
            }
        };
        let ours = directory_of(run, &waiting.child)?
            .and_then(|child| label(&child, thread::PARENT_LABEL))
            .as_deref()
            == Some(waiting.thread.as_str());
        let note = if ours {
            format!(
                "[the sub-thread {}, which you gave a task in the background, {}]",
                waiting.child,
                how(ended.as_ref())
            )
        } else {
            format!("[the thread {}, which you sent a message, {}]", waiting.child, how(ended.as_ref()))
        };
        let mut content = vec![InputContent::text(note)];
        if let Some(Ended::Replied { said }) = ended {
            content.push(InputContent::text(said));
        }
        let inbound = Inbound::Input {
            actor: waiting.child.clone(),
            content,
            reply_to: None,
            joins: Joins::Step,
        };
        let delivery = run.new_id()?;
        inbox::put(&mut run.changes, &waiting.thread, &delivery, &inbound)?;
        Ok(Ending::Done(()))
    }
}
