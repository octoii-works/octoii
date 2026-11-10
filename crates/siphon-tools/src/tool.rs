//! A tool a thread's model calls: its name, what it does, the JSON Schema its arguments fit — its arguments' own type's
//! — what a call of it does to the machine, read from the call's arguments for the call's weighing, and its `exec`,
//! which gives what the model reads, as the model's vocabulary holds a call's output, or refuses, saying why.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use beak::Kernel;
use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use siphon_machine::paths::Base;
use siphon_machine::{Host, Machine};
use siphon_runtime::Runtime;
use tokio_util::sync::CancellationToken;

use crate::effect::Effect;
use crate::permissions::Writes;

/// Why a tool did not do what it was asked, in words the model reads.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Refusal(pub String);

impl Refusal {
    pub fn new(words: impl Into<String>) -> Self {
        Self(words.into())
    }
}

/// What a call's exec is given: the thread whose model called it, and its key, which names this call alone; where its
/// paths start; where it may write, as its weighing said; the machine and the runtime; and its cancellation — a stop
/// cuts it off, and it gives at once what has come.
#[derive(Debug, Clone)]
pub struct Context {
    pub thread: String,
    pub call: String,
    pub base: Base,
    pub writes: Writes,
    pub kernel: Kernel,
    pub host: Host,
    pub runtime: Runtime,
    pub cancelled: CancellationToken,
}

impl Context {
    /// The machine, opened for this call: held to the thread's workspace and the machine's temporary folders, or
    /// writing anywhere, as its weighing said.
    ///
    /// # Errors
    /// Beak's failure to open its namespace.
    pub fn machine(&self) -> Result<Machine, Refusal> {
        let writes = match self.writes {
            Writes::Anywhere => siphon_machine::Writes::Anywhere,
            Writes::Workspace => {
                let mut places = vec![self.base.workspace.clone()];
                places.extend(self.host.temporary.iter().cloned());
                siphon_machine::Writes::Within(places)
            }
        };
        Machine::open(&self.kernel, &self.host, self.base.clone(), &writes, None).map_err(|error| Refusal(format!("the machine: {error}")))
    }
}

/// A tool.
pub trait Tool: Send + Sync + 'static {
    /// Its name, as the model calls it: lowercase words joined by `_`.
    const NAME: &'static str;
    const DESCRIPTION: &'static str;
    type Args: JsonSchema + DeserializeOwned + Send;

    /// What a call with `args` does to the machine, its paths starting at `base`.
    fn effect(&self, args: &Self::Args, base: &Base) -> Effect;

    /// Does what a call with `args` asks.
    ///
    /// # Errors
    /// A `Refusal`, saying why, where it could not.
    fn exec(&self, context: &Context, args: Self::Args) -> impl Future<Output = Result<Vec<InputContent>, Refusal>> + Send;
}

type Execing = Pin<Box<dyn Future<Output = Result<Vec<InputContent>, Refusal>> + Send>>;

/// A tool of any kind, as a table holds it: its arguments as JSON.
trait Erased: Send + Sync {
    fn effect(&self, arguments: &serde_json::Value, base: &Base) -> Result<Effect, Refusal>;
    fn exec(self: Arc<Self>, context: Context, arguments: serde_json::Value) -> Execing;
}

impl<T: Tool> Erased for T {
    fn effect(&self, arguments: &serde_json::Value, base: &Base) -> Result<Effect, Refusal> {
        let args: T::Args = serde_json::from_value(arguments.clone()).map_err(|error| Refusal(format!("invalid arguments: {error}")))?;
        Ok(Tool::effect(self, &args, base))
    }

    fn exec(self: Arc<Self>, context: Context, arguments: serde_json::Value) -> Execing {
        Box::pin(async move {
            let args: T::Args = serde_json::from_value(arguments).map_err(|error| Refusal(format!("invalid arguments: {error}")))?;
            Tool::exec(&*self, &context, args).await
        })
    }
}

/// A tool as the model is told of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Declared {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

struct Entry {
    declared: Declared,
    tool: Arc<dyn Erased>,
}

/// The tools a thread's model may call, by name.
#[derive(Clone, Default)]
pub struct Tools {
    entries: Arc<BTreeMap<String, Entry>>,
}

impl std::fmt::Debug for Tools {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.entries.keys()).finish()
    }
}

impl Tools {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// These tools, `tool` among them.
    ///
    /// # Panics
    /// Where these tools are shared already, as they are built before they are.
    #[must_use]
    pub fn with<T: Tool>(mut self, tool: T) -> Self {
        let declared = Declared {
            name: T::NAME.to_owned(),
            description: T::DESCRIPTION.to_owned(),
            parameters: schemars::schema_for!(T::Args).to_value(),
        };
        let entries = Arc::get_mut(&mut self.entries).expect("tools are built before they are shared");
        entries.insert(
            T::NAME.to_owned(),
            Entry {
                declared,
                tool: Arc::new(tool),
            },
        );
        self
    }

    /// The tools, as the model is told of them, in order by name.
    #[must_use]
    pub fn declared(&self) -> Vec<Declared> {
        self.entries.values().map(|entry| entry.declared.clone()).collect()
    }

    /// What a call of `name` with `arguments` does to the machine, its arguments checked against the tool's schema.
    ///
    /// # Errors
    /// A `Refusal` naming no tool there is, or saying how the arguments do not fit.
    pub fn effect(&self, name: &str, arguments: &serde_json::Value, base: &Base) -> Result<Effect, Refusal> {
        let entry = self.entries.get(name).ok_or_else(|| Refusal(format!("no tool {name}")))?;
        let validator =
            jsonschema::validator_for(&entry.declared.parameters).map_err(|error| Refusal(format!("the tool's schema: {error}")))?;
        let problems: Vec<String> = validator
            .iter_errors(arguments)
            .map(|error| format!("{}: {error}", error.instance_path()))
            .collect();
        if !problems.is_empty() {
            return Err(Refusal(format!("invalid arguments: {}", problems.join("; "))));
        }
        entry.tool.effect(arguments, base)
    }

    /// Execs a call of `name` with `arguments`.
    ///
    /// # Errors
    /// A `Refusal` naming no tool there is, or the tool's own.
    pub async fn exec(&self, name: &str, context: Context, arguments: serde_json::Value) -> Result<Vec<InputContent>, Refusal> {
        let tool = Arc::clone(&self.entries.get(name).ok_or_else(|| Refusal(format!("no tool {name}")))?.tool);
        tool.exec(context, arguments).await
    }
}
