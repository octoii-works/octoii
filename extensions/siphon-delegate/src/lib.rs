//! Sub-threads as an extension of Siphon's: the `delegate` tool, by which a thread's model hands a task to a thread of
//! its own, as a parent forks a child — only where the person asks it to delegate, or to work on things at once. A new
//! sub-thread is made in the thread's folder, with its model, its mode and its effort, the task its first input; a task
//! for one made before goes to its inbox, steering it where it runs. The call waits till the turn that took the task
//! ends, then gives what the sub-thread said last; several calls in one reply run at once. One in the background is
//! answered at once, and a process beside the thread's main process — of no turn, so a stop of the turn reaches it
//! not, as an interrupt reaches the foreground alone — waits on the sub-thread instead. A sub-thread is a thread like
//! any other, which the person follows and steers; it delegates no further, its work on a task stops as the call that
//! waits on it does, and it goes with its thread.

use std::future::Future;

use beak::layout;
use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use siphon::agent::ThreadProgram;
use siphon::agent::waiting::{DelegatedProgram, Waiting, directory_of, ended_of, gone, how, label};
use siphon::machine::paths::Base;
use siphon::threads::inbox::{self, Ended, Inbound, Joins, ReplyTo};
use siphon::threads::{Config, NewThread, Parent, Threads, thread};
use siphon::tools::{Context, Effect, Refusal, Setting, Told, Tool};
use siphon::{Extension, Registrar};
use siphon_runtime::{Changes, Ending, Program, Run, Wait};

/// Sub-threads, as a host adds them.
#[derive(Debug, Clone, Copy, Default)]
pub struct DelegateExtension;

impl Extension for DelegateExtension {
    fn name(&self) -> &'static str {
        "delegate"
    }

    fn title(&self) -> &'static str {
        "Sub-threads"
    }

    fn description(&self) -> &'static str {
        "The model hands tasks to sub-threads of its own, when the person asks it to delegate or work on things at once."
    }

    fn build(self: Box<Self>, registrar: &mut Registrar<'_>) {
        registrar.tool(DelegateTool);
        registrar.runtime(|runtime| runtime.program(DelegateProgram));
    }
}

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
    const DESCRIPTION: &'static str = "Hands a task to a sub-thread of your own, which works on it with the same tools in this folder. Call it only when the person asks you to delegate work, to use sub-agents, or to work on several things at once — never on your own initiative, and never for a new thread the person asks for. A new sub-thread sees nothing of this conversation: give `task` all it needs. You wait for it to finish and are given what it said last, several calls in one reply running at once; or, with `background`, you are answered at once and told when it finishes, for work that does not hold up yours. Every answer names the sub-thread: give it more to do by its id in `thread`. What it says it did is not proof: look at what it changed before you report it done. The person sees each sub-thread, and can follow and steer it.";
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
