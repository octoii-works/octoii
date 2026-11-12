//! What a thread's model is told, gathered before each of its turns by a `siphon.gather` request, in a task of its own:
//! the sections each teller tells — Siphon's own and its extensions' — the most widely shared first, so a provider's
//! cache of the beginning serves one thread as well as the next; and the tools it may call, Siphon's and its
//! extensions', and the tools its provider runs that an extension adds, where its model's facts say it does. What an
//! extension the person
//! switched off brings is left out.

use std::collections::HashMap;
use std::sync::Arc;

use crate::threads::{Config, thread};
use crate::tools::Tools;
use crate::tools::projects::Place;
use octoii_ai::models::Models;
use octoii_ai::vocabulary::Tool;
use serde::{Deserialize, Serialize};
use siphon_runtime::{Asked, Request, Runtime, Unanswered};

use crate::agent::told::Told;

/// The name Siphon's own tools and tellers go by, which no setting switches off.
pub const CORE: &str = "core";

/// What a gather request asks: what the model of the thread `thread`, by its id, is told now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gather {
    pub thread: String,
}

impl Request for Gather {
    const NAME: &'static str = "siphon.gather";
    type Answer = Told;
}

/// What tells a thread's model of itself and the place it works in, gathered before each turn: sections of what it is
/// told, and the names of the tools its scripts may call beside its own — as the MCP servers tell their catalog.
pub trait Teller: Send + Sync + 'static {
    /// What it tells the model of the thread `thread`, working at `place` — none for a thread of no folder — reading
    /// `volume`.
    ///
    /// # Errors
    /// What keeps it from telling.
    fn tell<'a>(
        &'a self,
        volume: &'a crate::Volume,
        thread: &'a str,
        place: Option<&'a Place>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Telling>> + Send + 'a>>;
}

/// What a teller tells: its sections, by name, and the tools a script may call by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Telling {
    pub sections: Vec<(String, String)>,
    pub tools: Vec<String>,
}

/// How widely what a teller tells is shared, which orders it: alike for every thread of the machine, of a project, or
/// a thread's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Shared {
    Machine,
    Project,
    Thread,
}

/// A teller as Siphon holds it: the extension it came from, and how widely what it tells is shared.
#[derive(Clone)]
pub struct OwnedTeller {
    pub extension: String,
    pub shared: Shared,
    pub teller: Arc<dyn Teller>,
}

impl std::fmt::Debug for OwnedTeller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnedTeller")
            .field("extension", &self.extension)
            .field("shared", &self.shared)
            .finish_non_exhaustive()
    }
}

/// Where what the model is told comes from: the tools it may call, and the extension each came from; the tellers, in
/// the order told — the most widely shared first, Siphon's before its extensions', theirs by name; and the models'
/// facts.
#[derive(Clone)]
pub struct Sources {
    pub tools: Tools,
    pub owners: HashMap<String, String>,
    pub tellers: Vec<OwnedTeller>,
    /// The tools a model's provider runs, each with the extension it came from.
    pub natives: Vec<(String, Tool)>,
    pub models: Arc<Models>,
}

impl Sources {
    /// `tellers` in the order told.
    #[must_use]
    pub fn ordered(mut tellers: Vec<OwnedTeller>) -> Vec<OwnedTeller> {
        tellers.sort_by(|a, b| (a.shared, a.extension != CORE, &a.extension).cmp(&(b.shared, b.extension != CORE, &b.extension)));
        tellers
    }
}

impl std::fmt::Debug for Sources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sources").field("tools", &self.tools).finish_non_exhaustive()
    }
}

/// Answers the gather request `asked` from `sources`.
///
/// # Errors
/// `Failed` where the thread cannot be read.
pub async fn answer_gather(sources: &Sources, asked: Asked<Gather>) -> Result<Told, Unanswered> {
    gathered(sources, &asked.runtime, &asked.payload.thread)
        .await
        .map_err(|error| Unanswered::Failed(error.to_string()))
}

/// Whether `model`'s provider runs the tool `native` for it, as its facts say.
fn runs(model: &octoii_ai::catalogue::Model, native: &Tool) -> bool {
    match native {
        Tool::WebSearch => model.searches_web,
        Tool::Function { .. } => false,
    }
}

/// What the model of the thread `id` is told now, as `sources` gather it on `runtime`.
///
/// # Errors
/// The volume's failure; a thread's settings not as Siphon writes them.
pub async fn gathered(sources: &Sources, runtime: &Runtime, id: &str) -> anyhow::Result<Told> {
    let snapshot = runtime.snapshot()?;
    let directory = snapshot
        .stat(&thread::path(id))?
        .and_then(|entry| entry.directory)
        .unwrap_or_default();
    let config = Config::of(&directory)?;
    let disabled = crate::settings::environment(&snapshot)?.disabled_extensions;
    let on = |extension: &str| extension == CORE || !disabled.iter().any(|off| off == extension);
    let place = config
        .workspace
        .as_deref()
        .map(|workspace| Place::of(std::path::Path::new(workspace)));
    let volume = crate::Volume { runtime: runtime.clone() };
    let mut told = Told::default();
    for owned in sources.tellers.iter().filter(|owned| on(&owned.extension)) {
        let telling = owned.teller.tell(&volume, id, place.as_ref()).await?;
        told.sections.extend(telling.sections);
        told.catalog.extend(telling.tools);
    }
    // A sub-thread works on its task alone: it starts no thread and reaches none.
    let sub_thread = directory
        .label(thread::PARENT_LABEL.as_bytes())
        .is_some_and(|parent| !parent.is_empty());
    told.tools = sources
        .tools
        .declared()
        .into_iter()
        .filter(|declared| sources.owners.get(&declared.name).is_none_or(|extension| on(extension)))
        .filter(|declared| !(sub_thread && declared.reaches_threads))
        .map(|declared| Tool::Function {
            name: declared.name,
            description: declared.description,
            parameters: declared.parameters,
        })
        .collect();
    let model = config.model.as_deref().and_then(|model| sources.models.model(model));
    for (extension, native) in &sources.natives {
        if on(extension) && model.as_ref().is_some_and(|model| runs(model, native)) {
            told.tools.push(native.clone());
        }
    }
    Ok(told)
}
