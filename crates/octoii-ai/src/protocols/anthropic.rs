//! Anthropic's Messages API. A request's instructions and the updates before every other item make its system; the
//! person's items make user turns, a call's results first; what the model wrote makes assistant turns, its own blocks
//! sent back as they came; an update waits for the end of the person's turn, then stands as a system message, or as a
//! `<system-update>` in the person's turn where the model takes no system message or the turn holds nothing of the
//! person's. An effort changed where it stands goes there where the model takes it, else is folded into the request's.
//! The stream's content blocks come back as items — text blocks one message, each thinking block reasoning, each
//! `tool_use` a call, each search with its results a web search — each keeping the blocks it came as where the model
//! needs them back; a paused answer is asked to go on, its items going on and its usage adding up.

use std::collections::HashMap;

use futures::StreamExt;
use futures::future::BoxFuture;
use serde_json::{Value, json};

use crate::catalogue::Model;
use crate::driver::{Call, Connection, Driver};
use crate::stream::{Event, Part, Source};
use crate::vocabulary::{
    Failure, FailureKind, FunctionCall, Incomplete, InputContent, Item, ItemStatus, Message, Origin, OutputContent, Reasoning,
    ReasoningEffort, Request, Response, ResponseStatus, Tool, ToolChoice, UrlCitation, Usage, WebSearchAction, WebSearchCall,
    WebSearchStatus,
};
use crate::wire::{self, Said};

/// The version of the API the requests are written for.
pub const API_VERSION: &str = "2026-10-21";
/// The beta that takes an effort changed where it stands.
pub const MESSAGE_EFFORT_BETA: &str = "mid-conversation-output-config-2026-07-01";

/// The request as Anthropic takes it for `model`, and the betas it needs.
///
/// # Errors
/// An `invalid_request` failure for what Anthropic refuses: a call forced while the model reasons, no limit on the
/// answer where the model gives none, a file in an update, a file the host does not have.
pub fn render(call: &Call, model: &Model, request: &Request) -> Result<(Value, Vec<&'static str>), Failure> {
    let reference = model.reference();
    let refused = |words: String| Failure::new(FailureKind::InvalidRequest, words);
    let mut effort = request.reasoning.map(|reasoning| reasoning.effort);
    let updates: Vec<ReasoningEffort> = request
        .input
        .iter()
        .filter_map(|item| match item {
            Item::ConfigurationUpdate(update) => Some(update.effort),
            _ => None,
        })
        .collect();
    let reasons = model.reasoning.is_some() && effort != Some(ReasoningEffort::None);
    let in_place = model.features.effort_updates == Some(true) && reasons && !updates.contains(&ReasoningEffort::None);
    if !in_place && let Some(last) = updates.last() {
        effort = Some(*last);
    }
    let reasons = model.reasoning.is_some() && effort != Some(ReasoningEffort::None);
    if reasons && matches!(request.tool_choice, Some(ToolChoice::Required | ToolChoice::Function { .. })) {
        return Err(refused(format!(
            "The model {reference} reasons, and Anthropic refuses a request that forces a call while the model reasons"
        )));
    }
    let max_tokens = request.max_output_tokens.or(model.max_output_tokens).ok_or_else(|| {
        refused(format!(
            "The request gives no limit on its answer and the model {reference} has none: Anthropic needs one"
        ))
    })?;
    let mut conversation = Conversation {
        call,
        reference: &reference,
        takes_system: model.features.instruction_updates != Some(false),
        in_place,
        system: Vec::new(),
        turns: Vec::new(),
        texts: Vec::new(),
        effort: None,
        user_in_turn: false,
    };
    conversation.build(request)?;
    let messages = conversation.messages();
    let mut body = json!({
        "model": model.id,
        "max_tokens": max_tokens,
        "messages": messages,
        "cache_control": { "type": "ephemeral" },
        "stream": true,
    });
    if !conversation.system.is_empty() {
        body["system"] = Value::Array(conversation.system);
    }
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(request.tools.iter().map(rendered_tool).collect());
        if let Some(choice) = &request.tool_choice {
            body["tool_choice"] = match choice {
                ToolChoice::Auto => json!({ "type": "auto" }),
                ToolChoice::None => json!({ "type": "none" }),
                ToolChoice::Required => json!({ "type": "any" }),
                ToolChoice::Function { name } => json!({ "type": "tool", "name": name }),
            };
        }
    }
    if model.reasoning.is_some() {
        match effort {
            Some(ReasoningEffort::None) => body["thinking"] = json!({ "type": "disabled" }),
            Some(_) => body["thinking"] = json!({ "type": "adaptive", "display": "summarized" }),
            None => {}
        }
    }
    let mut output = serde_json::Map::new();
    if let Some(effort) = effort.filter(|effort| *effort != ReasoningEffort::None) {
        output.insert("effort".into(), json!(effort));
    }
    if let Some(format) = &request.format {
        output.insert("format".into(), json!({ "type": "json_schema", "schema": format.schema }));
    }
    if !output.is_empty() {
        body["output_config"] = Value::Object(output);
    }
    let betas = if in_place && !updates.is_empty() {
        vec![MESSAGE_EFFORT_BETA]
    } else {
        Vec::new()
    };
    Ok((body, betas))
}

fn rendered_tool(tool: &Tool) -> Value {
    match tool {
        Tool::Function {
            name,
            description,
            parameters,
        } => {
            let schema = if parameters.is_null() {
                json!({ "type": "object", "properties": {} })
            } else {
                parameters.clone()
            };
            json!({ "name": name, "description": description, "input_schema": schema })
        }
        Tool::WebSearch => json!({ "type": "web_search_20250305", "name": "web_search" }),
    }
}

/// A call's id as Anthropic takes one: each character it refuses `_`.
fn wire_call_id(call_id: &str) -> String {
    call_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// The blocks an item came as, where Anthropic's metadata on it keeps them.
fn own_blocks(origin: Option<&Origin>) -> Option<&Vec<Value>> {
    origin?.provider_metadata.as_ref()?.get("anthropic")?.get("blocks")?.as_array()
}

#[derive(Debug)]
struct Turn {
    role: &'static str,
    content: Vec<Value>,
    output_config: Option<Value>,
}

/// The conversation as it is built, item by item, the updates held till the person's turn ends.
struct Conversation<'a> {
    call: &'a Call,
    reference: &'a str,
    /// Whether the model takes a system message where it stands.
    takes_system: bool,
    /// Whether an effort changed goes where it stands.
    in_place: bool,
    system: Vec<Value>,
    turns: Vec<Turn>,
    texts: Vec<String>,
    effort: Option<ReasoningEffort>,
    /// Whether the turn since the model last wrote holds something of the person's.
    user_in_turn: bool,
}

impl Conversation<'_> {
    fn build(&mut self, request: &Request) -> Result<(), Failure> {
        if let Some(instructions) = request.instructions.as_deref().filter(|text| !text.is_empty()) {
            self.system.push(text_block(instructions));
        }
        let mut leading = true;
        for item in &request.input {
            match item {
                Item::Message(Message::Developer { content } | Message::System { content }) if leading => {
                    for text in update_texts(content)? {
                        self.system.push(text_block(&text));
                    }
                }
                _ => {
                    leading = false;
                    self.add(item)?;
                }
            }
        }
        Ok(())
    }

    fn add(&mut self, item: &Item) -> Result<(), Failure> {
        match item {
            Item::Message(Message::User { content }) => {
                let blocks = input_blocks(self.call, content)?;
                self.user(blocks);
            }
            Item::Message(Message::Developer { content } | Message::System { content }) => self.texts.extend(update_texts(content)?),
            Item::Message(Message::Assistant { content, origin, .. }) => {
                let blocks = assistant_blocks(content, origin.as_ref());
                self.assistant(blocks);
            }
            Item::FunctionCall(made) => {
                let input = serde_json::from_str::<Value>(&made.arguments)
                    .ok()
                    .filter(Value::is_object)
                    .unwrap_or_else(|| json!({}));
                self.assistant(vec![
                    json!({ "type": "tool_use", "id": wire_call_id(&made.call_id), "name": made.name, "input": input }),
                ]);
            }
            Item::FunctionCallOutput(output) => {
                let content = match output.output.as_slice() {
                    [InputContent::InputText { text }] => json!(text),
                    parts => Value::Array(input_blocks(self.call, parts)?),
                };
                let mut result = json!({ "type": "tool_result", "tool_use_id": wire_call_id(&output.call_id), "content": content });
                if output.is_error {
                    result["is_error"] = json!(true);
                }
                self.user(vec![result]);
            }
            Item::Reasoning(reasoning) if reasoning.origin.model == self.reference => {
                if let Some(signature) = reasoning.signature.as_ref().filter(|signature| !signature.is_empty()) {
                    self.assistant(vec![
                        json!({ "type": "thinking", "thinking": reasoning.summary.concat(), "signature": signature }),
                    ]);
                } else if let Some(data) = reasoning.encrypted_content.as_ref().filter(|data| !data.is_empty()) {
                    self.assistant(vec![json!({ "type": "redacted_thinking", "data": data })]);
                }
            }
            Item::WebSearchCall(search) if search.origin.as_ref().is_some_and(|origin| origin.model == self.reference) => {
                let blocks = own_blocks(search.origin.as_ref()).cloned().unwrap_or_default();
                self.assistant(blocks);
            }
            Item::ConfigurationUpdate(update) if self.in_place => self.effort = Some(update.effort),
            _ => {}
        }
        Ok(())
    }

    fn user(&mut self, blocks: Vec<Value>) {
        if blocks.is_empty() {
            return;
        }
        self.user_in_turn = true;
        self.into("user", blocks);
    }

    fn assistant(&mut self, blocks: Vec<Value>) {
        if blocks.is_empty() {
            return;
        }
        self.flush();
        self.user_in_turn = false;
        self.into("assistant", blocks);
    }

    fn into(&mut self, role: &'static str, blocks: Vec<Value>) {
        match self.turns.last_mut() {
            Some(turn) if turn.role == role => turn.content.extend(blocks),
            _ => self.turns.push(Turn {
                role,
                content: blocks,
                output_config: None,
            }),
        }
    }

    /// Places the updates waiting: after the person's turn as a system message where the model takes one, else as
    /// `<system-update>` text in the person's turn; an effort changed where it stands.
    fn flush(&mut self) {
        let texts = std::mem::take(&mut self.texts);
        let effort = self.effort.take();
        let as_system = self.takes_system && self.user_in_turn;
        if !as_system {
            let tagged = texts.iter().map(|text| text_block(&super::system_update(text))).collect();
            self.user(tagged);
        }
        let content: Vec<Value> = if as_system {
            texts.iter().map(|text| text_block(text)).collect()
        } else {
            Vec::new()
        };
        if !content.is_empty() || effort.is_some() {
            self.turns.push(Turn {
                role: "system",
                content,
                output_config: effort.map(|effort| json!({ "effort": effort })),
            });
        }
    }

    /// The messages, the updates still waiting placed at the end, a person's call results first in their turn.
    fn messages(&mut self) -> Vec<Value> {
        self.flush();
        self.turns
            .iter()
            .map(|turn| {
                let content: Vec<&Value> = if turn.role == "user" {
                    let (results, rest): (Vec<&Value>, Vec<&Value>) = turn.content.iter().partition(|block| block["type"] == "tool_result");
                    results.into_iter().chain(rest).collect()
                } else {
                    turn.content.iter().collect()
                };
                let mut message = json!({ "role": turn.role, "content": content });
                if let Some(config) = &turn.output_config {
                    message["output_config"] = config.clone();
                }
                message
            })
            .collect()
    }
}

fn text_block(text: &str) -> Value {
    json!({ "type": "text", "text": text })
}

/// The texts of an update: a file fails, as Anthropic takes none in a system message.
fn update_texts(content: &[InputContent]) -> Result<Vec<String>, Failure> {
    content
        .iter()
        .filter_map(|part| match part {
            InputContent::InputText { text } if text.is_empty() => None,
            InputContent::InputText { text } => Some(Ok(text.clone())),
            _ => Some(Err(Failure::new(
                FailureKind::InvalidRequest,
                "Anthropic takes no file in a developer or system message",
            ))),
        })
        .collect()
}

fn input_blocks(call: &Call, content: &[InputContent]) -> Result<Vec<Value>, Failure> {
    let mut blocks = Vec::new();
    for part in content {
        match part {
            InputContent::InputText { text } if text.is_empty() => {}
            InputContent::InputText { text } => blocks.push(text_block(text)),
            InputContent::InputImage { file, .. } => {
                let data = base64_of(call, &file.reference)?;
                blocks.push(json!({ "type": "image", "source": { "type": "base64", "media_type": file.media_type, "data": data } }));
            }
            InputContent::InputFile { file } => {
                let mut document = if super::is_text(&file.media_type) {
                    let bytes = call
                        .files
                        .read(&file.reference)
                        .map_err(|missing| Failure::new(FailureKind::InvalidRequest, missing.to_string()))?;
                    json!({ "type": "document", "source": { "type": "text", "media_type": "text/plain", "data": String::from_utf8_lossy(&bytes) } })
                } else {
                    json!({ "type": "document", "source": { "type": "base64", "media_type": file.media_type, "data": base64_of(call, &file.reference)? } })
                };
                if let Some(name) = &file.filename {
                    document["title"] = json!(name);
                }
                blocks.push(document);
            }
        }
    }
    Ok(blocks)
}

fn base64_of(call: &Call, reference: &str) -> Result<String, Failure> {
    use base64::Engine;
    let bytes = call
        .files
        .read(reference)
        .map_err(|missing| Failure::new(FailureKind::InvalidRequest, missing.to_string()))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// The blocks of an assistant message: those it came as where they still say its text, else its text and refusals.
fn assistant_blocks(content: &[OutputContent], origin: Option<&Origin>) -> Vec<Value> {
    let written: String = content
        .iter()
        .map(|part| match part {
            OutputContent::OutputText { text, .. } => text.as_str(),
            OutputContent::Refusal { refusal } => refusal.as_str(),
        })
        .collect();
    if let Some(own) = own_blocks(origin) {
        let said: String = own.iter().filter_map(|block| block["text"].as_str()).collect();
        if said == written {
            return own.clone();
        }
    }
    if written.is_empty() {
        Vec::new()
    } else {
        vec![text_block(&written)]
    }
}

/// How a piece of the answer ended: not yet, paused for the model to go on, or whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    Open,
    Paused,
    Ended,
}

/// What one of the stream's events comes to.
#[derive(Debug)]
pub enum Read {
    Event(Event),
    Failure(Failure),
    /// The answer paused, for the model to go on from the blocks so far.
    Paused,
}

/// A block of the piece under way: the JSON it came as, made whole as it streams, and the item it makes.
#[derive(Debug)]
struct Block {
    raw: Value,
    item: Option<u32>,
    text: String,
}

/// What a request used, as Anthropic counts it.
#[derive(Debug, Clone, Copy, Default)]
struct Counts {
    input: u64,
    output: u64,
    written: u64,
    read: u64,
    searches: u64,
}

impl Counts {
    fn with(self, given: &Value) -> Self {
        let count = |name: &str, otherwise: u64| given[name].as_u64().unwrap_or(otherwise);
        Self {
            input: count("input_tokens", self.input),
            output: count("output_tokens", self.output),
            written: count("cache_creation_input_tokens", self.written),
            read: count("cache_read_input_tokens", self.read),
            searches: given["server_tool_use"]["web_search_requests"].as_u64().unwrap_or(self.searches),
        }
    }

    fn plus(self, other: Self) -> Self {
        Self {
            input: self.input + other.input,
            output: self.output + other.output,
            written: self.written + other.written,
            read: self.read + other.read,
            searches: self.searches + other.searches,
        }
    }

    fn usage(self) -> Usage {
        Usage {
            input_tokens: self.input + self.written + self.read,
            cached_tokens: self.read,
            cache_write_tokens: self.written,
            output_tokens: self.output,
            reasoning_tokens: 0,
            web_searches: self.searches,
            cost_nanodollars: None,
        }
    }
}

/// The message the text blocks make while they come one after another.
#[derive(Debug)]
struct OpenMessage {
    item: u32,
    blocks: Vec<usize>,
}

/// Anthropic's stream read into the vocabulary's events.
#[derive(Debug, Default)]
pub struct Reader {
    items: u32,
    /// The blocks of the piece under way, by their index in it, to the place they hold among `sent`.
    blocks: HashMap<u64, usize>,
    /// Every block of the answer so far, as it came, made whole, which a paused answer sends back.
    sent: Vec<Block>,
    message: Option<OpenMessage>,
    total: Counts,
    piece: Counts,
    id: Option<String>,
    stop_reason: Option<String>,
    ending: Option<Ending>,
}

impl Reader {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The blocks of the answer so far, as the model is sent them back to go on.
    #[must_use]
    pub fn sent(&self) -> Vec<Value> {
        self.sent.iter().map(|block| block.raw.clone()).collect()
    }

    /// Begins the next piece of a paused answer.
    pub fn next_piece(&mut self) {
        self.total = self.total.plus(self.piece);
        self.piece = Counts::default();
        self.blocks.clear();
        self.stop_reason = None;
        self.ending = Some(Ending::Open);
    }

    fn item(&mut self) -> u32 {
        self.items += 1;
        self.items - 1
    }

    /// What one of the stream's events comes to.
    pub fn read(&mut self, data: &str) -> Vec<Read> {
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return vec![Read::Failure(Failure::new(
                FailureKind::InvalidResponse,
                format!("The provider sent an event that is no JSON: {data}"),
            ))];
        };
        let index = value["index"].as_u64().unwrap_or(0);
        let mut out = Vec::new();
        match value["type"].as_str().unwrap_or_default() {
            "message_start" => {
                self.piece = Counts::default().with(&value["message"]["usage"]);
                if self.id.is_none() {
                    self.id = value["message"]["id"].as_str().map(str::to_owned);
                }
            }
            "content_block_start" => self.started(index, value["content_block"].clone(), &mut out),
            "content_block_delta" => self.changed(index, &value["delta"], &mut out),
            "content_block_stop" => self.stopped(index, &mut out),
            "message_delta" => {
                if let Some(reason) = value["delta"]["stop_reason"].as_str() {
                    self.stop_reason = Some(reason.to_owned());
                }
                self.piece = self.piece.with(&value["usage"]);
            }
            "message_stop" => {
                self.close_message(&mut out);
                if self.stop_reason.as_deref() == Some("pause_turn") {
                    self.ending = Some(Ending::Paused);
                    out.push(Read::Paused);
                } else {
                    self.ending = Some(Ending::Ended);
                    out.push(Read::Event(Event::Finished(self.response())));
                }
            }
            "error" => out.push(Read::Failure(failure_of_error(&value["error"]))),
            _ => {}
        }
        out
    }

    fn response(&self) -> Response {
        let counts = self.total.plus(self.piece);
        Response {
            id: self.id.clone().unwrap_or_default(),
            model: String::new(),
            status: match self.stop_reason.as_deref() {
                Some("max_tokens" | "refusal" | "model_context_window_exceeded") => ResponseStatus::Incomplete,
                _ => ResponseStatus::Completed,
            },
            output: Vec::new(),
            usage: Some(counts.usage()),
            failure: None,
            incomplete: match self.stop_reason.as_deref() {
                Some("max_tokens" | "model_context_window_exceeded") => Some(Incomplete::MaxOutputTokens),
                Some("refusal") => Some(Incomplete::ContentFilter),
                _ => None,
            },
            context_tokens: Some(self.piece.input + self.piece.written + self.piece.read),
        }
    }

    fn started(&mut self, index: u64, raw: Value, out: &mut Vec<Read>) {
        let kind = raw["type"].as_str().unwrap_or_default().to_owned();
        if kind != "text" {
            self.close_message(out);
        }
        let at = self.sent.len();
        self.blocks.insert(index, at);
        let text = raw["text"]
            .as_str()
            .or_else(|| raw["thinking"].as_str())
            .unwrap_or_default()
            .to_owned();
        self.sent.push(Block {
            raw,
            item: None,
            text: String::new(),
        });
        match kind.as_str() {
            "text" => {
                let item = if let Some(open) = &self.message {
                    open.item
                } else {
                    let item = self.item();
                    let id = format!("{}_{item}", self.id.as_deref().unwrap_or_default());
                    out.push(Read::Event(Event::Began {
                        index: item,
                        item: Item::Message(Message::Assistant {
                            id: Some(id),
                            content: Vec::new(),
                            status: Some(ItemStatus::InProgress),
                            origin: None,
                        }),
                    }));
                    self.message = Some(OpenMessage { item, blocks: Vec::new() });
                    item
                };
                if let Some(open) = &mut self.message {
                    open.blocks.push(at);
                }
                self.sent[at].item = Some(item);
                self.add_text(at, Part::Text, &text, out);
            }
            "thinking" | "redacted_thinking" => {
                let item = self.item();
                self.sent[at].item = Some(item);
                out.push(Read::Event(Event::Began {
                    index: item,
                    item: Item::Reasoning(reasoning(format!("rs_{}_{item}", self.id.as_deref().unwrap_or_default()))),
                }));
                self.add_text(at, Part::Summary, &text, out);
            }
            "tool_use" => {
                let item = self.item();
                self.sent[at].item = Some(item);
                let call_id = self.sent[at].raw["id"].as_str().unwrap_or_default().to_owned();
                out.push(Read::Event(Event::Began {
                    index: item,
                    item: Item::FunctionCall(FunctionCall {
                        id: Some(format!("fc_{call_id}")),
                        call_id,
                        name: self.sent[at].raw["name"].as_str().unwrap_or_default().to_owned(),
                        arguments: String::new(),
                        status: Some(ItemStatus::InProgress),
                        origin: None,
                    }),
                }));
            }
            "server_tool_use" if self.sent[at].raw["name"] == "web_search" => {
                let item = self.item();
                self.sent[at].item = Some(item);
                out.push(Read::Event(Event::Began {
                    index: item,
                    item: Item::WebSearchCall(WebSearchCall {
                        id: format!("ws_{}", self.sent[at].raw["id"].as_str().unwrap_or_default()),
                        status: WebSearchStatus::InProgress,
                        action: None,
                        origin: None,
                    }),
                }));
            }
            "web_search_tool_result" => self.searched(at, out),
            _ => {}
        }
    }

    fn add_text(&mut self, at: usize, part: Part, text: &str, out: &mut Vec<Read>) {
        if text.is_empty() {
            return;
        }
        let block = &mut self.sent[at];
        block.text.push_str(text);
        if let Some(item) = block.item {
            out.push(Read::Event(Event::Delta {
                index: item,
                part,
                text: text.to_owned(),
            }));
        }
    }

    fn changed(&mut self, index: u64, delta: &Value, out: &mut Vec<Read>) {
        let Some(&at) = self.blocks.get(&index) else { return };
        let text = |name: &str| delta[name].as_str().unwrap_or_default().to_owned();
        match delta["type"].as_str().unwrap_or_default() {
            "text_delta" => self.add_text(at, Part::Text, &text("text"), out),
            "thinking_delta" => self.add_text(at, Part::Summary, &text("thinking"), out),
            "signature_delta" => {
                let block = &mut self.sent[at].raw;
                let signature = format!("{}{}", block["signature"].as_str().unwrap_or_default(), text("signature"));
                block["signature"] = json!(signature);
            }
            "input_json_delta" => {
                let partial = text("partial_json");
                let block = &mut self.sent[at];
                block.text.push_str(&partial);
                let is_call = block.raw["type"] == "tool_use";
                if is_call && let Some(item) = block.item {
                    out.push(Read::Event(Event::Delta {
                        index: item,
                        part: Part::Arguments,
                        text: partial,
                    }));
                }
            }
            "citations_delta" => {
                let block = &mut self.sent[at].raw;
                if !block["citations"].is_array() {
                    block["citations"] = json!([]);
                }
                if let Some(citations) = block["citations"].as_array_mut() {
                    citations.push(delta["citation"].clone());
                }
            }
            _ => {}
        }
    }

    fn stopped(&mut self, index: u64, out: &mut Vec<Read>) {
        let Some(&at) = self.blocks.get(&index) else { return };
        let block = &mut self.sent[at];
        let kind = block.raw["type"].as_str().unwrap_or_default().to_owned();
        match kind.as_str() {
            "text" => block.raw["text"] = json!(block.text),
            "thinking" => {
                block.raw["thinking"] = json!(block.text);
                let item = block.item.unwrap_or_default();
                let mut made = reasoning(format!("rs_{}_{item}", self.id.as_deref().unwrap_or_default()));
                made.summary = vec![block.text.clone()];
                made.signature = block.raw["signature"].as_str().map(str::to_owned);
                made.status = Some(ItemStatus::Completed);
                out.push(Read::Event(Event::Ended {
                    index: item,
                    item: Item::Reasoning(made),
                }));
            }
            "redacted_thinking" => {
                let item = block.item.unwrap_or_default();
                let mut made = reasoning(format!("rs_{}_{item}", self.id.as_deref().unwrap_or_default()));
                made.encrypted_content = block.raw["data"].as_str().map(str::to_owned);
                made.status = Some(ItemStatus::Completed);
                out.push(Read::Event(Event::Ended {
                    index: item,
                    item: Item::Reasoning(made),
                }));
            }
            "tool_use" => {
                let input = if block.text.is_empty() {
                    block
                        .raw
                        .get("input")
                        .filter(|input| input.as_object().is_some_and(|input| !input.is_empty()))
                        .map_or_else(|| "{}".to_owned(), Value::to_string)
                } else {
                    block.text.clone()
                };
                block.raw["input"] = serde_json::from_str(&input).unwrap_or_else(|_| json!({}));
                let item = block.item.unwrap_or_default();
                let call_id = block.raw["id"].as_str().unwrap_or_default().to_owned();
                out.push(Read::Event(Event::Ended {
                    index: item,
                    item: Item::FunctionCall(FunctionCall {
                        id: Some(format!("fc_{call_id}")),
                        call_id,
                        name: block.raw["name"].as_str().unwrap_or_default().to_owned(),
                        arguments: input,
                        status: Some(ItemStatus::Completed),
                        origin: None,
                    }),
                }));
            }
            "server_tool_use" if !block.text.is_empty() => {
                block.raw["input"] = serde_json::from_str(&block.text).unwrap_or_else(|_| json!({}));
            }
            _ => {}
        }
    }

    /// A search's results: the search it ran ended, its blocks kept to go back.
    fn searched(&mut self, at: usize, out: &mut Vec<Read>) {
        let tool_use_id = self.sent[at].raw["tool_use_id"].as_str().unwrap_or_default().to_owned();
        let Some(search) = self
            .sent
            .iter()
            .position(|block| block.raw["type"] == "server_tool_use" && block.raw["id"] == tool_use_id.as_str())
        else {
            return;
        };
        let results = &self.sent[at].raw["content"];
        let sources = results
            .as_array()
            .map(|found| {
                found
                    .iter()
                    .filter_map(|result| result["url"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let item = self.sent[search].item.unwrap_or_default();
        out.push(Read::Event(Event::Ended {
            index: item,
            item: Item::WebSearchCall(WebSearchCall {
                id: format!("ws_{tool_use_id}"),
                status: if results.is_array() {
                    WebSearchStatus::Completed
                } else {
                    WebSearchStatus::Failed
                },
                action: Some(WebSearchAction::Search {
                    query: self.sent[search].raw["input"]["query"].as_str().unwrap_or_default().to_owned(),
                    sources,
                }),
                origin: Some(blocks_origin(vec![self.sent[search].raw.clone(), self.sent[at].raw.clone()])),
            }),
        }));
    }

    /// Ends the message the text blocks made: its text, the pages they cite, the blocks it came as.
    fn close_message(&mut self, out: &mut Vec<Read>) {
        let Some(open) = self.message.take() else { return };
        let mut text = String::new();
        let mut annotations = Vec::new();
        for &at in &open.blocks {
            let block = &self.sent[at];
            let start = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
            text.push_str(&block.text);
            let end = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
            for citation in block.raw["citations"].as_array().into_iter().flatten() {
                if let Some(url) = citation["url"].as_str() {
                    annotations.push(UrlCitation {
                        url: url.to_owned(),
                        title: citation["title"].as_str().unwrap_or_default().to_owned(),
                        start_index: start,
                        end_index: end,
                    });
                }
            }
        }
        let blocks = open.blocks.iter().map(|&at| self.sent[at].raw.clone()).collect();
        out.push(Read::Event(Event::Ended {
            index: open.item,
            item: Item::Message(Message::Assistant {
                id: Some(format!("{}_{}", self.id.as_deref().unwrap_or_default(), open.item)),
                content: vec![OutputContent::OutputText { text, annotations }],
                status: Some(ItemStatus::Completed),
                origin: Some(blocks_origin(blocks)),
            }),
        }));
    }
}

fn reasoning(id: String) -> Reasoning {
    Reasoning {
        id: Some(id),
        summary: Vec::new(),
        content: Vec::new(),
        encrypted_content: None,
        signature: None,
        status: Some(ItemStatus::InProgress),
        origin: Origin {
            model: String::new(),
            provider_metadata: None,
        },
    }
}

/// An origin that keeps the blocks an item came as, for the model to be sent them back.
fn blocks_origin(blocks: Vec<Value>) -> Origin {
    Origin {
        model: String::new(),
        provider_metadata: Some(json!({ "anthropic": { "blocks": Value::Array(blocks) } })),
    }
}

/// The kind of failure one of Anthropic's error types stands for.
fn kind_of(kind: &str) -> Option<FailureKind> {
    Some(match kind {
        "authentication_error" => FailureKind::Auth,
        "permission_error" => FailureKind::Forbidden,
        "billing_error" => FailureKind::Quota,
        "rate_limit_error" => FailureKind::RateLimit,
        "request_too_large" => FailureKind::PayloadTooLarge,
        "overloaded_error" | "api_error" => FailureKind::Server,
        "timeout_error" => FailureKind::Timeout,
        "invalid_request_error" | "not_found_error" => FailureKind::InvalidRequest,
        _ => return None,
    })
}

fn said_of(error: &Value) -> Said {
    let message = error["message"]
        .as_str()
        .unwrap_or("The provider failed without saying why")
        .to_owned();
    let code = error["type"].as_str().map(str::to_owned);
    let kind = if super::context_too_long(&message) {
        Some(FailureKind::ContextOverflow)
    } else {
        code.as_deref().and_then(kind_of)
    };
    Said {
        kind,
        message: Some(message),
        code,
        ..Said::default()
    }
}

fn failure_of_error(error: &Value) -> Failure {
    let said = said_of(error);
    let mut failure = Failure::new(said.kind.unwrap_or(FailureKind::Server), said.message.unwrap_or_default());
    failure.code = said.code;
    failure
}

/// What a refusal before the stream says: its error's type and words, and — where Anthropic's unified limits say the
/// account's are spent — a quota used up, given again at the reset they name.
fn said(headers: &http::HeaderMap, body: &str) -> Said {
    let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    let mut said = serde_json::from_str::<Value>(body)
        .ok()
        .map(|value| said_of(&value["error"]))
        .unwrap_or_default();
    if header("anthropic-ratelimit-unified-status") == Some("rejected") {
        said.kind = Some(FailureKind::Quota);
        said.reset_at_ms = header("anthropic-ratelimit-unified-reset")
            .and_then(|reset| reset.trim().parse::<i64>().ok())
            .map(|seconds| seconds * 1000);
        said.window_minutes = match header("anthropic-ratelimit-unified-representative-claim") {
            Some("five_hour") => Some(300),
            Some("seven_day" | "seven_day_opus" | "seven_day_sonnet") => Some(10_080),
            _ => None,
        };
    }
    said
}

/// What makes each request of an answer: its address, its key where it has one, and the betas it needs.
struct Sender {
    url: String,
    key: Option<String>,
    betas: Vec<&'static str>,
}

impl Sender {
    fn request(&self, body: &Value) -> Result<http::Request<bytes::Bytes>, Failure> {
        let mut made = http::Request::post(&self.url)
            .header("anthropic-version", API_VERSION)
            .header("accept", "text/event-stream")
            .header("content-type", "application/json");
        if let Some(key) = &self.key {
            made = made.header("x-api-key", key);
        }
        if !self.betas.is_empty() {
            made = made.header("anthropic-beta", self.betas.join(","));
        }
        made.body(bytes::Bytes::from(body.to_string()))
            .map_err(|error| Failure::new(FailureKind::InvalidRequest, error.to_string()))
    }
}

/// A driver of Anthropic's Messages API at `base_url`: reached with an API key, sent as `x-api-key`, or through an
/// agent the person signs in to on its own, which takes none.
#[derive(Debug, Clone, Copy)]
pub struct AnthropicDriver {
    pub id: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    pub connection: Connection,
}

impl Driver for AnthropicDriver {
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
            let key = match self.connection {
                Connection::ApiKey { .. } => Some(
                    call.api_key()?
                        .ok_or_else(|| Failure::new(FailureKind::Auth, format!("No API key is set for {}", self.name)))?,
                ),
                _ => None,
            };
            let (mut body, betas) = render(call, model, request)?;
            let sender = Sender {
                url: format!("{}/messages", call.base_url(self.base_url)),
                key,
                betas,
            };
            let response = wire::send(call, sender.request(&body)?).await?;
            if !response.status().is_success() {
                return Err(wire::failure_of(call, response, &|headers: &http::HeaderMap, body: &str| Some(said(headers, body))).await);
            }
            let call = call.clone();
            let source: Source = Box::pin(async_stream::stream! {
                let mut reader = Reader::new();
                let mut response = response;
                let asked = body["messages"].as_array().map_or(0, Vec::len);
                loop {
                    let mut paused = false;
                    let mut events = wire::events(response.into_body());
                    while let Some(event) = events.next().await {
                        let event = match event {
                            Ok(event) => event,
                            Err(failure) => {
                                yield Err(failure);
                                return;
                            }
                        };
                        for read in reader.read(&event.data) {
                            match read {
                                Read::Event(made) => yield Ok(made),
                                Read::Failure(failure) => {
                                    yield Err(failure);
                                    return;
                                }
                                Read::Paused => paused = true,
                            }
                        }
                    }
                    if !paused {
                        return;
                    }
                    // The model goes on from the blocks of its answer so far, sent back as its turn.
                    if let Some(messages) = body["messages"].as_array_mut() {
                        messages.truncate(asked);
                        messages.push(json!({ "role": "assistant", "content": reader.sent() }));
                    }
                    reader.next_piece();
                    let sent = match sender.request(&body) {
                        Ok(sent) => wire::send(&call, sent).await,
                        Err(failure) => Err(failure),
                    };
                    response = match sent {
                        Ok(next) if next.status().is_success() => next,
                        Ok(next) => {
                            yield Err(wire::failure_of(&call, next, &|headers: &http::HeaderMap, body: &str| Some(said(headers, body))).await);
                            return;
                        }
                        Err(failure) => {
                            yield Err(failure);
                            return;
                        }
                    };
                }
            });
            Ok(source)
        })
    }
}
