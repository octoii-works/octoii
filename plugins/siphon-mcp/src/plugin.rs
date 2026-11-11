//! The MCP servers as a plugin of Siphon's: their pool started on the machine, their tools a toolset of the model's and
//! its scripts', the trust a project's configuration waits for asked of the person, and their catalog told the model
//! of each thread at the place it works — the person asked first, under the thread, where they decided nothing on its
//! project's configuration as it is.

use std::fmt::Write as _;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use siphon::{Builder, Plugin};
use siphon_agent::{Teller, Telling};
use siphon_runtime::{Changes, Runtime};
use siphon_tools::projects::Place;
use siphon_tools::{Context, Refusal, Toolset};

use crate::trust::{Trust, TrustProgram};
use crate::{Catalog, Servers};

/// The MCP servers, as a host adds them.
#[derive(Debug, Clone, Copy, Default)]
pub struct McpPlugin;

impl Plugin for McpPlugin {
    fn name(&self) -> &'static str {
        "mcp"
    }

    fn build(self: Box<Self>, builder: &mut Builder) {
        let servers = Servers::start(builder.machine.clone(), builder.machine.home.as_deref());
        let toolset: Arc<dyn Toolset> = Arc::new(servers.clone());
        builder.tools(|tools| tools.with_toolset(toolset));
        builder.runtime(|runtime| runtime.program(TrustProgram).ask_person::<Trust>());
        builder.teller(Arc::new(servers));
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
    ) -> Pin<Box<dyn Future<Output = Result<Vec<octoii_ai::vocabulary::InputContent>, Refusal>> + Send + 'a>> {
        Box::pin(Servers::call(self, context, name, arguments))
    }
}

impl Teller for Servers {
    fn tell<'a>(
        &'a self,
        runtime: &'a Runtime,
        thread: &'a str,
        place: &'a Place,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Telling>> + Send + 'a>> {
        Box::pin(async move {
            let catalog = self.catalog(runtime, place).await?;
            if let Some(trust) = &catalog.asks {
                asked(runtime, thread, trust).await?;
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

/// Asks the person `trust` under the thread `thread`'s main process, where nothing asks it there yet.
async fn asked(runtime: &Runtime, thread: &str, trust: &Trust) -> anyhow::Result<()> {
    let main = siphon_threads::thread::main(thread);
    let asking = beak::layout::process(&main, TrustProgram::KEY.as_bytes());
    if runtime.snapshot()?.record(&asking)?.is_some() {
        return Ok(());
    }
    let mut changes = Changes::new();
    changes.start::<TrustProgram>(main, TrustProgram::KEY, trust)?;
    runtime.commit(changes).await?;
    Ok(())
}

/// The catalog of the MCP servers' tools as the model is told it: how they are called, their declarations, and what
/// could not be taken of the servers.
fn catalog_told(catalog: &Catalog) -> String {
    let mut out = String::from("<catalog>\n");
    if !catalog.tools.is_empty() {
        out.push_str("The MCP servers' tools, members of `tools` as a script of `execute` calls them:\n");
        out.push_str(&siphon_tools::declarations(&catalog.tools));
        out.push('\n');
    }
    for problem in &catalog.problems {
        let _ = writeln!(out, "[{problem}]");
    }
    out.push_str("</catalog>");
    out
}
