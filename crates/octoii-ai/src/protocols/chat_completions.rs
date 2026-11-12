//! Chat Completions, which Z.ai and the servers on the person's machine speak. A request goes as its messages: the
//! instructions and the updates before every other item one `system` message; each turn of the person's its call
//! results as `tool` messages, then a user message of the files they held, then the person's messages, its updates
//! inside `<system-reminder>` at the end of the last of them; and what the model wrote between two of the person's turns
//! as assistant messages of `reasoning_content`, text and `tool_calls`. Functions are its only tools. Its chunks come
//! back as the vocabulary's events.

use std::collections::BTreeMap;

use futures::StreamExt;
use futures::future::BoxFuture;
use serde_json::{Value, json};

use crate::catalogue::{Features, InputLimits, Model};
use crate::driver::{Call, Connection, Driver};
use crate::stream::{Event, Part, Source};
use crate::vocabulary::{
    Failure, FailureKind, FunctionCall, Incomplete, InputContent, Item, ItemStatus, Message, Origin, OutputContent, Reasoning,
    ReasoningEffort, Request, Response, ResponseStatus, Tool, ToolChoice, Usage,
};
use crate::wire::{self, FailureDetail};

/// How a provider asks its model to reason: by `reasoning_effort`, or by Z.ai's `thinking` beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Thinking {
    ReasoningEffort,
    Switched,
}

/// How a provider speaks Chat Completions: how it asks its model to reason, and whether it takes a tool choice but
/// `auto`.
#[derive(Debug, Clone, Copy)]
pub struct Dialect {
    pub thinking: Thinking,
    pub tool_choices: bool,
}

/// What joins the texts Chat Completions takes as one.
const JOINER: &str = "\n\n";

/// `request` as Chat Completions takes it for `model`, in `dialect`.
///
/// # Errors
/// An `invalid_request` failure where an update holds a file, or a file named is not the host's.
pub fn render(call: &Call, model: &Model, request: &Request, dialect: &Dialect) -> Result<Value, Failure> {
    let reference = model.reference();
    let mut effort = request.reasoning.map(|reasoning| reasoning.effort);
    let mut conversation = Conversation {
        call,
        reference: &reference,
        messages: Vec::new(),
        assistant: None,
        turn: None,
    };
    let mut instructions: Vec<String> = request.instructions.iter().cloned().collect();
    let mut leading = true;
    for item in &request.input {
        match item {
            Item::Message(Message::Developer { content } | Message::System { content }) if leading => instructions.push(texts_of(content)?),
            Item::ConfigurationUpdate(update) => effort = Some(update.effort),
            _ => {
                leading = false;
                conversation.add(item)?;
            }
        }
    }
    conversation.flush();
    let mut messages = Vec::new();
    if !instructions.is_empty() {
        messages.push(json!({ "role": "system", "content": instructions.join(JOINER) }));
    }
    messages.extend(conversation.messages);
    let mut body = json!({ "model": model.id, "messages": messages, "stream": true, "stream_options": { "include_usage": true } });
    let functions: Vec<Value> = request
        .tools
        .iter()
        .filter_map(|tool| match tool {
            Tool::Function {
                name,
                description,
                parameters,
            } => Some(json!({ "type": "function", "function": { "name": name, "description": description, "parameters": parameters } })),
            Tool::WebSearch => None,
        })
        .collect();
    if !functions.is_empty() {
        body["tools"] = Value::Array(functions);
        if let Some(choice) = &request.tool_choice {
            body["tool_choice"] = match (choice, dialect.tool_choices) {
                (_, false) | (ToolChoice::Auto, true) => json!("auto"),
                (ToolChoice::None, true) => json!("none"),
                (ToolChoice::Required, true) => json!("required"),
                (ToolChoice::Function { name }, true) => json!({ "type": "function", "function": { "name": name } }),
            };
        }
    }
    let reasons = model.reasoning.is_some() || effort.is_some_and(|effort| effort != ReasoningEffort::None);
    match dialect.thinking {
        Thinking::ReasoningEffort => {
            if let Some(effort) = effort {
                body["reasoning_effort"] = json!(effort);
            }
        }
        Thinking::Switched if reasons => {
            let enabled = effort != Some(ReasoningEffort::None);
            body["thinking"] = json!({ "type": if enabled { "enabled" } else { "disabled" } });
            if let Some(effort) = effort.filter(|_| enabled) {
                body["reasoning_effort"] = json!(effort);
            }
        }
        Thinking::Switched => {}
    }
    if let Some(format) = &request.format {
        body["response_format"] =
            json!({ "type": "json_schema", "json_schema": { "name": format.name, "schema": format.schema, "strict": true } });
    }
    if let Some(most) = request.max_output_tokens {
        body["max_tokens"] = json!(most);
    }
    Ok(body)
}

/// The text of an update, which takes text alone.
fn texts_of(content: &[InputContent]) -> Result<String, Failure> {
    content
        .iter()
        .map(|part| match part {
            InputContent::InputText { text } => Ok(text.as_str()),
            _ => Err(Failure::new(
                FailureKind::InvalidRequest,
                "An update takes text alone, and one holds a file",
            )),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|texts| texts.join(JOINER))
}

/// An assistant message as it is gathered: its reasoning, its text, its calls — each field in its order, one held
/// again or one before it beginning another message.
#[derive(Debug, Default)]
struct Assistant {
    reasoning: Option<String>,
    text: Option<String>,
    calls: Vec<Value>,
}

/// A turn of the person's as it is gathered: its call results, the files they held, its messages and its updates.
#[derive(Debug, Default)]
struct Turn {
    tools: Vec<Value>,
    files: Vec<Value>,
    users: Vec<Vec<Value>>,
    updates: Vec<String>,
}

struct Conversation<'a> {
    call: &'a Call,
    reference: &'a str,
    messages: Vec<Value>,
    assistant: Option<Assistant>,
    turn: Option<Turn>,
}

impl Conversation<'_> {
    fn add(&mut self, item: &Item) -> Result<(), Failure> {
        match item {
            Item::Message(Message::User { content }) => {
                let parts = self.parts(content)?;
                self.turn().users.push(parts);
            }
            Item::Message(Message::Developer { content } | Message::System { content }) => {
                let text = texts_of(content)?;
                self.turn().updates.push(text);
            }
            Item::Message(Message::Assistant { content, .. }) => {
                let text: String = content
                    .iter()
                    .map(|part| match part {
                        OutputContent::OutputText { text, .. } => text.as_str(),
                        OutputContent::Refusal { refusal } => refusal.as_str(),
                    })
                    .collect();
                let assistant = self.assistant(|held| held.text.is_some() || !held.calls.is_empty());
                assistant.text = Some(text);
            }
            Item::Reasoning(reasoning) if reasoning.origin.model == self.reference => {
                let text = [reasoning.content.concat(), reasoning.summary.concat()].concat();
                if !text.is_empty() {
                    let assistant = self.assistant(|held| held.reasoning.is_some() || held.text.is_some() || !held.calls.is_empty());
                    assistant.reasoning = Some(text);
                }
            }
            Item::FunctionCall(made) => {
                let assistant = self.assistant(|_| false);
                assistant.calls.push(
                    json!({ "id": made.call_id, "type": "function", "function": { "name": made.name, "arguments": made.arguments } }),
                );
            }
            Item::FunctionCallOutput(output) => {
                let mut text = Vec::new();
                for part in &output.output {
                    match part {
                        InputContent::InputText { text: words } => text.push(words.as_str()),
                        file => {
                            let parts = self.parts(std::slice::from_ref(file))?;
                            self.turn().files.extend(parts);
                        }
                    }
                }
                let message = json!({ "role": "tool", "tool_call_id": output.call_id, "content": text.join(JOINER) });
                self.turn().tools.push(message);
            }
            _ => {}
        }
        Ok(())
    }

    /// The assistant message gathered, a new one begun where `begins_another` says the one gathered holds what comes
    /// before.
    fn assistant(&mut self, begins_another: impl Fn(&Assistant) -> bool) -> &mut Assistant {
        self.flush_turn();
        if self.assistant.as_ref().is_some_and(begins_another) {
            self.flush_assistant();
        }
        self.assistant.get_or_insert_with(Assistant::default)
    }

    fn turn(&mut self) -> &mut Turn {
        self.flush_assistant();
        self.turn.get_or_insert_with(Turn::default)
    }

    fn flush(&mut self) {
        self.flush_assistant();
        self.flush_turn();
    }

    fn flush_assistant(&mut self) {
        let Some(assistant) = self.assistant.take() else { return };
        let mut message = json!({ "role": "assistant", "content": assistant.text.unwrap_or_default() });
        if let Some(reasoning) = assistant.reasoning {
            message["reasoning_content"] = json!(reasoning);
        }
        if !assistant.calls.is_empty() {
            message["tool_calls"] = Value::Array(assistant.calls);
        }
        self.messages.push(message);
    }

    /// A turn's messages: its call results, then a user message of the files they held, then the person's, its
    /// updates at the end of the last of them.
    fn flush_turn(&mut self) {
        let Some(mut turn) = self.turn.take() else { return };
        self.messages.append(&mut turn.tools);
        if !turn.files.is_empty() {
            self.messages.push(json!({ "role": "user", "content": turn.files }));
        }
        if !turn.updates.is_empty() {
            let tagged = turn
                .updates
                .iter()
                .map(|update| json!({ "type": "text", "text": super::system_reminder(update) }));
            match turn.users.last_mut() {
                Some(last) => last.extend(tagged),
                None => turn.users.push(tagged.collect()),
            }
        }
        for parts in turn.users {
            self.messages.push(json!({ "role": "user", "content": parts }));
        }
    }

    fn parts(&self, content: &[InputContent]) -> Result<Vec<Value>, Failure> {
        content
            .iter()
            .map(|part| {
                Ok(match part {
                    InputContent::InputText { text } => json!({ "type": "text", "text": text }),
                    InputContent::InputImage { file, .. } => {
                        json!({ "type": "image_url", "image_url": { "url": super::data_url(self.call, file)? } })
                    }
                    InputContent::InputFile { file } if super::is_text(&file.media_type) => {
                        json!({ "type": "text", "text": super::file_as_text(self.call, file)? })
                    }
                    InputContent::InputFile { file } => json!({
                        "type": "file",
                        "file": { "file_data": super::data_url(self.call, file)?, "filename": file.filename.as_deref().unwrap_or("file") },
                    }),
                })
            })
            .collect()
    }
}

/// A call as it streams: its place among the items, its id, its name, its arguments so far.
#[derive(Debug)]
struct Streaming {
    item: u32,
    call_id: String,
    name: String,
    arguments: String,
}

/// Chunks read into the vocabulary's events: the reasoning one item, the text another, each call its own, in the
/// order they began.
#[derive(Debug, Default)]
pub struct Reader {
    id: Option<String>,
    items: u32,
    reasoning: Option<(u32, String)>,
    text: Option<(u32, String)>,
    calls: BTreeMap<u64, Streaming>,
    finish: Option<String>,
    usage: Option<Usage>,
}

impl Reader {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn item(&mut self) -> u32 {
        self.items += 1;
        self.items - 1
    }

    /// The events one chunk stands for.
    ///
    /// # Errors
    /// The failure a chunk that is no JSON, or one of an error, stands for.
    pub fn chunk(&mut self, data: &str) -> Result<Vec<Event>, Failure> {
        let value: Value = serde_json::from_str(data).map_err(|_| {
            Failure::new(
                FailureKind::InvalidResponse,
                format!("The provider sent a chunk that is no JSON: {data}"),
            )
        })?;
        if value["error"].is_object() {
            return Err(failure_of(&value["error"]));
        }
        if self.id.is_none() {
            self.id = value["id"].as_str().map(str::to_owned);
        }
        if value["usage"].is_object() {
            self.usage = Some(usage_of(&value["usage"]));
        }
        let mut out = Vec::new();
        let Some(choice) = value["choices"].get(0) else { return Ok(out) };
        let delta = &choice["delta"];
        let thought = delta["reasoning_content"]
            .as_str()
            .or_else(|| delta["reasoning"].as_str())
            .unwrap_or_default();
        if !thought.is_empty() {
            let index = if let Some((index, text)) = &mut self.reasoning {
                text.push_str(thought);
                *index
            } else {
                let index = self.item();
                out.push(Event::Began {
                    index,
                    item: Item::Reasoning(reasoning(String::new())),
                });
                self.reasoning = Some((index, thought.to_owned()));
                index
            };
            out.push(Event::Delta {
                index,
                part: Part::Reasoning,
                text: thought.to_owned(),
            });
        }
        if let Some(words) = delta["content"].as_str().filter(|words| !words.is_empty()) {
            let index = if let Some((index, text)) = &mut self.text {
                text.push_str(words);
                *index
            } else {
                let index = self.item();
                out.push(Event::Began {
                    index,
                    item: message(String::new(), ItemStatus::InProgress),
                });
                self.text = Some((index, words.to_owned()));
                index
            };
            out.push(Event::Delta {
                index,
                part: Part::Text,
                text: words.to_owned(),
            });
        }
        for call in delta["tool_calls"].as_array().into_iter().flatten() {
            self.call_changed(call, &mut out);
        }
        if let Some(reason) = choice["finish_reason"].as_str() {
            self.finish = Some(reason.to_owned());
        }
        Ok(out)
    }

    fn call_changed(&mut self, call: &Value, out: &mut Vec<Event>) {
        let at = call["index"].as_u64().unwrap_or(0);
        if !self.calls.contains_key(&at) {
            let item = self.item();
            let made = Streaming {
                item,
                call_id: call["id"].as_str().unwrap_or_default().to_owned(),
                name: call["function"]["name"].as_str().unwrap_or_default().to_owned(),
                arguments: String::new(),
            };
            out.push(Event::Began {
                index: item,
                item: Item::FunctionCall(function_call(&made, ItemStatus::InProgress)),
            });
            self.calls.insert(at, made);
        }
        let Some(made) = self.calls.get_mut(&at) else { return };
        if let Some(name) = call["function"]["name"]
            .as_str()
            .filter(|name| made.name.is_empty() && !name.is_empty())
        {
            name.clone_into(&mut made.name);
        }
        if let Some(arguments) = call["function"]["arguments"].as_str().filter(|arguments| !arguments.is_empty()) {
            made.arguments.push_str(arguments);
            out.push(Event::Delta {
                index: made.item,
                part: Part::Arguments,
                text: arguments.to_owned(),
            });
        }
    }

    /// The end of the response: each item whole, then the response.
    #[must_use]
    pub fn end(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        if let Some((index, text)) = self.reasoning.take() {
            let mut made = reasoning(text);
            made.status = Some(ItemStatus::Completed);
            out.push(Event::Ended {
                index,
                item: Item::Reasoning(made),
            });
        }
        if let Some((index, text)) = self.text.take() {
            out.push(Event::Ended {
                index,
                item: message(text, ItemStatus::Completed),
            });
        }
        for made in std::mem::take(&mut self.calls).into_values() {
            out.push(Event::Ended {
                index: made.item,
                item: Item::FunctionCall(function_call(&made, ItemStatus::Completed)),
            });
        }
        let truncated = self.finish.as_deref() == Some("length");
        let filtered = self.finish.as_deref() == Some("content_filter");
        out.push(Event::Finished(Response {
            id: self.id.clone().unwrap_or_default(),
            model: String::new(),
            status: if truncated || filtered {
                ResponseStatus::Incomplete
            } else {
                ResponseStatus::Completed
            },
            output: Vec::new(),
            usage: self.usage,
            failure: None,
            incomplete: if truncated {
                Some(Incomplete::MaxOutputTokens)
            } else {
                filtered.then_some(Incomplete::ContentFilter)
            },
            context_tokens: self.usage.map(|usage| usage.input_tokens),
        }));
        out
    }
}

fn reasoning(text: String) -> Reasoning {
    Reasoning {
        id: None,
        summary: Vec::new(),
        content: if text.is_empty() { Vec::new() } else { vec![text] },
        encrypted_content: None,
        signature: None,
        status: Some(ItemStatus::InProgress),
        origin: Origin {
            model: String::new(),
            provider_metadata: None,
        },
    }
}

fn message(text: String, status: ItemStatus) -> Item {
    Item::Message(Message::Assistant {
        id: None,
        content: if text.is_empty() {
            Vec::new()
        } else {
            vec![OutputContent::OutputText {
                text,
                annotations: Vec::new(),
            }]
        },
        status: Some(status),
        origin: None,
    })
}

fn function_call(made: &Streaming, status: ItemStatus) -> FunctionCall {
    FunctionCall {
        id: None,
        call_id: made.call_id.clone(),
        name: made.name.clone(),
        arguments: made.arguments.clone(),
        status: Some(status),
        origin: None,
    }
}

fn usage_of(given: &Value) -> Usage {
    let count = |value: &Value| value.as_u64().unwrap_or(0);
    Usage {
        input_tokens: count(&given["prompt_tokens"]),
        cached_tokens: count(&given["prompt_tokens_details"]["cached_tokens"]),
        cache_write_tokens: 0,
        output_tokens: count(&given["completion_tokens"]),
        reasoning_tokens: count(&given["completion_tokens_details"]["reasoning_tokens"]),
        web_searches: 0,
        cost_nanodollars: None,
    }
}

fn detail_of(error: &Value) -> FailureDetail {
    let message = error["message"]
        .as_str()
        .unwrap_or("The provider failed without saying why")
        .to_owned();
    FailureDetail {
        kind: super::context_too_long(&message).then_some(FailureKind::ContextOverflow),
        message: Some(message),
        code: error["code"].as_str().or_else(|| error["type"].as_str()).map(str::to_owned),
        ..FailureDetail::default()
    }
}

fn failure_of(error: &Value) -> Failure {
    let detail = detail_of(error);
    let mut failure = Failure::new(detail.kind.unwrap_or(FailureKind::Server), detail.message.unwrap_or_default());
    failure.code = detail.code;
    failure
}

/// A driver of Chat Completions at `base_url`: reached with an API key where its connection says, else as a server of
/// the person's that takes none, and listing its models where it is one.
#[derive(Debug, Clone, Copy)]
pub struct ChatDriver {
    pub id: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    pub connection: Connection,
    pub dialect: Dialect,
}

impl Driver for ChatDriver {
    fn id(&self) -> &'static str {
        self.id
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn connection(&self) -> Connection {
        self.connection
    }

    fn stream<'a>(&'a self, call: &'a Call, model: &'a Model, request: &'a Request) -> BoxFuture<'a, Result<Source, Failure>> {
        Box::pin(async move {
            let body = render(call, model, request, &self.dialect)?;
            let mut made = http::Request::post(format!("{}/chat/completions", call.base_url(self.base_url)))
                .header("accept", "text/event-stream")
                .header("content-type", "application/json");
            if let Some(key) = call.api_key()? {
                made = made.header("authorization", format!("Bearer {key}"));
            } else if matches!(self.connection, Connection::ApiKey { .. }) {
                return Err(Failure::new(FailureKind::Auth, format!("No API key is set for {}", self.name)));
            }
            let request = made
                .body(bytes::Bytes::from(body.to_string()))
                .map_err(|error| Failure::new(FailureKind::InvalidRequest, error.to_string()))?;
            let response = wire::send(call, request).await?;
            if !response.status().is_success() {
                return Err(wire::failure_of(call, response, &|body: &str| {
                    serde_json::from_str::<Value>(body).ok().map(|value| detail_of(&value["error"]))
                })
                .await);
            }
            let source: Source = Box::pin(async_stream::stream! {
                let mut reader = Reader::new();
                let mut events = wire::events(response.into_body());
                while let Some(event) = events.next().await {
                    let data = match event {
                        Ok(event) => event.data,
                        Err(failure) => {
                            yield Err(failure);
                            return;
                        }
                    };
                    if data.trim() == "[DONE]" {
                        break;
                    }
                    match reader.chunk(&data) {
                        Ok(made) => {
                            for event in made {
                                yield Ok(event);
                            }
                        }
                        Err(failure) => {
                            yield Err(failure);
                            return;
                        }
                    }
                }
                for event in reader.end() {
                    yield Ok(event);
                }
            });
            Ok(source)
        })
    }

    /// A server of the person's lists the models it holds, by their ids; what each is, the server does not say.
    fn list<'a>(&'a self, call: &'a Call) -> BoxFuture<'a, Result<Option<Vec<Model>>, Failure>> {
        Box::pin(async move {
            if !matches!(self.connection, Connection::Server) {
                return Ok(None);
            }
            let request = http::Request::get(format!("{}/models", call.base_url(self.base_url)))
                .body(bytes::Bytes::new())
                .map_err(|error| Failure::new(FailureKind::InvalidRequest, error.to_string()))?;
            let response = wire::send(call, request).await?;
            let listed: Value = wire::json(call, response, &wire::nothing).await?;
            Ok(Some(
                listed["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| entry["id"].as_str())
                    .map(|id| Model {
                        id: id.to_owned(),
                        provider: call.provider.clone(),
                        name: id.to_owned(),
                        context_length: None,
                        max_output_tokens: None,
                        reads_images: false,
                        reads_files: false,
                        reasoning: None,
                        parameters: None,
                        pricing: None,
                        release_date: None,
                        searches_web: false,
                        limits: InputLimits::default(),
                        features: Features::default(),
                    })
                    .collect(),
            ))
        })
    }
}
