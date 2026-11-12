//! The MCP servers as an extension of Siphon's: their pool started on the machine, their tools a toolset of the model's and
//! its scripts', the trust a project's configuration waits for asked of the person, and their catalog told the model
//! of each thread at the place it works — the person asked first, under the thread, where they decided nothing on its
//! project's configuration as it is.

use std::fmt::Write as _;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use siphon::agent::{Shared, Teller, Telling};
use siphon::protocol::Error;
use siphon::tools::projects::Place;
use siphon::tools::{Context, Refusal, Toolset};
use siphon::{Caller, Extension, Registrar};
use siphon_runtime::Runtime;

use crate::protocol;
use crate::{Catalog, Servers};

/// The MCP servers, as a host adds them: the folder of the person's own configuration, whose `mcp.json` names their
/// servers for every project, where they keep one.
#[derive(Debug, Clone, Default)]
pub struct McpExtension {
    pub config: Option<std::path::PathBuf>,
}

impl Extension for McpExtension {
    fn name(&self) -> &'static str {
        "mcp"
    }

    fn title(&self) -> &'static str {
        "MCP"
    }

    fn description(&self) -> &'static str {
        "The tools of the MCP servers the person and the project configure."
    }

    fn build(self: Box<Self>, registrar: &mut Registrar<'_>) {
        let servers = Servers::start(registrar.machine().clone(), self.config.as_deref());
        let toolset: Arc<dyn Toolset> = Arc::new(servers.clone());
        registrar.toolset(toolset);
        let listing = servers.clone();
        registrar.methods(protocol::set(), protocol::Method::read, move |caller: Caller, method| {
            let servers = listing.clone();
            async move { answered(&servers, &caller, method).await }
        });
        registrar.teller(Shared::Project, Arc::new(servers));
    }
}

impl Toolset for Servers {
    /// A server's tool: its server's name and the tool's after a dot.
    fn has(&self, name: &str) -> bool {
        name.contains('.')
    }

    fn call<'a>(
        &'a self,
        context: &'a Context,
        name: &'a str,
        arguments: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<siphon::tools::Told, Refusal>> + Send + 'a>> {
        Box::pin(Servers::call(self, context, name, arguments))
    }
}

impl Teller for Servers {
    fn tell<'a>(
        &'a self,
        runtime: &'a Runtime,
        thread: &'a str,
        place: Option<&'a Place>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Telling>> + Send + 'a>> {
        Box::pin(async move {
            // The servers are configured for the person and a project: a thread of no folder has none of a project's.
            let Some(place) = place else { return Ok(Telling::default()) };
            let catalog = self.catalog(runtime, place).await?;
            if let Some(trust) = &catalog.asks {
                siphon::trust::ask(runtime, thread, trust).await?;
            }
            let mut telling = Telling::default();
            if !catalog.tools.is_empty() || !catalog.problems.is_empty() {
                telling.sections.push(("catalog".to_owned(), catalog_told(&catalog)));
            }
            telling.tools = catalog.tools.iter().map(|tool| tool.name.clone()).collect();
            Ok(telling)
        })
    }
}

/// Does `method` on the servers, for the client of `caller`.
async fn answered(servers: &Servers, caller: &Caller, method: protocol::Method) -> Result<serde_json::Value, Error> {
    let runtime = &caller.siphon.runtime;
    let failed = |error: &dyn std::fmt::Display| Error {
        code: "failed".into(),
        message: error.to_string(),
    };
    match method {
        protocol::Method::ListServers(asked) => {
            let place = asked
                .workspace
                .as_deref()
                .map(|workspace| Place::of(std::path::Path::new(workspace)));
            let listed = servers.listed(runtime, place.as_ref()).await.map_err(|error| failed(&error))?;
            serde_json::to_value(listed).map_err(|error| failed(&error))
        }
    }
}

/// The catalog of the MCP servers' tools as the model is told it: how they are called, their declarations, and what
/// could not be taken of the servers.
fn catalog_told(catalog: &Catalog) -> String {
    let mut out = String::from("<catalog>\n");
    if !catalog.tools.is_empty() {
        out.push_str("Tools of MCP servers, called from an execute script on `tools`:\n");
        out.push_str(&siphon::tools::declarations(&catalog.tools));
        out.push('\n');
    }
    for problem in &catalog.problems {
        let _ = writeln!(out, "[{problem}]");
    }
    out.push_str("</catalog>");
    out
}
