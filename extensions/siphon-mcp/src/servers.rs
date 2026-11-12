//! The MCP servers a thread's tools reach: the person's own, as the Octoii Home's `mcp.json` names them; then those its
//! project's `.mcp.json` names, once the person trusts them as they are — one of a name the person's has standing for
//! theirs, as the nearer setting holds. A server is connected as it is first needed, then kept and shared by every
//! thread, and stopped once nothing used it for `IDLE`, to be connected again as something next does; one that ended
//! is connected anew. A command's server runs on the machine as a coprocess, through the namespace, as the person's
//! own programs do but for what the host keeps for itself. A tool of one is named `<server>.<tool>`, as a mount table
//! names what is under a mount point.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use beak::Snapshot;
use octoii_ai::vocabulary::InputContent;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, ResourceContents};
use rmcp::service::{Peer, RoleClient, RunningService, ServiceExt};
use rmcp::transport::{StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig};
use siphon::machine::paths::Base;
use siphon::machine::{Host, Machine, Variables, Writes};
use siphon::{Ends, Kept};
use siphon_runtime::Runtime;

use crate::config::{Configured, Launch, OWN_FILE, PROJECT_FILE, Server, parse};
use crate::trust::{self, ShownServer, Trust};
use siphon::tools::projects::Place;
use siphon::tools::{Context, Declared, Refusal, Told};

/// How long a server nothing uses keeps running before it is stopped.
pub const IDLE: Duration = Duration::from_secs(600);
/// How long a server may take to be connected.
const CONNECTING_MAX: Duration = Duration::from_secs(30);
/// The most of a configuration read.
const CONFIGURATION_BYTES_MAX: u64 = 1 << 20;

/// What a thread's model is told of the servers it reaches: their tools, what could not be taken of them, and what the
/// person is asked to trust, where they decided nothing on its project's configuration as it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    pub tools: Vec<Declared>,
    pub problems: Vec<String>,
    pub asks: Option<Trust>,
}

/// The servers, kept connected as they are used.
#[derive(Debug, Clone)]
pub struct Servers {
    kept: Kept<Server, Connection, String>,
    host: Host,
    own: Option<PathBuf>,
}

/// A server connected, its service shared by all that use it, which ends as the last of them goes.
#[derive(Debug, Clone)]
struct Connection(Arc<RunningService<RoleClient, ()>>);

impl Ends for Connection {
    fn ended(&self) -> bool {
        self.0.peer().is_transport_closed()
    }
}

impl Servers {
    /// The servers run on the machine `host` gives; the person's own named in the Octoii Home `home`, where there is
    /// one. They are stopped as the last handle goes.
    #[must_use]
    pub fn start(host: Host, home: Option<&Path>) -> Self {
        Self {
            kept: Kept::new(IDLE),
            host,
            own: home.map(|home| home.join(OWN_FILE)),
        }
    }

    /// The servers a thread working at `place` reaches, as the configurations are now, `variables` the host's.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn configured(&self, snapshot: &Snapshot, place: &Place, variables: &dyn Fn(&str) -> Option<String>) -> beak::Result<Found> {
        let mut found = Found::default();
        if let Some(path) = &self.own
            && let Some(text) = read(path, &mut found.configured.problems)
        {
            let folder = path.parent().unwrap_or(path);
            found.configured.extend(parse(path, &text, folder, variables));
        }
        let path = place.project.join(PROJECT_FILE);
        let Some(text) = read(&path, &mut found.configured.problems) else {
            return Ok(found);
        };
        let project = parse(&path, &text, &place.project, variables);
        if project.servers.is_empty() {
            found.configured.problems.extend(project.problems);
            return Ok(found);
        }
        let folder = place.project.to_string_lossy().into_owned();
        let digest = trust::digest(text.as_bytes());
        match trust::decided(snapshot, &folder, &digest)? {
            Some(true) => found.configured.extend(project),
            Some(false) => found.configured.problems.push(format!(
                "the MCP servers of {} are not run: the person did not trust them",
                path.display()
            )),
            None => {
                found.configured.problems.push(format!(
                    "the MCP servers of {} are not run till the person trusts them",
                    path.display()
                ));
                let servers = project
                    .servers
                    .iter()
                    .map(|server| ShownServer {
                        name: server.name.clone(),
                        runs: server.runs(),
                    })
                    .collect();
                found.asks = Some(Trust {
                    folder,
                    path: path.to_string_lossy().into_owned(),
                    digest,
                    servers,
                });
            }
        }
        Ok(found)
    }

    /// What a thread working at `place` is told of the servers it reaches, each connected and its tools listed, as
    /// `runtime` holds the person's trust.
    ///
    /// # Errors
    /// The volume's failure.
    pub async fn catalog(&self, runtime: &Runtime, place: &Place) -> beak::Result<Catalog> {
        let found = self.configured(&runtime.snapshot()?, place, &host_variable)?;
        let listing = found.configured.servers.iter().map(|server| async move {
            let peer = self.peer(server, runtime).await?;
            peer.list_all_tools().await.map_err(|error| error.to_string())
        });
        let listed = futures::future::join_all(listing).await;
        let mut catalog = Catalog {
            problems: found.configured.problems,
            asks: found.asks,
            ..Catalog::default()
        };
        for (server, listed) in found.configured.servers.iter().zip(listed) {
            match listed {
                Ok(tools) => catalog.tools.extend(tools.into_iter().map(|tool| Declared {
                    name: format!("{}.{}", server.name, tool.name),
                    description: tool.description.map(String::from).unwrap_or_default(),
                    parameters: serde_json::Value::Object((*tool.input_schema).clone()),
                    reaches_threads: false,
                })),
                Err(why) => catalog.problems.push(format!("the MCP server {}: {why}", server.name)),
            }
        }
        Ok(catalog)
    }

    /// Calls the tool `name` — `<server>.<tool>` — with `arguments`, as the server the call's thread reaches by that
    /// name answers it.
    ///
    /// # Errors
    /// A `Refusal` where no server has the name, the server could not be reached, or the tool said it failed.
    pub async fn call(&self, context: &Context, name: &str, arguments: serde_json::Value) -> Result<Told, Refusal> {
        let (named, tool) = name.split_once('.').ok_or_else(|| Refusal(format!("no tool {name}")))?;
        let snapshot = context.runtime.snapshot().map_err(|error| Refusal(error.to_string()))?;
        let found = self
            .configured(&snapshot, &Place::of(&context.base.workspace), &host_variable)
            .map_err(|error| Refusal(error.to_string()))?;
        let server = found
            .configured
            .servers
            .into_iter()
            .find(|server| server.name == named)
            .ok_or_else(|| Refusal(format!("no MCP server {named} where this thread works")))?;
        let mut params = CallToolRequestParams::new(tool.to_owned());
        params.arguments = match arguments {
            serde_json::Value::Object(arguments) => Some(arguments),
            serde_json::Value::Null => None,
            _ => return Err(Refusal("its arguments are not an object".into())),
        };
        let peer = self
            .peer(&server, &context.runtime)
            .await
            .map_err(|why| Refusal(format!("the MCP server {named}: {why}")))?;
        let result = tokio::select! {
            result = peer.call_tool(params) => result.map_err(|error| Refusal(format!("the MCP server {named}: {error}")))?,
            () = context.cancelled.cancelled() => return Err(Refusal("the call was cut off".into())),
        };
        given(context, result).await
    }

    /// The person's own configuration's servers, for every project.
    fn own_configured(&self) -> Configured {
        let mut configured = Configured::default();
        if let Some(path) = &self.own
            && let Some(text) = read(path, &mut configured.problems)
        {
            let folder = path.parent().unwrap_or(path);
            configured.extend(parse(path, &text, folder, &host_variable));
        }
        configured
    }

    /// What the project at `place` names in its configuration, with the person's decision on it as it is; none where
    /// it names no server.
    fn project(snapshot: &Snapshot, place: &Place) -> beak::Result<Option<ProjectServers>> {
        let path = place.project.join(PROJECT_FILE);
        let Some(text) = read(&path, &mut Vec::new()) else {
            return Ok(None);
        };
        let project = parse(&path, &text, &place.project, &host_variable);
        if project.servers.is_empty() {
            return Ok(None);
        }
        let folder = place.project.to_string_lossy().into_owned();
        let digest = trust::digest(text.as_bytes());
        let trusted = trust::decided(snapshot, &folder, &digest)?;
        let servers = project
            .servers
            .iter()
            .map(|server| ShownServer {
                name: server.name.clone(),
                runs: server.runs(),
            })
            .collect();
        let named = Trust {
            folder,
            path: path.to_string_lossy().into_owned(),
            digest,
            servers,
        };
        Ok(Some(ProjectServers { named, trusted }))
    }

    /// Each server of the person's and the project's at `place` as it stands: connected or not, and the tools of each
    /// connected — none connected to tell — what of the configurations could not be taken, the person's own
    /// configuration, and what the project's names with the person's decision on it as it is.
    ///
    /// # Errors
    /// The volume's failure.
    pub async fn listed(&self, runtime: &Runtime, place: Option<&Place>) -> beak::Result<ServersListed> {
        let snapshot = runtime.snapshot()?;
        let found = match place {
            Some(place) => self.configured(&snapshot, place, &host_variable)?,
            None => Found {
                configured: self.own_configured(),
                asks: None,
            },
        };
        let held = self.kept.held(found.configured.servers.clone()).await;
        let mut servers = Vec::new();
        for (server, connection) in found.configured.servers.iter().zip(held) {
            let tools = match &connection {
                Some(connection) => connection
                    .0
                    .list_all_tools()
                    .await
                    .map(|tools| tools.into_iter().map(|tool| tool.name.to_string()).collect())
                    .unwrap_or_default(),
                None => Vec::new(),
            };
            servers.push(ServerListed {
                name: server.name.clone(),
                source: server.source.to_string_lossy().into_owned(),
                runs: server.runs(),
                connected: connection.is_some(),
                tools,
            });
        }
        let project = match place {
            Some(place) => Self::project(&snapshot, place)?,
            None => None,
        };
        Ok(ServersListed {
            servers,
            problems: found.configured.problems,
            person: self.own.as_ref().map(|own| own.to_string_lossy().into_owned()),
            project,
        })
    }

    /// The peer of `server`, connected now where it is not, its command started by `runtime`'s kernel.
    async fn peer(&self, server: &Server, runtime: &Runtime) -> Result<Peer<RoleClient>, String> {
        let connecting = connect(runtime.clone(), self.host.clone(), server.clone());
        let connection = self.kept.get(server.clone(), connecting).await?;
        Ok(connection.0.peer().clone())
    }
}

/// A server as the person is shown it: its name, the configuration it comes from by its path, what it runs or where it
/// is reached, whether it is connected now, and its tools' names where it is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct ServerListed {
    pub name: String,
    pub source: String,
    pub runs: String,
    pub connected: bool,
    pub tools: Vec<String>,
}

/// A project's configuration of servers as the person decides on it: what it names, and whether they trust it — none
/// where they decided nothing on it as it is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct ProjectServers {
    pub named: Trust,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub trusted: Option<bool>,
}

/// The servers a thread working at a place would use: each as it stands, what of their configurations could not be
/// taken, the person's own configuration by its path, and the project's, with what the person decided on it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct ServersListed {
    pub servers: Vec<ServerListed>,
    pub problems: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub person: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project: Option<ProjectServers>,
}

/// The servers a thread reaches, and what the person is asked to trust of them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Found {
    pub configured: Configured,
    pub asks: Option<Trust>,
}

/// The host's variable `name`.
fn host_variable(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// The text of the configuration at `path`; none where there is none, or it could not be read, which is a problem.
fn read(path: &Path, problems: &mut Vec<String>) -> Option<String> {
    match std::fs::metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Ok(metadata) if metadata.len() > CONFIGURATION_BYTES_MAX => {
            problems.push(format!(
                "{} is past the {CONFIGURATION_BYTES_MAX} bytes a configuration may be",
                path.display()
            ));
            return None;
        }
        _ => {}
    }
    std::fs::read_to_string(path)
        .map_err(|error| problems.push(format!("{}: {error}", path.display())))
        .ok()
}

/// `server` connected, its command started by `runtime`'s kernel on the machine `host` gives; else why it could not be.
async fn connect(runtime: Runtime, host: Host, server: Server) -> Result<Connection, String> {
    match &server.launch {
        Launch::Command { command, args, env, cwd } => {
            let base = Base {
                workspace: cwd.clone(),
                home: host.home.clone(),
            };
            let machine = Machine::open(runtime.kernel(), &host, base, &Writes::Anywhere, None).map_err(|error| error.to_string())?;
            let mut argv = vec![command.clone()];
            argv.extend(args.iter().cloned());
            let coprocess = machine
                .coprocess(runtime.now_ms(), b"", argv, cwd, Variables::Over(env.clone()))
                .map_err(|error| error.to_string())?;
            let serving = ().serve((coprocess.stdout, coprocess.stdin));
            match tokio::time::timeout(CONNECTING_MAX, serving).await {
                Ok(Ok(service)) => Ok(Connection(Arc::new(service))),
                failed => {
                    let why = match failed {
                        Ok(Err(error)) => error.to_string(),
                        _ => format!("it did not answer within {CONNECTING_MAX:?}"),
                    };
                    let errors = coprocess.stderr.kept().unwrap_or_default();
                    let errors = String::from_utf8_lossy(&errors);
                    Err(if errors.trim().is_empty() {
                        why
                    } else {
                        format!("{why}; it wrote:\n{}", errors.trim_end())
                    })
                }
            }
        }
        Launch::Http { url, headers } => {
            let mut sent = HashMap::new();
            for (name, value) in headers {
                let name = http::HeaderName::try_from(name.as_str()).map_err(|error| format!("the header {name}: {error}"))?;
                let value = http::HeaderValue::try_from(value.as_str()).map_err(|error| format!("the header {name}: {error}"))?;
                sent.insert(name, value);
            }
            let config = StreamableHttpClientTransportConfig::with_uri(url.as_str()).custom_headers(sent);
            let serving = ().serve(StreamableHttpClientTransport::from_config(config));
            match tokio::time::timeout(CONNECTING_MAX, serving).await {
                Ok(Ok(service)) => Ok(Connection(Arc::new(service))),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Err(format!("it did not answer within {CONNECTING_MAX:?}")),
            }
        }
    }
}

/// What a server's tool gave, as the model reads it: its text, each block on a line of its own; of what is no text, an
/// image shown, the rest noted; its structured content where it gave no text; refused where it says it failed.
async fn given(context: &Context, result: CallToolResult) -> Result<Told, Refusal> {
    let mut texts = Vec::new();
    let mut images = Vec::new();
    for block in result.content {
        match block {
            ContentBlock::Text(text) => texts.push(text.text),
            ContentBlock::Image(image) => match base64::engine::general_purpose::STANDARD.decode(&image.data) {
                Ok(bytes) => images.push(bytes),
                Err(error) => texts.push(format!("[an image of {} that could not be read: {error}]", image.mime_type)),
            },
            ContentBlock::Audio(audio) => texts.push(format!("[audio of {}, not shown]", audio.mime_type)),
            ContentBlock::Resource(embedded) => match embedded.resource {
                ResourceContents::TextResourceContents { uri, text, .. } => texts.push(format!("[{uri}]\n{text}")),
                ResourceContents::BlobResourceContents { uri, mime_type, blob, .. } => {
                    let kind = mime_type.unwrap_or_else(|| "bytes".into());
                    texts.push(format!("[{uri}: {kind} of {} base64 characters, not shown]", blob.len()));
                }
                _ => texts.push("[a resource of a kind not shown]".into()),
            },
            ContentBlock::ResourceLink(link) => texts.push(format!("[{}: {}]", link.name, link.uri)),
            _ => texts.push("[content of a kind not shown]".into()),
        }
    }
    // Its structured content, a client's to read, is the model's too where the server gave it no content.
    if texts.is_empty()
        && let Some(structured) = &result.structured_content
    {
        texts.push(structured.to_string());
    }
    let text = texts.join("\n");
    if result.is_error == Some(true) {
        return Err(Refusal(text));
    }
    let mut out = Vec::new();
    if !text.is_empty() || images.is_empty() {
        out.push(InputContent::text(text));
    }
    for bytes in images {
        out.extend(context.show_image(&bytes, None).await?);
    }
    Ok(Told {
        output: out,
        structured: result.structured_content,
        instructions: Vec::new(),
    })
}
