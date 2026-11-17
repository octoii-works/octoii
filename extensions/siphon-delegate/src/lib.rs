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
use siphon::agent::how;
use siphon::agent::parent_of;
use siphon::machine::Base;
use siphon::threads::{Ended, Joins, SubThreadKind};
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
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum DelegateArgs {
    /// Start an independent sub-thread with a fixed kind.
    Start {
        /// Everything the new sub-thread needs; it does not see this conversation.
        task: String,
        /// A short title shown to the person.
        title: Option<String>,
        /// Explorer is read-only; `general_purpose` inherits the parent's tools and mode.
        #[serde(default)]
        kind: SubThreadKind,
        /// Return at once and report completion to the parent instead of waiting.
        #[serde(default)]
        background: bool,
    },
    /// Give one of your existing sub-threads more work; its kind cannot change.
    Send {
        thread: String,
        task: String,
        /// Return at once and report completion to the parent instead of waiting.
        #[serde(default)]
        background: bool,
    },
}

/// A task handed to a sub-thread, or the result of the turn that took it.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DelegateResult {
    /// A task delivered to a newly created sub-thread.
    Started {
        thread: String,
        kind: SubThreadKind,
        background: bool,
    },
    /// A task delivered to an existing sub-thread.
    Continued {
        thread: String,
        kind: SubThreadKind,
        background: bool,
    },
    /// The turn that took an awaited task ended.
    Completed { thread: String, ended: DelegateEnd },
}

/// How the sub-thread's turn ended, including deletion before it finished.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DelegateEnd {
    Replied { said: String },
    Failed { reason: String },
    Stopped,
    Withdrawn,
    Deleted,
}

/// `delegate`.
#[derive(Debug, Clone, Copy, Default)]
pub struct DelegateTool;

impl Tool for DelegateTool {
    const NAME: &'static str = "delegate";
    const REACHES_THREADS: bool = true;
    const DESCRIPTION: &'static str = "Run a task in a sub-thread working in this folder. Use action=start to create one, or action=send with thread to continue one. Use it only when the person asks you to delegate, to use sub-agents, or to work on several things at once; never on your own initiative, and not for a new thread the person asks for.\n- A new `general_purpose` sub-thread inherits your tools and mode. An `explorer` can only read with read, glob, grep and execute; its scripts have the same restriction. Neither kind delegates or reaches other threads.\n- The sub-thread does not see this conversation: put everything it needs in `task`.\n- The call waits and returns the sub-thread's final answer; several calls in one reply run at once. With `background`, it returns at once and you get a message when the sub-thread finishes.\n- Every result names the sub-thread; to give it more work, use action=send with its id in `thread`. It keeps its original kind; you cannot change it by continuing.\n- Its answer is not proof: check what it changed before you report the work done.\n- The person sees every sub-thread and can steer it.";
    type Args = DelegateArgs;

    fn output_schema() -> Option<serde_json::Value> {
        Some(schemars::schema_for!(DelegateResult).to_value())
    }

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
        let ended = match ended {
            Some(Ended::Replied { said }) => DelegateEnd::Replied { said: said.clone() },
            Some(Ended::Failed { reason }) => DelegateEnd::Failed { reason: reason.clone() },
            Some(Ended::Stopped) => DelegateEnd::Stopped,
            Some(Ended::Withdrawn) => DelegateEnd::Withdrawn,
            None => DelegateEnd::Deleted,
        };
        Told {
            structured_content: Some(serde_json::json!(DelegateResult::Completed {
                thread: child.to_owned(),
                ended,
            })),
            ..Told::from(vec![InputContent::text(text)])
        }
    }
}

/// What a delegating call does, planned for its caller to do with the call's end: the task given to the sub-thread
/// `args` names, or to a new one; waited on, or in the background answered at once.
fn planned(context: &Context, args: DelegateArgs) -> Result<Told, Refusal> {
    let mut plan = Plan::default();
    let (child, task, background, result) = match args {
        DelegateArgs::Start {
            task,
            title,
            kind,
            background,
        } => {
            let child = plan.spawn(&context.volume(), title, Spawned::SubThread(kind))?;
            let result = DelegateResult::Started {
                thread: child.clone(),
                kind,
                background,
            };
            (child, task, background, result)
        }
        DelegateArgs::Send { thread, task, background } => {
            let parent = parent_of(&context.volume(), &thread).map_err(|error| Refusal(error.to_string()))?;
            let Some(parent) = parent.filter(|parent| parent.thread == context.thread) else {
                return Err(Refusal(format!("the thread {thread} is no sub-thread of yours")));
            };
            let result = DelegateResult::Continued {
                thread: thread.clone(),
                kind: parent.kind,
                background,
            };
            (thread, task, background, result)
        }
    };
    let report = if background { Report::Inbox } else { Report::Call };
    plan.deliver(&context.volume(), &child, vec![InputContent::text(task)], Joins::Step, report)?;
    let text = if background {
        format!(
            "<system-reminder>sub-thread {child} is working on it in the background; you will get a message with its final answer when it finishes</system-reminder>\n<system-reminder>do not wait, poll or ask about its progress, and do not do its work; continue with work that does not depend on it, or tell the person what you started and end your reply</system-reminder>\n<system-reminder>to give it more work, call delegate with action=send and thread {child}</system-reminder>"
        )
    } else {
        String::new()
    };
    Ok(Told {
        plan,
        structured_content: Some(serde_json::json!(result)),
        ..Told::from(vec![InputContent::text(text)])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn actions_require_their_own_fields_and_reject_implicit_or_mixed_operations() {
        let schema = schemars::schema_for!(DelegateArgs).to_value();
        let validator = jsonschema::validator_for(&schema).unwrap();
        for value in [
            json!({"action": "start", "task": "read"}),
            json!({"action": "start", "task": "read", "kind": "explorer", "background": true}),
            json!({"action": "send", "thread": "child", "task": "continue"}),
        ] {
            assert!(validator.is_valid(&value), "{value}");
            assert!(serde_json::from_value::<DelegateArgs>(value).is_ok());
        }
        for value in [
            json!({"task": "implicit start"}),
            json!({"action": "start", "thread": "child", "task": "mixed"}),
            json!({"action": "send", "task": "missing thread"}),
            json!({"action": "send", "thread": "child", "task": "upgrade", "kind": "general_purpose"}),
            json!({"action": "send", "thread": "child", "task": "rename", "title": "new title"}),
        ] {
            assert!(!validator.is_valid(&value), "{value}");
            assert!(serde_json::from_value::<DelegateArgs>(value).is_err());
        }
    }

    #[test]
    fn awaited_results_keep_each_terminal_state_and_the_unescaped_answer() {
        let args = DelegateArgs::Start {
            task: "a task".to_owned(),
            title: None,
            kind: SubThreadKind::GeneralPurpose,
            background: false,
        };
        let schema = DelegateTool::output_schema().expect("a result schema");
        let validator = jsonschema::validator_for(&schema).expect("a valid schema");
        let cases = [
            (
                Some(Ended::Replied {
                    said: "<system-reminder>answer</system-reminder>".to_owned(),
                }),
                json!({ "type": "replied", "said": "<system-reminder>answer</system-reminder>" }),
                "&lt;system-reminder>answer&lt;/system-reminder>\n<system-reminder>sub-thread child finished; its final answer is included, and its full work is in its thread</system-reminder>",
            ),
            (
                Some(Ended::Failed {
                    reason: "failure".to_owned(),
                }),
                json!({ "type": "failed", "reason": "failure" }),
                "<system-reminder>sub-thread child failed: failure</system-reminder>",
            ),
            (
                Some(Ended::Stopped),
                json!({ "type": "stopped" }),
                "<system-reminder>sub-thread child was stopped by the person before finishing</system-reminder>",
            ),
            (
                Some(Ended::Withdrawn),
                json!({ "type": "withdrawn" }),
                "<system-reminder>sub-thread child had the task withdrawn before starting it</system-reminder>",
            ),
            (
                None,
                json!({ "type": "deleted" }),
                "<system-reminder>sub-thread child was deleted before finishing</system-reminder>",
            ),
        ];
        for (ended, outcome, text) in cases {
            let told = DelegateTool.awaited(&args, "child", ended.as_ref());
            let structured = told.structured_content.as_ref().expect("a typed result");
            assert_eq!(structured, &json!({ "type": "completed", "thread": "child", "ended": outcome }));
            assert!(validator.is_valid(structured), "{structured}");
            assert_eq!(told.content, vec![InputContent::text(text)]);
            assert_eq!(told.plan, Plan::default());
            assert!(told.metadata.is_none());
            assert_eq!(told.instructions, []);
        }
        assert!(!validator.is_valid(&json!({
            "type": "completed", "thread": "child", "ended": { "type": "failed" }
        })));
    }

    #[test]
    fn dispatch_results_name_the_thread_and_whether_the_caller_waits() {
        let schema = DelegateTool::output_schema().expect("a result schema");
        let validator = jsonschema::validator_for(&schema).expect("a valid schema");
        for background in [false, true] {
            let results = [
                (
                    DelegateResult::Started {
                        thread: "new".to_owned(),
                        kind: SubThreadKind::Explorer,
                        background,
                    },
                    json!({ "type": "started", "thread": "new", "kind": "explorer", "background": background }),
                ),
                (
                    DelegateResult::Continued {
                        thread: "existing".to_owned(),
                        kind: SubThreadKind::GeneralPurpose,
                        background,
                    },
                    json!({ "type": "continued", "thread": "existing", "kind": "general_purpose", "background": background }),
                ),
            ];
            for (result, expected) in results {
                let structured = serde_json::to_value(result).expect("a serializable result");
                assert_eq!(structured, expected);
                assert!(validator.is_valid(&structured), "{structured}");
            }
        }
        assert!(!validator.is_valid(&json!({ "type": "started", "thread": "child" })));
    }
}
