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

use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use siphon::agent::primitives::parent_of;
use siphon::agent::waiting::how;
use siphon::machine::paths::Base;
use siphon::threads::inbox::{Ended, Joins};
use siphon::tools::{Context, Effect, Plan, Refusal, Report, Spawned, Told, Tool};
use siphon::{Extension, Registrar};

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
    }
}

/// The arguments of `delegate`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DelegateArgs {
    /// The complete task. A new sub-thread knows nothing else.
    pub task: String,
    /// A short title for a new sub-thread, shown to the person.
    pub title: Option<String>,
    /// Id of one of your sub-threads, running or finished, to continue; it keeps its earlier work. Omit to start a new
    /// one.
    pub thread: Option<String>,
    /// Return at once; you get a message when the sub-thread finishes. Do not wait for it or poll.
    #[serde(default)]
    pub background: bool,
}

/// `delegate`.
#[derive(Debug, Clone, Copy, Default)]
pub struct DelegateTool;

impl Tool for DelegateTool {
    const NAME: &'static str = "delegate";
    const REACHES_THREADS: bool = true;
    const DESCRIPTION: &'static str = "Run a task in a sub-thread that has your tools and works in this folder. Use it only when the person asks you to delegate, to use sub-agents, or to work on several things at once; never on your own initiative, and not for a new thread the person asks for.\n- The sub-thread does not see this conversation: put everything it needs in `task`.\n- The call waits and returns the sub-thread's final answer; several calls in one reply run at once. With `background`, it returns at once and you get a message when the sub-thread finishes.\n- Every result names the sub-thread; to give it more work, pass its id in `thread`.\n- Its answer is not proof: check what it changed before you report the work done.\n- The person sees every sub-thread and can steer it.";
    type Args = DelegateArgs;

    fn effect(&self, _: &DelegateArgs, _: &Base) -> Effect {
        Effect::Delegates
    }

    fn exec(&self, context: &Context, args: DelegateArgs) -> impl Future<Output = Result<Told, Refusal>> + Send {
        std::future::ready(planned(context, args))
    }

    fn awaited(&self, _: &DelegateArgs, child: &str, ended: Option<&Ended>) -> Told {
        let text = match ended {
            Some(Ended::Replied { said }) => format!(
                "{}\n<system-reminder>sub-thread {child} {}</system-reminder>",
                siphon::reminder::escaped(said),
                how(ended)
            ),
            _ => format!("<system-reminder>sub-thread {child} {}</system-reminder>", how(ended)),
        };
        Told::from(vec![InputContent::text(text)])
    }
}

/// What a delegating call does, planned for its caller to do with the call's end: the task given to the sub-thread
/// `args` names, or to a new one; waited on, or in the background answered at once.
fn planned(context: &Context, args: DelegateArgs) -> Result<Told, Refusal> {
    let mut plan = Plan::default();
    let child = match &args.thread {
        Some(named) => {
            let parent = parent_of(&context.runtime, named).map_err(|error| Refusal(error.to_string()))?;
            if parent.as_deref() != Some(context.thread.as_str()) {
                return Err(Refusal(format!("the thread {named} is no sub-thread of yours")));
            }
            named.clone()
        }
        None => plan.spawn(&context.runtime, args.title.clone(), Spawned::SubThread)?,
    };
    let report = if args.background { Report::Inbox } else { Report::Call };
    plan.deliver(&context.runtime, &child, vec![InputContent::text(args.task)], Joins::Step, report)?;
    let text = if args.background {
        format!(
            "<system-reminder>sub-thread {child} is working on it in the background; you will get a message with its final answer when it finishes</system-reminder>\n<system-reminder>do not wait, poll or ask about its progress, and do not do its work; continue with work that does not depend on it, or tell the person what you started and end your reply</system-reminder>\n<system-reminder>to give it more work, call delegate with thread {child}</system-reminder>"
        )
    } else {
        String::new()
    };
    Ok(Told {
        plan,
        ..Told::from(vec![InputContent::text(text)])
    })
}
