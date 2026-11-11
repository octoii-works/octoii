//! A live session of Claude Code — one `claude` — answering one request at a time: given the results of the calls it
//! holds and the words past where it stands, it streams the message it answers with, read into the vocabulary as
//! Anthropic's stream is; then, once the message ended, it settles where it stands — holding the calls the message
//! asked for, or done with its turn — and knows where the conversation ends in it, so a session closed later is
//! resumed there. A message that called a tool the thread never offered leaves the session to answer itself, which no
//! request follows: it is closed, and keeps no position.

use std::collections::{HashMap, HashSet};

use octoii_ai::protocols::anthropic::{Read, Reader};
use octoii_ai::stream::Event;
use octoii_ai::vocabulary::{Failure, FailureKind};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::conversation::{self, Told};
use crate::process::{self, Ended, Heard, PREFIX, Process, RateLimit};

/// What a request's answer is sent as, to the stream the thread reads.
pub type Events = mpsc::UnboundedSender<Result<Event, Failure>>;

/// A live session.
#[derive(Debug)]
pub struct Session {
    process: Process,
    heard: mpsc::UnboundedReceiver<Heard>,
    /// What it must be asked with to go on: its model, how it thinks, its tools.
    pub settings: String,
    /// Its id in Claude Code, as its first message tells.
    pub id: Option<String>,
    /// The hash of the conversation it stands at, as its last answer left it.
    pub position: Option<String>,
    pub used: Instant,
    /// The calls it holds, by the model's call each is of.
    held: HashSet<String>,
    /// The thread's tools, by name.
    tools: HashSet<String>,
    limit: RateLimit,
    closed: bool,
}

/// Where a session stands after an answer: settled, the entry of its transcript the answer ended at; or closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Settled {
    At(Option<String>),
    Closed,
}

impl Session {
    /// A session of `process`, heard through `heard`, asked with `settings`, offering the tools named `tools`.
    #[must_use]
    pub fn new(process: Process, heard: mpsc::UnboundedReceiver<Heard>, settings: String, tools: HashSet<String>) -> Self {
        Self {
            process,
            heard,
            settings,
            id: None,
            position: None,
            used: Instant::now(),
            held: HashSet::new(),
            tools,
            limit: RateLimit::default(),
            closed: false,
        }
    }

    /// Whether it may be asked again: it ended no other way.
    #[must_use]
    pub fn is_open(&self) -> bool {
        !self.closed
    }

    /// Gives it what it is asked: results to the calls it holds, the rest as a message of the person's.
    pub fn give(&mut self, told: Told) {
        self.used = Instant::now();
        if self.held.is_empty() {
            let content: Vec<Value> = told.results.into_iter().chain(told.rest).collect();
            if !content.is_empty() {
                self.process.say(content);
            }
            return;
        }
        for result in told.results {
            let call = result["tool_use_id"].as_str().unwrap_or_default().to_owned();
            self.held.remove(&call);
            self.process.give(call, call_result(&result));
        }
        if !told.rest.is_empty() {
            self.process.say(told.rest);
        }
    }

    /// What it is heard to write next, a turn's windows told to `measured`; none once it ended.
    async fn next(&mut self, measured: &mut HashMap<String, u64>) -> Option<Heard> {
        if self.closed {
            return None;
        }
        let heard = self.heard.recv().await;
        match &heard {
            None => self.closed = true,
            Some(Heard::Told(process::Told::Result(ended))) => {
                for (model, usage) in &ended.model_usage {
                    if usage.context_window > 0 {
                        measured.insert(model.clone(), usage.context_window);
                    }
                }
            }
            Some(Heard::Told(process::Told::System { subtype, session_id })) if subtype == "init" => {
                self.id.clone_from(session_id);
            }
            Some(Heard::Told(process::Told::RateLimitEvent { rate_limit_info })) => self.limit = rate_limit_info.clone(),
            Some(Heard::Held(call)) => {
                self.held.insert(call.clone());
            }
            Some(_) => {}
        }
        heard
    }

    /// Closes it: its `claude` let go of.
    pub fn close(&mut self) {
        self.closed = true;
        self.heard.close();
    }

    /// Streams its answer onto `events`: the message it writes, read as Anthropic's stream is, `end` the
    /// conversation's hash before it — none for an answer aside from it — where it stands once the message is in it
    /// kept as its position; the windows a turn measured told to `measured`. The answer, to settle; none where it
    /// closed. The stream ends as the caller lets go of `events`.
    pub async fn stream(&mut self, end: Option<&str>, events: &Events, measured: &mut HashMap<String, u64>) -> Option<Answer> {
        let mut answering = Answering::new(self.tools.clone());
        while !answering.ended {
            let Some(heard) = self.next(measured).await else {
                let _ = events.send(Err(Failure::new(FailureKind::Server, "Claude Code ended before it answered")));
                return None;
            };
            if let Err(failure) = answering.take(heard, &self.limit, events) {
                let _ = events.send(Err(failure));
                self.close();
                return None;
            }
            if events.is_closed() {
                self.close();
                return None;
            }
        }
        if answering.diverged {
            self.close();
            return None;
        }
        self.position = end.map(|end| conversation::next(end, &answering.message()));
        Some(Answer(answering))
    }

    /// Waits till it stands still after `answer` — holding the calls the message asked for, or done with its turn —
    /// the windows a turn measured told to `measured`.
    pub async fn settle(&mut self, answer: Answer, measured: &mut HashMap<String, u64>) -> Settled {
        let Answer(mut answering) = answer;
        loop {
            if answering.asked && !self.held.is_empty() {
                break;
            }
            match self.next(measured).await {
                None => return Settled::Closed,
                Some(Heard::Told(process::Told::Result(_))) => break,
                Some(Heard::Told(process::Told::Assistant {
                    message,
                    uuid,
                    parent_tool_use_id: None,
                    ..
                })) => {
                    answering.entry(&message, uuid);
                }
                Some(_) => {}
            }
        }
        Settled::At(answering.last_entry)
    }

    /// Answers aside with the JSON of a schema its turn ends with, as one message's text onto `events`.
    pub async fn answer_structured(&mut self, events: &Events, measured: &mut HashMap<String, u64>) {
        loop {
            let Some(heard) = self.next(measured).await else {
                let _ = events.send(Err(Failure::new(FailureKind::Server, "Claude Code ended without an answer")));
                return;
            };
            let Heard::Told(process::Told::Result(ended)) = heard else {
                continue;
            };
            if ended.subtype != "success" || ended.is_error {
                let _ = events.send(Err(result_failure(&ended, &self.limit)));
                return;
            }
            let text = ended.structured_output.unwrap_or(Value::Null).to_string();
            let usage = ended.usage.unwrap_or_else(|| json!({}));
            let mut reader = Reader::new();
            let made = [
                json!({ "type": "message_start", "message": { "id": "structured", "type": "message", "role": "assistant", "content": [], "usage": usage } }),
                json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "text", "text": "" } }),
                json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": text } }),
                json!({ "type": "content_block_stop", "index": 0 }),
                json!({ "type": "message_delta", "delta": { "stop_reason": "end_turn" }, "usage": usage }),
                json!({ "type": "message_stop" }),
            ];
            for event in made {
                for read in reader.read(&event.to_string()) {
                    if let Read::Event(event) = read {
                        let _ = events.send(Ok(event));
                    }
                }
            }
            return;
        }
    }
}

/// An answer whose message ended, for its session to settle after.
#[derive(Debug)]
pub struct Answer(Answering);

/// A block of the message as it streams in: its words, or its call, the input's JSON as it comes.
#[derive(Debug)]
enum Streamed {
    Text(String),
    Call { id: String, name: String, json: String },
    Other,
}

/// One answer as it streams: read into the vocabulary, the thread's tools by their own names; the message kept as the
/// thread will send it back, so the conversation is known by its hash once the message is in it.
#[derive(Debug)]
struct Answering {
    reader: Reader,
    tools: HashSet<String>,
    message_id: Option<String>,
    blocks: Vec<(u64, Streamed)>,
    last_entry: Option<String>,
    ended: bool,
    asked: bool,
    diverged: bool,
}

impl Answering {
    fn new(tools: HashSet<String>) -> Self {
        Self {
            reader: Reader::new(),
            tools,
            message_id: None,
            blocks: Vec::new(),
            last_entry: None,
            ended: false,
            asked: false,
            diverged: false,
        }
    }

    /// Takes what was heard into the answer, its events sent onto `events`.
    ///
    /// # Errors
    /// The failure the answer ends with.
    fn take(&mut self, heard: Heard, limit: &RateLimit, events: &Events) -> Result<(), Failure> {
        let Heard::Told(told) = heard else { return Ok(()) };
        match told {
            process::Told::StreamEvent {
                event,
                parent_tool_use_id: None,
            } => {
                let event = self.track(event);
                for read in self.reader.read(&event.to_string()) {
                    match read {
                        Read::Event(event) => {
                            if matches!(event, Event::Finished(_)) {
                                self.ended = true;
                            }
                            let _ = events.send(Ok(event));
                        }
                        Read::Failure(failure) => return Err(failure),
                        Read::Paused => {}
                    }
                }
            }
            process::Told::Assistant {
                message,
                uuid,
                parent_tool_use_id: None,
                error,
            } => {
                if let Some(error) = error {
                    return Err(assistant_failure(&error, &message, limit));
                }
                self.entry(&message, uuid);
            }
            process::Told::Result(ended) => return Err(result_failure(&ended, limit)),
            _ => {}
        }
        Ok(())
    }

    /// Notes the entry of the transcript a block of the message was written as: the last is where it is resumed.
    fn entry(&mut self, message: &Value, uuid: String) {
        if self.message_id.is_some() && message["id"].as_str() == self.message_id.as_deref() {
            self.last_entry = Some(uuid);
        }
    }

    /// Keeps what `event` adds to the message, and gives it as the thread reads it: its calls by their own names.
    fn track(&mut self, mut event: Value) -> Value {
        let index = event["index"].as_u64().unwrap_or(0);
        match event["type"].as_str().unwrap_or_default() {
            "message_start" => self.message_id = event["message"]["id"].as_str().map(str::to_owned),
            "content_block_start" => {
                let block = &mut event["content_block"];
                let streamed = match block["type"].as_str() {
                    Some("text") => Streamed::Text(block["text"].as_str().unwrap_or_default().to_owned()),
                    Some("tool_use") => {
                        let given = block["name"].as_str().unwrap_or_default();
                        let name = given.strip_prefix(PREFIX).unwrap_or(given).to_owned();
                        if !self.tools.contains(&name) {
                            self.diverged = true;
                        }
                        block["name"] = json!(name);
                        Streamed::Call {
                            id: block["id"].as_str().unwrap_or_default().to_owned(),
                            name,
                            json: String::new(),
                        }
                    }
                    _ => Streamed::Other,
                };
                self.blocks.push((index, streamed));
            }
            "content_block_delta" => {
                let delta = &event["delta"];
                match self.blocks.iter_mut().rev().find(|(at, _)| *at == index) {
                    Some((_, Streamed::Text(text))) => text.push_str(delta["text"].as_str().unwrap_or_default()),
                    Some((_, Streamed::Call { json, .. })) => json.push_str(delta["partial_json"].as_str().unwrap_or_default()),
                    _ => {}
                }
            }
            "message_delta" => self.asked = event["delta"]["stop_reason"] == "tool_use",
            _ => {}
        }
        event
    }

    /// The message as the thread will send it back: its words and its calls; its thinking left out.
    fn message(&self) -> Value {
        let content: Vec<Value> = self
            .blocks
            .iter()
            .filter_map(|(_, block)| match block {
                Streamed::Text(text) => Some(json!({ "type": "text", "text": text })),
                Streamed::Call { id, name, json } => {
                    let input = serde_json::from_str::<Value>(json)
                        .ok()
                        .filter(Value::is_object)
                        .unwrap_or_else(|| json!({}));
                    Some(json!({ "type": "tool_use", "id": id, "name": name, "input": input }))
                }
                Streamed::Other => None,
            })
            .collect();
        json!({ "role": "assistant", "content": content })
    }
}

/// A result block as an MCP tool's result: its words and images, and whether it failed.
fn call_result(block: &Value) -> Value {
    let content: Vec<Value> = match &block["content"] {
        Value::String(text) => vec![json!({ "type": "text", "text": text })],
        Value::Array(blocks) => blocks
            .iter()
            .map(|block| match block["type"].as_str() {
                Some("text") => json!({ "type": "text", "text": block["text"] }),
                Some("image") if block["source"]["type"] == "base64" => {
                    json!({ "type": "image", "data": block["source"]["data"], "mimeType": block["source"]["media_type"] })
                }
                _ => json!({ "type": "text", "text": block.to_string() }),
            })
            .collect(),
        _ => Vec::new(),
    };
    json!({ "content": content, "isError": block["is_error"] == true })
}

/// A failure of `kind` in `words`, a quota used up where the plan's limits refused it: given again at their reset.
fn failure(kind: FailureKind, words: String, limit: &RateLimit) -> Failure {
    if limit.status != "rejected" {
        return Failure::new(kind, words);
    }
    let mut failure = Failure::new(FailureKind::Quota, words);
    failure.reset_at_ms = limit.resets_at.map(|seconds| seconds * 1000);
    failure.window_minutes = match limit.rate_limit_type.as_deref() {
        Some("five_hour") => Some(300),
        Some(kind) if kind.starts_with("seven_day") => Some(10_080),
        _ => None,
    };
    failure
}

/// The failure an assistant message of Claude Code's carries, in its words.
fn assistant_failure(error: &str, message: &Value, limit: &RateLimit) -> Failure {
    let kind = match error {
        "authentication_failed" | "oauth_org_not_allowed" | "verification_required" | "cloud_credential_error" => FailureKind::Auth,
        "account_on_hold" | "billing_error" => FailureKind::Forbidden,
        "rate_limit" => FailureKind::RateLimit,
        "invalid_request" | "max_output_tokens" | "model_not_found" => FailureKind::InvalidRequest,
        _ => FailureKind::Server,
    };
    let words: Vec<&str> = message["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|block| block["text"].as_str())
        .collect();
    let words = if words.is_empty() {
        format!("Claude Code failed: {error}")
    } else {
        words.join("\n")
    };
    failure(kind, words, limit)
}

/// The failure a result that ended a turn short stands for, by the status its API call failed with.
fn result_failure(ended: &Ended, limit: &RateLimit) -> Failure {
    let kind = match ended.api_error_status {
        Some(401) => FailureKind::Auth,
        Some(403) => FailureKind::Forbidden,
        Some(429) => FailureKind::RateLimit,
        Some(400 | 404 | 413) => FailureKind::InvalidRequest,
        _ => FailureKind::Server,
    };
    let words = if ended.subtype == "success" {
        ended
            .result
            .clone()
            .filter(|words| !words.is_empty())
            .unwrap_or_else(|| "Claude Code ended its turn without an answer".to_owned())
    } else if ended.errors.is_empty() {
        format!("Claude Code ended its turn: {}", ended.subtype)
    } else {
        ended.errors.join("; ")
    };
    failure(kind, words, limit)
}
