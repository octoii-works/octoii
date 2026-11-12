//! `todo`: the model's list of the steps of its work, which the person sees above the chat — the whole list each call,
//! replacing the one before. The list is the latest call's arguments, kept in the log as every call is: nothing else
//! holds it.

use std::fmt::Write as _;

use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use siphon_machine::paths::Base;
use ts_rs::TS;

use crate::effect::Effect;
use crate::told::{Structured, Told};
use crate::tool::{Context, Refusal, Tool};

/// Where a step stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

/// A step of the work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
pub struct Todo {
    /// What the step is, in a few words.
    pub content: String,
    pub status: TodoStatus,
}

/// The arguments of `todo`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TodoArgs {
    /// Every step, in order: the list that replaces the one before.
    pub todos: Vec<Todo>,
}

/// `todo`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TodoTool;

impl Tool for TodoTool {
    const NAME: &'static str = "todo";
    const DESCRIPTION: &'static str = "Keeps the list of the steps of your work, which the person sees above the chat. Give the whole list each time: it replaces the one before. Keep one step in_progress, mark each completed as soon as it is done, and add the steps you find on the way. Use it for work of three steps or more, or where the person asks for several things; not for one simple step or a question.";
    type Args = TodoArgs;

    fn effect(&self, _: &TodoArgs, _: &Base) -> Effect {
        Effect::Reads
    }

    fn exec(&self, _: &Context, args: TodoArgs) -> impl std::future::Future<Output = Result<Told, Refusal>> + Send {
        let words = vec![InputContent::text(counted(&args.todos))];
        std::future::ready(Ok(Told::structured(words, &Structured::Todos { todos: args.todos })))
    }
}

/// The list `todos` as its call's result tells it: how many steps are done, and the one in progress.
fn counted(todos: &[Todo]) -> String {
    if todos.is_empty() {
        return "the list is cleared".into();
    }
    let done = todos.iter().filter(|todo| todo.status == TodoStatus::Completed).count();
    let mut out = format!("{done} of {} steps completed", todos.len());
    for todo in todos.iter().filter(|todo| todo.status == TodoStatus::InProgress) {
        let _ = write!(out, "; in progress: {}", todo.content);
    }
    out
}
