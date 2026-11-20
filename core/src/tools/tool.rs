//! The contract a tool implements and its dispatch from JSON arguments.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use octoii_ai::vocabulary::InputContent;
use octoii_siphon_lib::{Batch, FiberId};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::machine::Base;
use crate::threads::{CallStatus, Ended};
use crate::tools::calls::Setting;
use crate::tools::{Context, Effect, Told};

/// Why a tool did not do what it was asked, in words the model reads.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Refusal(pub String);

impl Refusal {
    pub fn new(words: impl Into<String>) -> Self {
        Self(words.into())
    }
}

/// How a tool's program ended, retaining what it told even where it failed.
#[derive(Debug, Clone, PartialEq)]
pub struct Finished {
    pub status: CallStatus,
    pub told: Told,
}

impl Finished {
    pub(super) fn of(outcome: &octoii_siphon_lib::Outcome) -> Self {
        Self {
            status: if matches!(outcome, octoii_siphon_lib::Outcome::Completed(_)) {
                CallStatus::Ok
            } else {
                CallStatus::ToolError
            },
            told: Told::from(vec![InputContent::text(format!("{outcome:?}"))]),
        }
    }
}

/// A tool.
pub trait Tool: Send + Sync + 'static {
    /// Its name, as the model calls it: lowercase words joined by `_`.
    const NAME: &'static str;
    const DESCRIPTION: &'static str;
    /// Whether its calls start threads or reach other threads, which a sub-thread is not given, as it works on its
    /// task alone.
    const REACHES_THREADS: bool = false;
    type Args: JsonSchema + DeserializeOwned + Send;

    /// The schema of structuredContent, where this tool provides programmatic results.
    #[must_use]
    fn output_schema() -> Option<serde_json::Value> {
        None
    }

    /// What a call with `args` does to the machine, its paths starting at `base`.
    fn effect(&self, args: &Self::Args, base: &Base) -> Effect;

    /// Does what a call with `args` asks.
    ///
    /// # Errors
    /// A `Refusal`, saying why, where it could not.
    fn exec(&self, context: &Context, args: Self::Args) -> impl Future<Output = Result<Told, Refusal>> + Send;

    /// For a tool the person answers rather than execs: the JSON Schema their answer to a call with `args` fits.
    fn asks(&self, _: &Self::Args) -> Option<serde_json::Value> {
        None
    }

    /// For a tool the person answers: their `answer` to a call with `args`, as model content and programmatic data.
    fn answered(&self, _: &Self::Args, answer: serde_json::Value) -> Told {
        Told {
            content: vec![InputContent::text(answer.to_string())],
            structured_content: Some(answer),
            ..Told::default()
        }
    }

    /// What the model is told it does: its description, unless it says more as it stands — as a tool that tells the
    /// tools it calls does.
    fn described(&self) -> String {
        Self::DESCRIPTION.to_owned()
    }

    /// For a tool that runs as a program of its own: adds to `changes` the start of its process for a call with
    /// `args`, keyed `key`, under `parent`, the caller, its calls weighed as `setting` says; none for a tool exec'd.
    fn start(&self, _: &mut Batch<'_>, _: &FiberId, _: &str, _: &Self::Args, _: &Setting<'_>) -> Option<octoii_siphon_lib::Result<()>> {
        None
    }

    /// For a tool that runs as a program: what its process's end tells, as an exec of it tells it.
    fn finished(&self, outcome: &octoii_siphon_lib::Outcome) -> Finished {
        Finished::of(outcome)
    }

    /// For a call that ends with how the turn of the thread `thread` that took its delivery ended: what that tells,
    /// as `ended` says — none, the thread deleted before it did.
    fn awaited(&self, _: &Self::Args, thread: &str, ended: Option<&Ended>) -> Told {
        Told::from(vec![InputContent::text(crate::threads::inbox::told_of(thread, ended))])
    }
}

pub(super) type Execing = Pin<Box<dyn Future<Output = Result<Told, Refusal>> + Send>>;

/// The most characters of a call's arguments told back where they do not fit its tool.
const GIVEN_CHARS_MAX: usize = 1000;

/// The refusal of arguments `given` that do not fit their tool, for `error`: the error, then the arguments as they came.
pub(crate) fn invalid(error: &serde_json::Error, given: &str) -> Refusal {
    let shown: String = given.chars().take(GIVEN_CHARS_MAX).collect();
    let cut = if shown.len() < given.len() { " …" } else { "" };
    Refusal(format!(
        "invalid arguments: {error}\n<system-reminder>given: {shown}{cut}</system-reminder>"
    ))
}

/// A tool of any kind, as a table holds it: its arguments as JSON.
pub(super) trait Erased: Send + Sync {
    fn effect(&self, arguments: &serde_json::Value, base: &Base) -> Result<Effect, Refusal>;
    fn exec(self: Arc<Self>, context: Context, arguments: serde_json::Value) -> Execing;
    fn asks(&self, arguments: &serde_json::Value) -> Option<serde_json::Value>;
    fn answered(&self, arguments: &serde_json::Value, answer: serde_json::Value) -> Told;
    fn start(
        &self,
        changes: &mut Batch<'_>,
        parent: &FiberId,
        key: &str,
        arguments: &serde_json::Value,
        setting: &Setting<'_>,
    ) -> Option<octoii_siphon_lib::Result<()>>;
    fn finished(&self, outcome: &octoii_siphon_lib::Outcome) -> Finished;
    fn awaited(&self, arguments: &serde_json::Value, thread: &str, ended: Option<&Ended>) -> Told;
}

impl<T: Tool> Erased for T {
    fn effect(&self, arguments: &serde_json::Value, base: &Base) -> Result<Effect, Refusal> {
        let args: T::Args = serde_json::from_value(arguments.clone()).map_err(|error| invalid(&error, &arguments.to_string()))?;
        Ok(Tool::effect(self, &args, base))
    }

    fn asks(&self, arguments: &serde_json::Value) -> Option<serde_json::Value> {
        let args: T::Args = serde_json::from_value(arguments.clone()).ok()?;
        Tool::asks(self, &args)
    }

    fn answered(&self, arguments: &serde_json::Value, answer: serde_json::Value) -> Told {
        match serde_json::from_value::<T::Args>(arguments.clone()) {
            Ok(args) => Tool::answered(self, &args, answer),
            Err(_) => Told {
                content: vec![InputContent::text(answer.to_string())],
                structured_content: Some(answer),
                ..Told::default()
            },
        }
    }

    fn start(
        &self,
        changes: &mut Batch<'_>,
        parent: &FiberId,
        key: &str,
        arguments: &serde_json::Value,
        setting: &Setting<'_>,
    ) -> Option<octoii_siphon_lib::Result<()>> {
        let args: T::Args = serde_json::from_value(arguments.clone()).ok()?;
        Tool::start(self, changes, parent, key, &args, setting)
    }

    fn finished(&self, outcome: &octoii_siphon_lib::Outcome) -> Finished {
        Tool::finished(self, outcome)
    }

    fn awaited(&self, arguments: &serde_json::Value, thread: &str, ended: Option<&Ended>) -> Told {
        match serde_json::from_value::<T::Args>(arguments.clone()) {
            Ok(args) => Tool::awaited(self, &args, thread, ended),
            Err(_) => Told::from(vec![InputContent::text(crate::threads::inbox::told_of(thread, ended))]),
        }
    }

    fn exec(self: Arc<Self>, context: Context, arguments: serde_json::Value) -> Execing {
        Box::pin(async move {
            let given = arguments.to_string();
            let args: T::Args = serde_json::from_value(arguments).map_err(|error| invalid(&error, &given))?;
            Tool::exec(&*self, &context, args).await
        })
    }
}

/// A set of tools beyond those a thread's model is given one by one, found by their names as the set has them — an MCP
/// server's, each `<server>.<tool>`: whether a name is one of its, and a call of one done. Each may do anything, as its own end
/// checks what it is given.
pub trait Toolset: Send + Sync + 'static {
    /// Whether `name` is one of its tools'.
    fn has(&self, name: &str) -> bool;

    /// A call of its tool `name` with `arguments`, for `context`.
    fn call<'a>(
        &'a self,
        context: &'a Context,
        name: &'a str,
        arguments: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<Told, Refusal>> + Send + 'a>>;
}
