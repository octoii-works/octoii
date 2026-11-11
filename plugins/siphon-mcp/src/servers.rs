//! The MCP servers a thread's tools reach: the person's own, as the Octoii Home's `mcp.json` names them; then those its
//! project's `.mcp.json` names, once the person trusts them as they are — one of a name the person's has standing for
//! theirs, as the nearer setting holds. A server is connected as it is first needed, then kept and shared by every
//! thread, and stopped once nothing used it for `IDLE`, to be connected again as something next does; one that ended
//! is connected anew. A command's server runs on the machine, through the namespace, as the person's own programs do
//! but for what the host keeps for itself, recorded in the volume under `siphon.servers`, its record removed as it
//! stops. A tool of one is named `<server>.<tool>`, as a mount table names what is under a mount point.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use beak::{Exec, Snapshot, Spec, Stdio};
use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock, ResourceContents};
use rmcp::service::{Peer, RoleClient, RunningService, ServiceExt};
use rmcp::transport::{StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig};
use siphon_machine::paths::Base;
use siphon_machine::{Host, Machine, Writes};
use siphon_runtime::{Changes, Runtime};
use tokio::sync::{mpsc, oneshot};

use crate::config::{Configured, Launch, OWN_FILE, PROJECT_FILE, Server, parse};
use crate::stdio;
use crate::trust::{self, ShownServer, Trust};
use siphon_tools::projects::Place;
use siphon_tools::{Context, Declared, Refusal};

/// How long a server nothing uses keeps running before it is stopped.
pub const IDLE: Duration = Duration::from_secs(600);
/// How long a server may take to be connected.
const CONNECTING_MAX: Duration = Duration::from_secs(30);
/// The directory the servers run as commands are recorded under.
const RECORDS: &[u8] = b"siphon.servers";
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

/// The servers, as a handle to the task that holds them.
#[derive(Debug, Clone)]
pub struct Servers {
    orders: mpsc::UnboundedSender<Order>,
    own: Option<PathBuf>,
}

/// A server's peer asked of the task that holds the servers, by work on the runtime given, which records it.
struct Order(Server, Runtime, oneshot::Sender<Result<Peer<RoleClient>, String>>);

impl Servers {
    /// The servers run on the machine `host` gives, recorded in the volume of the runtime that first needs each; the
    /// person's own named in the Octoii Home `home`, where there is one. The task that holds them ends as the last
    /// handle goes, stopping them.
    #[must_use]
    pub fn start(host: Host, home: Option<&Path>) -> Self {
        let (orders, taken) = mpsc::unbounded_channel();
        tokio::spawn(hold(host, taken));
        Self {
            orders,
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
    pub async fn call(&self, context: &Context, name: &str, arguments: serde_json::Value) -> Result<Vec<InputContent>, Refusal> {
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

    /// The peer of `server`, connected now where it is not, recorded in `runtime`'s volume.
    async fn peer(&self, server: &Server, runtime: &Runtime) -> Result<Peer<RoleClient>, String> {
        let (reply, peer) = oneshot::channel();
        self.orders
            .send(Order(server.clone(), runtime.clone(), reply))
            .map_err(|_| "the servers are stopped".to_owned())?;
        peer.await.map_err(|_| "the servers are stopped".to_owned())?
    }
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

/// A server connected: its service, its record where it runs as a command, in the runtime's volume, and when it was
/// last used.
struct Running {
    service: RunningService<RoleClient, ()>,
    record: Option<(Runtime, Bytes)>,
    used: Instant,
}

enum Slot {
    Connecting(Vec<oneshot::Sender<Result<Peer<RoleClient>, String>>>),
    Running(Running),
}

type Connected = (Server, Result<(RunningService<RoleClient, ()>, Option<(Runtime, Bytes)>), String>);

/// Holds the servers: connects each as it is first asked for, its askers waiting; gives its peer to each that asks
/// after; stops each nothing used for `IDLE`.
async fn hold(host: Host, mut orders: mpsc::UnboundedReceiver<Order>) {
    let mut slots: HashMap<Server, Slot> = HashMap::new();
    let (done, mut connected) = mpsc::unbounded_channel::<Connected>();
    let mut ticks = tokio::time::interval(IDLE / 10);
    loop {
        tokio::select! {
            order = orders.recv() => {
                let Some(Order(server, runtime, reply)) = order else { break };
                match slots.get_mut(&server) {
                    Some(Slot::Running(running)) if !running.service.peer().is_transport_closed() => {
                        running.used = Instant::now();
                        let _ = reply.send(Ok(running.service.peer().clone()));
                    }
                    Some(Slot::Connecting(waiting)) => waiting.push(reply),
                    _ => {
                        if let Some(Slot::Running(ended)) = slots.insert(server.clone(), Slot::Connecting(vec![reply])) {
                            stop(ended);
                        }
                        let (host, done) = (host.clone(), done.clone());
                        tokio::spawn(async move {
                            let result = connect(&runtime, &host, &server).await;
                            let _ = done.send((server, result));
                        });
                    }
                }
            }
            Some((server, result)) = connected.recv() => {
                let Some(Slot::Connecting(waiting)) = slots.remove(&server) else { continue };
                match result {
                    Ok((service, record)) => {
                        for reply in waiting {
                            let _ = reply.send(Ok(service.peer().clone()));
                        }
                        slots.insert(server, Slot::Running(Running { service, record, used: Instant::now() }));
                    }
                    Err(why) => {
                        for reply in waiting {
                            let _ = reply.send(Err(why.clone()));
                        }
                    }
                }
            }
            _ = ticks.tick() => {
                let idle: Vec<Server> = slots
                    .iter()
                    .filter(|(_, slot)| matches!(slot, Slot::Running(running) if running.used.elapsed() >= IDLE))
                    .map(|(server, _)| server.clone())
                    .collect();
                for server in idle {
                    if let Some(Slot::Running(running)) = slots.remove(&server) {
                        stop(running);
                    }
                }
            }
        }
    }
    for (_, slot) in slots.drain() {
        if let Slot::Running(running) = slot {
            stop(running);
        }
    }
}

/// Stops `running`: its service ended, and its command's record removed, which kills what of it is left.
fn stop(running: Running) {
    drop(running.service);
    if let Some((runtime, record)) = running.record {
        tokio::spawn(remove(runtime, record));
    }
}

/// `server` connected: its service, and its record where it runs as a command; else why it could not be.
async fn connect(
    runtime: &Runtime,
    host: &Host,
    server: &Server,
) -> Result<(RunningService<RoleClient, ()>, Option<(Runtime, Bytes)>), String> {
    match &server.launch {
        Launch::Command { command, args, env, cwd } => {
            let record = beak::layout::child(
                RECORDS,
                b"server",
                runtime
                    .kernel()
                    .new_id(runtime.now_ms())
                    .map_err(|error| error.to_string())?
                    .as_bytes(),
            );
            let mut changes = Changes::new();
            changes.mkdir(Bytes::from_static(RECORDS)).mkdir(record.clone());
            runtime.commit(changes).await.map_err(|error| error.to_string())?;
            let base = Base {
                workspace: cwd.clone(),
                home: host.home.clone(),
            };
            let machine = Machine::open(runtime.kernel(), host, base, &Writes::Anywhere, None).map_err(|error| error.to_string())?;
            let mut argv = vec![command.clone()];
            argv.extend(args.iter().cloned());
            let spec = Spec {
                set: env.clone(),
                stdio: Stdio::Coprocess,
                ..Spec::new(Exec::Argv(argv), cwd.to_string_lossy())
            };
            let started = runtime.kernel().spawn(runtime.now_ms(), &record, machine.namespace(), &spec);
            let (process, events) = match started {
                Ok(started) => started,
                Err(error) => {
                    remove(runtime.clone(), record).await;
                    return Err(if error.status == beak::Status::MissingProgram {
                        format!("no program {command} on the machine")
                    } else {
                        error.to_string()
                    });
                }
            };
            let process = Arc::new(process);
            let serving = ().serve((stdio::output(events), stdio::Input(process.clone())));
            match tokio::time::timeout(CONNECTING_MAX, serving).await {
                Ok(Ok(service)) => Ok((service, Some((runtime.clone(), record)))),
                failed => {
                    let why = match failed {
                        Ok(Err(error)) => error.to_string(),
                        _ => format!("it did not answer within {CONNECTING_MAX:?}"),
                    };
                    let errors = process.output().unwrap_or_default();
                    remove(runtime.clone(), record).await;
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
                Ok(Ok(service)) => Ok((service, None)),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Err(format!("it did not answer within {CONNECTING_MAX:?}")),
            }
        }
    }
}

/// Removes the record of a server's command, which kills what of it is left.
async fn remove(runtime: Runtime, record: Bytes) {
    let mut changes = Changes::new();
    changes.force_remove(record, None);
    let _ = runtime.commit(changes).await;
}

/// What a server's tool gave, as the model reads it: its text, each block on a line of its own; of what is no text, an
/// image shown, the rest noted; its structured content where it gave no text; refused where it says it failed.
async fn given(context: &Context, result: CallToolResult) -> Result<Vec<InputContent>, Refusal> {
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
    if texts.is_empty()
        && let Some(structured) = result.structured_content
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
    Ok(out)
}
