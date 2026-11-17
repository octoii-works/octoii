//! Memory as an extension of Octoii's: what a project's threads keep across threads, which their model is told how to
//! keep and given the index of, and which the person lists and deletes from.

pub mod memory;
pub mod protocol;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use octoii::agent::{Shared, Teller, Telling};
use octoii::machine::Host;
use octoii::protocol::{Error, Nothing};
use octoii::tools::Place;
use octoii::{Caller, Extension, Registrar};
use serde_json::json;

use crate::protocol::{MemoryListed, Method};

/// Memory, as a host adds it.
#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryExtension;

impl Extension for MemoryExtension {
    fn name(&self) -> &'static str {
        "memory"
    }

    fn title(&self) -> &'static str {
        "Memory"
    }

    fn description(&self) -> &'static str {
        "What the model keeps of each project across threads, which the person reads and deletes from."
    }

    fn build(self: Box<Self>, registrar: &mut Registrar<'_>) {
        let host = registrar.machine().clone();
        if let Some(folder) = host.memory.clone() {
            registrar.writable("memory folder", folder);
        }
        registrar.teller(Shared::Project, Arc::new(Told { host: host.clone() }));
        registrar.methods(protocol::set(), Method::read, move |_: Caller, method| {
            let host = host.clone();
            async move { answered(&host, method) }
        });
    }
}

/// The memory of the project a thread works in, where `host` keeps one, as its model is told it.
#[derive(Debug)]
struct Told {
    host: Host,
}

impl Teller for Told {
    fn tell<'a>(
        &'a self,
        _: &'a octoii::volume::Volume,
        _: &'a str,
        place: Option<&'a Place>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Telling>> + Send + 'a>> {
        let folder = place.and_then(|place| self.host.memory_of(&place.project));
        let telling = Telling {
            sections: folder
                .map(|folder| ("memory".to_owned(), memory::told(&folder)))
                .into_iter()
                .collect(),
            tools: Vec::new(),
        };
        Box::pin(std::future::ready(Ok(telling)))
    }
}

/// Does `method` on the memory `host` keeps.
fn answered(host: &Host, method: Method) -> Result<serde_json::Value, Error> {
    let refused = |code: &str, message: String| Error {
        code: code.to_owned(),
        message,
    };
    match method {
        Method::ListMemory(asked) => {
            let folder = memory_of(host, &asked.path);
            Ok(json!(MemoryListed {
                memories: folder.as_deref().map_or_default(memory::list),
                folder: folder.map(|folder| folder.to_string_lossy().into_owned()),
            }))
        }
        Method::DeleteMemory(asked) => {
            let folder = memory_of(host, &asked.path).ok_or_else(|| refused("no_memory", "this machine keeps no memory".into()))?;
            memory::delete(&folder, &asked.file).map_err(|error| refused("failed", error.to_string()))?;
            Ok(json!(Nothing {}))
        }
    }
}

/// The folder of the memory of the project the folder `path` is in, where `host` keeps memory.
fn memory_of(host: &Host, path: &str) -> Option<PathBuf> {
    host.memory_of(&Place::of(Path::new(path)).project)
}
