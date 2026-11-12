//! A tool a thread's model calls: its name, what it does, the JSON Schema its arguments fit — its arguments' own type's
//! — what a call of it does to the machine, read from the call's arguments for the call's weighing, and its `exec`,
//! which gives what the model reads, as the model's vocabulary holds a call's output, or refuses, saying why.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use crate::machine::paths::Base;
use crate::machine::{Host, Machine};
use beak::Kernel;
use bytes::Bytes;
use octoii_ai::vocabulary::{ImageDetail, InputContent};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use siphon_runtime::{Changes, Runtime};
use tokio_util::sync::CancellationToken;

use crate::threads::inbox::Ended;
use crate::tools::calls::Setting;
use crate::tools::effect::Effect;
use crate::tools::permissions::Writes;
use crate::tools::told::Told;

/// Why a tool did not do what it was asked, in words the model reads.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Refusal(pub String);

impl Refusal {
    pub fn new(words: impl Into<String>) -> Self {
        Self(words.into())
    }
}

/// What a call's exec is given: the thread whose model called it; the process the call is of, under which what it runs
/// on the machine is recorded; its key, which names this call alone; where its paths start; where it may write, as its
/// weighing said; the machine and the runtime; and its cancellation — a stop cuts it off, and it gives at once what has
/// come.
#[derive(Debug, Clone)]
pub struct Context {
    pub thread: String,
    pub process: Bytes,
    pub call: String,
    pub base: Base,
    pub writes: Writes,
    /// The places beside the workspace its work writes as freely.
    pub places: Vec<PathBuf>,
    pub kernel: Kernel,
    pub host: Host,
    pub runtime: Runtime,
    pub cancelled: CancellationToken,
}

impl Context {
    /// The machine, opened for this call: held to the thread's workspace and the places beside it written as freely, or
    /// writing anywhere, as its weighing said.
    ///
    /// # Errors
    /// Beak's failure to open its namespace.
    pub fn machine(&self) -> Result<Machine, Refusal> {
        Machine::open(&self.kernel, &self.host, self.base.clone(), &self.writes_of(), None)
            .map_err(|error| Refusal(format!("the machine: {error}")))
    }

    /// Where this call's work may write, as its weighing said: its thread's workspace and the places beside it written
    /// as freely, made where they are not, or anywhere.
    #[must_use]
    pub fn writes_of(&self) -> crate::machine::Writes {
        match self.writes {
            Writes::Anywhere => crate::machine::Writes::Anywhere,
            Writes::Workspace => {
                let places = self.places.iter().filter(|place| made(place)).cloned();
                crate::machine::Writes::Within(std::iter::once(self.base.workspace.clone()).chain(places).collect())
            }
        }
    }

    /// The variables a thread's commands run with over the machine's own: `TMPDIR`, its scratchpad, so what their work
    /// leaves for a while goes there rather than into the workspace.
    #[must_use]
    pub fn variables(&self) -> Vec<(String, String)> {
        self.scratchpad()
            .map(|scratchpad| ("TMPDIR".to_owned(), scratchpad.to_string_lossy().into_owned()))
            .into_iter()
            .collect()
    }

    /// The thread's scratchpad, made where it is not — the system clears its temporary folder.
    fn scratchpad(&self) -> Option<PathBuf> {
        self.host.scratchpad_of(&self.thread).filter(made)
    }

    /// The image `bytes`, named `name` where it has one, as the model is shown it: fitted to what every model reads,
    /// kept in the thread's files by its content, after a line saying what it is.
    ///
    /// # Errors
    /// A `Refusal` where it is no image that can be read, or it could not be kept.
    pub async fn show_image(&self, bytes: &[u8], name: Option<&str>) -> Result<Vec<InputContent>, Refusal> {
        let snapshot = self.runtime.snapshot().map_err(|error| Refusal(error.to_string()))?;
        let mut changes = Changes::new();
        let (file, size) = crate::threads::files::keep_image(&snapshot, &mut changes, &self.thread, bytes, name)
            .map_err(|error| Refusal(format!("{}: {error}", name.unwrap_or("the image"))))?;
        if !changes.is_empty() {
            self.runtime.commit(changes).await.map_err(|error| Refusal(error.to_string()))?;
        }
        let size = format!("{}×{}", size.width, size.height);
        let said = match name {
            Some(name) => format!("{name}, an image of {size}"),
            None => format!("an image of {size}"),
        };
        Ok(vec![
            InputContent::text(said),
            InputContent::InputImage {
                file,
                detail: ImageDetail::Auto,
            },
        ])
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

    /// For a tool the person answers: their `answer` to a call with `args`, as the model reads it.
    fn answered(&self, _: &Self::Args, answer: serde_json::Value) -> Vec<InputContent> {
        vec![InputContent::text(answer.to_string())]
    }

    /// What the model is told it does: its description, unless it says more as it stands — as a tool that tells the
    /// tools it calls does.
    fn described(&self) -> String {
        Self::DESCRIPTION.to_owned()
    }

    /// For a tool that runs as a program of its own: adds to `changes` the start of its process for a call with
    /// `args`, keyed `key`, under `parent`, the caller, its calls weighed as `setting` says; none for a tool exec'd.
    fn start(&self, _: &mut Changes, _: &Bytes, _: &str, _: &Self::Args, _: &Setting<'_>) -> Option<serde_json::Result<()>> {
        None
    }

    /// For a tool that runs as a program: what its process's end tells, as an exec of it tells it.
    fn finished(&self, outcome: &beak::Outcome) -> Told {
        Told::from(vec![InputContent::text(format!("{outcome:?}"))])
    }

    /// For a call that ends with how the turn of the thread `thread` that took its delivery ended: what that tells,
    /// as `ended` says — none, the thread deleted before it did.
    fn awaited(&self, _: &Self::Args, thread: &str, ended: Option<&Ended>) -> Told {
        Told::from(vec![InputContent::text(crate::threads::inbox::told_of(thread, ended))])
    }
}

type Execing = Pin<Box<dyn Future<Output = Result<Told, Refusal>> + Send>>;

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
trait Erased: Send + Sync {
    fn effect(&self, arguments: &serde_json::Value, base: &Base) -> Result<Effect, Refusal>;
    fn exec(self: Arc<Self>, context: Context, arguments: serde_json::Value) -> Execing;
    fn asks(&self, arguments: &serde_json::Value) -> Option<serde_json::Value>;
    fn answered(&self, arguments: &serde_json::Value, answer: serde_json::Value) -> Vec<InputContent>;
    fn start(
        &self,
        changes: &mut Changes,
        parent: &Bytes,
        key: &str,
        arguments: &serde_json::Value,
        setting: &Setting<'_>,
    ) -> Option<serde_json::Result<()>>;
    fn finished(&self, outcome: &beak::Outcome) -> Told;
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

    fn answered(&self, arguments: &serde_json::Value, answer: serde_json::Value) -> Vec<InputContent> {
        match serde_json::from_value::<T::Args>(arguments.clone()) {
            Ok(args) => Tool::answered(self, &args, answer),
            Err(_) => vec![InputContent::text(answer.to_string())],
        }
    }

    fn start(
        &self,
        changes: &mut Changes,
        parent: &Bytes,
        key: &str,
        arguments: &serde_json::Value,
        setting: &Setting<'_>,
    ) -> Option<serde_json::Result<()>> {
        let args: T::Args = serde_json::from_value(arguments.clone()).ok()?;
        Tool::start(self, changes, parent, key, &args, setting)
    }

    fn finished(&self, outcome: &beak::Outcome) -> Told {
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

/// A tool as the model is told of it, and whether it reaches threads.
#[derive(Debug, Clone, PartialEq)]
pub struct Declared {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub reaches_threads: bool,
}

#[derive(Clone)]
struct Entry {
    declared: Declared,
    tool: Arc<dyn Erased>,
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

/// A tool's parameters as the model is given them: their JSON Schema without what speaks to a schema's reader rather than
/// to the model — the dialect, and the Rust type's name and doc.
fn parameters(mut schema: serde_json::Value) -> serde_json::Value {
    if let Some(object) = schema.as_object_mut() {
        for key in ["$schema", "title", "description"] {
            object.remove(key);
        }
    }
    schema
}

/// The tools a thread's model may call, by name: its own, and those of its toolsets.
#[derive(Clone, Default)]
pub struct Tools {
    entries: Arc<BTreeMap<String, Entry>>,
    toolsets: Vec<Arc<dyn Toolset>>,
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

    /// These tools, `tool` among them; others that share them left as they are.
    #[must_use]
    pub fn with<T: Tool>(mut self, tool: T) -> Self {
        let declared = Declared {
            name: T::NAME.to_owned(),
            description: tool.described(),
            parameters: parameters(schemars::schema_for!(T::Args).to_value()),
            reaches_threads: T::REACHES_THREADS,
        };
        Arc::make_mut(&mut self.entries).insert(
            T::NAME.to_owned(),
            Entry {
                declared,
                tool: Arc::new(tool),
            },
        );
        self
    }

    /// These tools, and those of `toolset`.
    #[must_use]
    pub fn with_toolset(mut self, toolset: Arc<dyn Toolset>) -> Self {
        self.toolsets.push(toolset);
        self
    }

    /// The toolset `name` is a tool of: none of these tools', but one a toolset has.
    fn served(&self, name: &str) -> Option<&Arc<dyn Toolset>> {
        if self.entries.contains_key(name) {
            return None;
        }
        self.toolsets.iter().find(|toolset| toolset.has(name))
    }

    /// Whether there is a tool of `name`: one of these, or one a server may have.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.entries.contains_key(name) || self.served(name).is_some()
    }

    /// The tools, as the model is told of them, in order by name.
    #[must_use]
    pub fn declared(&self) -> Vec<Declared> {
        self.entries.values().map(|entry| entry.declared.clone()).collect()
    }

    /// What a call of `name` with `arguments` does to the machine, its arguments checked against the tool's schema;
    /// a toolset's tool acts, as it may do anything.
    ///
    /// # Errors
    /// A `Refusal` naming no tool there is, or saying how the arguments do not fit.
    pub fn effect(&self, name: &str, arguments: &serde_json::Value, base: &Base) -> Result<Effect, Refusal> {
        // A toolset's tool may do anything; its own end checks what it is given.
        if self.served(name).is_some() {
            return Ok(Effect::Acts { tool: name.to_owned() });
        }
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

    /// The JSON Schema the person's answer to a call of `name` with `arguments` fits, where the person answers it.
    #[must_use]
    pub fn asks(&self, name: &str, arguments: &serde_json::Value) -> Option<serde_json::Value> {
        self.entries.get(name)?.tool.asks(arguments)
    }

    /// The person's `answer` to a call of `name` with `arguments`, as the model reads it.
    #[must_use]
    pub fn answered(&self, name: &str, arguments: &serde_json::Value, answer: serde_json::Value) -> Vec<InputContent> {
        match self.entries.get(name) {
            Some(entry) => entry.tool.answered(arguments, answer),
            None => vec![InputContent::text(answer.to_string())],
        }
    }

    /// Adds to `changes` the start of the process of a call of `name` with `arguments`, keyed `key`, under `parent`,
    /// where its tool runs as a program; none where it is exec'd.
    pub fn start(
        &self,
        changes: &mut Changes,
        parent: &Bytes,
        key: &str,
        name: &str,
        arguments: &serde_json::Value,
        setting: &Setting<'_>,
    ) -> Option<serde_json::Result<()>> {
        self.entries.get(name)?.tool.start(changes, parent, key, arguments, setting)
    }

    /// What the end of the turn of the thread `thread` that took the delivery of a call of `name` with `arguments` tells,
    /// as `ended` says.
    #[must_use]
    pub fn awaited(&self, name: &str, arguments: &serde_json::Value, thread: &str, ended: Option<&Ended>) -> Told {
        self.entries.get(name).map_or_else(
            || Told::from(vec![InputContent::text(crate::threads::inbox::told_of(thread, ended))]),
            |entry| entry.tool.awaited(arguments, thread, ended),
        )
    }

    /// What the end `outcome` of the process of a call of `name` tells.
    #[must_use]
    pub fn finished(&self, name: &str, outcome: &beak::Outcome) -> Told {
        self.entries.get(name).map_or_else(
            || Told::from(vec![InputContent::text(format!("{outcome:?}"))]),
            |entry| entry.tool.finished(outcome),
        )
    }

    /// Execs a call of `name` with `arguments`.
    ///
    /// # Errors
    /// A `Refusal` naming no tool there is, or the tool's own.
    pub async fn exec(&self, name: &str, context: Context, arguments: serde_json::Value) -> Result<Told, Refusal> {
        if let Some(toolset) = self.served(name) {
            return toolset.call(&context, name, arguments).await;
        }
        let tool = Arc::clone(&self.entries.get(name).ok_or_else(|| Refusal(format!("no tool {name}")))?.tool);
        tool.exec(context, arguments).await
    }
}

/// Whether the folder `folder` is there, made where it was not, as a place work writes must be to be mounted.
fn made(folder: &PathBuf) -> bool {
    std::fs::create_dir_all(folder).is_ok()
}
