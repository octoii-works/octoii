//! The tools and toolsets available to a model, with schema and sub-thread checks.

use std::collections::BTreeMap;
use std::sync::Arc;

use octoii_ai::vocabulary::InputContent;
use octoii_siphon_lib::{Batch, FiberId};

use crate::machine::Base;
use crate::threads::{Ended, SubThreadKind};
use crate::tools::calls::Setting;
use crate::tools::tool::Erased;
use crate::tools::{Context, Effect, Finished, Refusal, Told, Tool, Toolset};

/// A tool as the model is told of it, and whether it reaches threads.
#[derive(Debug, Clone, PartialEq)]
pub struct Declared {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub output_schema: Option<serde_json::Value>,
    pub reaches_threads: bool,
}

#[derive(Clone)]
struct Entry {
    declared: Declared,
    tool: Arc<dyn Erased>,
}

/// A tool's `schema` as a call with `arguments` means it: where it is one of several variants, each naming itself by
/// fixed values — the action of a tool of many actions — the one variant whose fixed values the arguments give, so
/// arguments that do not fit are told how they miss that variant's fields, not that they fit none; else the whole.
fn meant<'a>(schema: &'a serde_json::Value, arguments: &serde_json::Value) -> std::borrow::Cow<'a, serde_json::Value> {
    let Some(variants) = schema.get("oneOf").and_then(serde_json::Value::as_array) else {
        return std::borrow::Cow::Borrowed(schema);
    };
    let named = |variant: &&serde_json::Value| {
        let fixed: Vec<(&String, &serde_json::Value)> = variant
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(name, property)| Some((name, property.get("const")?)))
            .collect();
        !fixed.is_empty() && fixed.iter().all(|(name, value)| arguments.get(name.as_str()) == Some(value))
    };
    match variants.iter().filter(named).collect::<Vec<_>>()[..] {
        // The variant alone, with the definitions the whole holds, which it may refer to.
        [one] => {
            let mut narrowed = one.clone();
            for shared in ["$defs", "definitions"] {
                if let Some(definitions) = schema.get(shared) {
                    narrowed[shared] = definitions.clone();
                }
            }
            std::borrow::Cow::Owned(narrowed)
        }
        _ => std::borrow::Cow::Borrowed(schema),
    }
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
            output_schema: T::output_schema(),
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

    /// Whether this fixed sub-thread kind may call `name`; ordinary threads retain their tools.
    pub(crate) fn allows(&self, name: &str, kind: Option<SubThreadKind>) -> bool {
        let Some(kind) = kind else {
            return self.has(name);
        };
        if let Some(entry) = self.entries.get(name) {
            return !entry.declared.reaches_threads
                && match kind {
                    SubThreadKind::Explorer => matches!(name, "read" | "glob" | "grep" | "execute"),
                    SubThreadKind::GeneralPurpose => true,
                };
        }
        kind == SubThreadKind::GeneralPurpose && self.served(name).is_some()
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
        let schema = meant(&entry.declared.parameters, arguments);
        let validator = jsonschema::validator_for(&schema).map_err(|error| Refusal(format!("the tool's schema: {error}")))?;
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
    pub fn answered(&self, name: &str, arguments: &serde_json::Value, answer: serde_json::Value) -> Told {
        match self.entries.get(name) {
            Some(entry) => entry.tool.answered(arguments, answer),
            None => Told {
                content: vec![InputContent::text(answer.to_string())],
                structured_content: Some(answer),
                ..Told::default()
            },
        }
    }

    /// Adds to `changes` the start of the process of a call of `name` with `arguments`, keyed `key`, under `parent`,
    /// where its tool runs as a program; none where it is exec'd.
    pub fn start(
        &self,
        changes: &mut Batch<'_>,
        parent: &FiberId,
        key: &str,
        name: &str,
        arguments: &serde_json::Value,
        setting: &Setting<'_>,
    ) -> Option<octoii_siphon_lib::Result<()>> {
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
    pub fn finished(&self, name: &str, outcome: &octoii_siphon_lib::Outcome) -> Finished {
        self.entries
            .get(name)
            .map_or_else(|| Finished::of(outcome), |entry| entry.tool.finished(outcome))
    }

    /// Execs a call of `name` with `arguments`.
    ///
    /// # Errors
    /// A `Refusal` naming no tool there is, or the tool's own.
    pub async fn exec(&self, name: &str, context: Context, arguments: serde_json::Value) -> Result<Told, Refusal> {
        if let Some(parent) = crate::threads::parent_of(&context.volume(), &context.thread).map_err(|error| Refusal(error.to_string()))?
            && !self.allows(name, Some(parent.kind))
        {
            return Err(Refusal(format!("the tool {name} is not available to this sub-thread")));
        }
        if let Some(toolset) = self.served(name) {
            return toolset.call(&context, name, arguments).await;
        }
        let tool = Arc::clone(&self.entries.get(name).ok_or_else(|| Refusal(format!("no tool {name}")))?.tool);
        tool.exec(context, arguments).await
    }
}
