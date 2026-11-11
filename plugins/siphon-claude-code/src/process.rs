//! One `claude`, talked to over its stream-json control protocol as a coprocess: a line of JSON each way. What it is
//! asked — `initialize` first, then what a session asks of it — goes as a control request and comes back as its
//! response; what it asks back is answered here: its calls of the MCP server it was told is ours, the thread's tools,
//! answered as MCP's JSON-RPC — a tool's call held, never run, till its result is given, the model's call named by the
//! `claudecode/toolUseId` Claude Code puts on it. Everything else it writes is heard as it comes. One task owns it all;
//! as the handle goes, or the task is told to stop, its streams are let go of, which ends the program.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};
use siphon_machine::{Coprocess, Machine, Stderr, Variables};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader, Lines};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::{CancellationToken, DropGuard};

/// The name of the MCP server the thread's tools are on, as Claude Code is told it.
pub const SERVER: &str = "tools";
/// What Claude Code puts before the name of each tool of that server.
pub const PREFIX: &str = "mcp__tools__";
/// The key of the model's call on a call of a tool, as Claude Code writes it.
const TOOL_USE_ID: &str = "claudecode/toolUseId";

/// A `claude` started: its program, its arguments, the folder it works in, and its variables, whole.
#[derive(Debug, Clone)]
pub struct Launch {
    pub program: String,
    pub arguments: Vec<String>,
    pub cwd: std::path::PathBuf,
    pub variables: Vec<(String, String)>,
}

/// What a session hears of its `claude`: a message it wrote, or a call of a tool it now holds.
#[derive(Debug)]
pub enum Heard {
    Told(Told),
    Held(String),
}

/// A message `claude` writes, as far as a session reads it.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Told {
    System {
        subtype: String,
        #[serde(default)]
        session_id: Option<String>,
    },
    StreamEvent {
        event: Value,
        #[serde(default)]
        parent_tool_use_id: Option<String>,
    },
    Assistant {
        message: Value,
        uuid: String,
        #[serde(default)]
        parent_tool_use_id: Option<String>,
        #[serde(default)]
        error: Option<String>,
    },
    Result(Ended),
    RateLimitEvent {
        rate_limit_info: RateLimit,
    },
    #[serde(other)]
    Other,
}

/// How a turn ended, as its result tells: whether it failed, its words or errors, the JSON it ended with, the status
/// its API call failed with, and the window it measured each model against.
#[derive(Debug, Deserialize)]
pub struct Ended {
    pub subtype: String,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub structured_output: Option<Value>,
    #[serde(default)]
    pub usage: Option<Value>,
    #[serde(default, rename = "modelUsage")]
    pub model_usage: HashMap<String, ModelUsage>,
    #[serde(default)]
    pub api_error_status: Option<u16>,
}

/// What a turn used of a model, as far as it is read: the window it was measured against.
#[derive(Debug, Deserialize)]
pub struct ModelUsage {
    #[serde(default, rename = "contextWindow")]
    pub context_window: u64,
}

/// How the plan's limits stand, as Claude Code last told: refused or not, when the limit is given again, in seconds
/// since the epoch, and which limit it is.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RateLimit {
    #[serde(default)]
    pub status: String,
    #[serde(default, rename = "resetsAt")]
    pub resets_at: Option<i64>,
    #[serde(default, rename = "rateLimitType")]
    pub rate_limit_type: Option<String>,
}

/// What the task that owns a `claude` is given to do.
enum Order {
    Ask(Value, oneshot::Sender<Result<Value, String>>),
    Say(Vec<Value>),
    Give(String, Value),
}

/// A `claude`, as a handle to the task that owns it.
#[derive(Debug)]
pub struct Process {
    orders: mpsc::UnboundedSender<Order>,
    stderr: Stderr,
    _stop: DropGuard,
}

impl std::fmt::Debug for Order {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Ask(..) => "Ask",
            Self::Say(_) => "Say",
            Self::Give(..) => "Give",
        })
    }
}

impl Process {
    /// `launch` started on `machine` at `time`, its MCP server offering `tools`, and what it answered `initialize`:
    /// with what it is heard to write.
    ///
    /// # Errors
    /// Where it could not be started, or ended or failed before it answered `initialize`: why, with what it wrote to
    /// its errors.
    pub async fn start(
        machine: &Machine,
        time: i64,
        launch: Launch,
        tools: Vec<Value>,
        initialize: Value,
    ) -> Result<(Self, Value, mpsc::UnboundedReceiver<Heard>), Started> {
        let mut argv = vec![launch.program];
        argv.extend(launch.arguments);
        let Coprocess { stdin, stdout, stderr } = machine
            .coprocess(time, b"", argv, &launch.cwd, Variables::Only(launch.variables))
            .map_err(|error| match error {
                siphon_machine::RunError::MissingProgram(_) => Started::Missing,
                other => Started::Failed(other.to_string()),
            })?;
        let (orders, taken) = mpsc::unbounded_channel();
        let (heard, hearing) = mpsc::unbounded_channel();
        let stop = CancellationToken::new();
        let owner = Owner {
            stdin,
            tools,
            pending: HashMap::new(),
            held: HashMap::new(),
            given: HashMap::new(),
            next: 0,
            heard,
        };
        tokio::spawn(owner.run(BufReader::new(stdout).lines(), taken, stop.clone()));
        let process = Self {
            orders,
            stderr,
            _stop: stop.drop_guard(),
        };
        match process.ask(initialize).await {
            Ok(answer) => Ok((process, answer, hearing)),
            Err(why) => Err(Started::Failed(process.with_errors(&why))),
        }
    }

    /// What it answers the control request `request`.
    ///
    /// # Errors
    /// Its error, or that it ended before it answered.
    pub async fn ask(&self, request: Value) -> Result<Value, String> {
        let (answer, answered) = oneshot::channel();
        self.orders.send(Order::Ask(request, answer)).map_err(|_| ENDED.to_owned())?;
        answered.await.map_err(|_| ENDED.to_owned())?
    }

    /// Gives it `content` as a message of the person's.
    pub fn say(&self, content: Vec<Value>) {
        let _ = self.orders.send(Order::Say(content));
    }

    /// Gives the call of a tool `call` its result, an MCP tool's: to the call where it is held, else kept till it is
    /// made.
    pub fn give(&self, call: String, result: Value) {
        let _ = self.orders.send(Order::Give(call, result));
    }

    /// `why`, with what it wrote to its errors, where it wrote any.
    #[must_use]
    pub fn with_errors(&self, why: &str) -> String {
        let errors = self.stderr.kept().unwrap_or_default();
        let errors = String::from_utf8_lossy(&errors);
        if errors.trim().is_empty() {
            why.to_owned()
        } else {
            format!("{why}; it wrote:\n{}", errors.trim_end())
        }
    }
}

/// Why a `claude` did not start: there is none on the machine, or it failed, in its words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Started {
    Missing,
    Failed(String),
}

/// What a call to a process that ended is told.
const ENDED: &str = "Claude Code ended";

/// The task that owns a `claude`: its input, the tools its MCP server offers, its requests waiting for their answers,
/// its calls held by the model's call each is of, and results given before their calls were made.
struct Owner {
    stdin: siphon_machine::Stdin,
    tools: Vec<Value>,
    pending: HashMap<String, oneshot::Sender<Result<Value, String>>>,
    /// Each call held: the control request it came in, and its JSON-RPC id.
    held: HashMap<String, (String, Value)>,
    given: HashMap<String, Value>,
    next: u64,
    heard: mpsc::UnboundedSender<Heard>,
}

impl Owner {
    async fn run(
        mut self,
        mut stdout: Lines<BufReader<siphon_machine::Stdout>>,
        mut orders: mpsc::UnboundedReceiver<Order>,
        stop: CancellationToken,
    ) {
        loop {
            tokio::select! {
                () = stop.cancelled() => break,
                line = stdout.next_line() => {
                    let Ok(Some(line)) = line else { break };
                    self.read(&line).await;
                }
                Some(order) = orders.recv() => self.take(order).await,
            }
        }
        for (_, answer) in self.pending.drain() {
            let _ = answer.send(Err(ENDED.to_owned()));
        }
    }

    async fn write(&mut self, value: &Value) {
        let mut line = value.to_string();
        line.push('\n');
        let _ = self.stdin.write_all(line.as_bytes()).await;
    }

    async fn take(&mut self, order: Order) {
        match order {
            Order::Ask(request, answer) => {
                self.next += 1;
                let id = format!("octoii-{}", self.next);
                self.pending.insert(id.clone(), answer);
                self.write(&json!({ "type": "control_request", "request_id": id, "request": request }))
                    .await;
            }
            Order::Say(content) => {
                let message = json!({
                    "type": "user",
                    "session_id": "",
                    "message": { "role": "user", "content": content },
                    "parent_tool_use_id": null,
                    "client_composed": true,
                });
                self.write(&message).await;
            }
            Order::Give(call, result) => match self.held.remove(&call) {
                Some((request, id)) => {
                    self.answer_mcp(&request, &json!({ "jsonrpc": "2.0", "id": id, "result": result }))
                        .await;
                }
                None => {
                    self.given.insert(call, result);
                }
            },
        }
    }

    /// Reads a line it wrote: an answer to a request of ours, a request of its own, or a message heard.
    async fn read(&mut self, line: &str) {
        let Ok(value) = serde_json::from_str::<Value>(line) else { return };
        match value["type"].as_str() {
            Some("control_response") => {
                let response = &value["response"];
                let Some(answer) = response["request_id"].as_str().and_then(|id| self.pending.remove(id)) else {
                    return;
                };
                let answered = if response["subtype"] == "success" {
                    Ok(response["response"].clone())
                } else {
                    Err(response["error"].as_str().unwrap_or("Claude Code refused").to_owned())
                };
                let _ = answer.send(answered);
            }
            Some("control_request") => self.asked(&value).await,
            Some("control_cancel_request") => {}
            _ => {
                if let Ok(told) = serde_json::from_value::<Told>(value) {
                    let _ = self.heard.send(Heard::Told(told));
                }
            }
        }
    }

    /// Answers a request of its own: a message to our MCP server; anything else refused, as nothing else was offered.
    async fn asked(&mut self, value: &Value) {
        let request = value["request_id"].as_str().unwrap_or_default().to_owned();
        let asked = &value["request"];
        if asked["subtype"] != "mcp_message" || asked["server_name"] != SERVER {
            let refused = json!({
                "type": "control_response",
                "response": { "subtype": "error", "request_id": request, "error": "not offered" },
            });
            self.write(&refused).await;
            return;
        }
        let message = &asked["message"];
        let Some(id) = message.get("id").cloned() else {
            // A notification: told it was taken, as nothing answers one.
            self.answer_mcp(&request, &json!({ "jsonrpc": "2.0", "result": {}, "id": 0 })).await;
            return;
        };
        let result = match message["method"].as_str().unwrap_or_default() {
            "initialize" => json!({
                "protocolVersion": message["params"]["protocolVersion"],
                "capabilities": { "tools": {} },
                "serverInfo": { "name": SERVER, "version": env!("CARGO_PKG_VERSION") },
            }),
            "tools/list" => json!({ "tools": self.tools }),
            "ping" => json!({}),
            "tools/call" => {
                let Some(call) = message["params"]["_meta"][TOOL_USE_ID].as_str().map(str::to_owned) else {
                    let failed =
                        json!({ "content": [{ "type": "text", "text": "The call came with no id to hold it by." }], "isError": true });
                    self.answer_mcp(&request, &json!({ "jsonrpc": "2.0", "id": id, "result": failed }))
                        .await;
                    return;
                };
                if let Some(result) = self.given.remove(&call) {
                    self.answer_mcp(&request, &json!({ "jsonrpc": "2.0", "id": id, "result": result }))
                        .await;
                } else {
                    self.held.insert(call.clone(), (request, id));
                    let _ = self.heard.send(Heard::Held(call));
                }
                return;
            }
            method => {
                let error = json!({ "code": -32601, "message": format!("no method {method}") });
                self.answer_mcp(&request, &json!({ "jsonrpc": "2.0", "id": id, "error": error }))
                    .await;
                return;
            }
        };
        self.answer_mcp(&request, &json!({ "jsonrpc": "2.0", "id": id, "result": result }))
            .await;
    }

    /// Answers its control request `request` with the JSON-RPC message `message` of our MCP server's.
    async fn answer_mcp(&mut self, request: &str, message: &Value) {
        let response = json!({
            "type": "control_response",
            "response": { "subtype": "success", "request_id": request, "response": { "mcp_response": message } },
        });
        self.write(&response).await;
    }
}
