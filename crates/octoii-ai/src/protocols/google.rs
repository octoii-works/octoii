//! Google's Interactions API. A request goes as an interaction: the instructions and the leading updates as its
//! `system_instruction`, the conversation as steps — the person's input, the model's output, its calls by Gemini's ids
//! and their results naming them, its thoughts and Google's searches as the model wrote them — later updates inside
//! `<system-update>` at the end of the person's turn, files inline, text files as their text, and the settings in its
//! `generation_config`, kept nowhere, streamed. Its steps come back as items: an item whose step stopped is ended once
//! the next step starts or the interaction ends, as the end may cut it off.

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
use crate::wire::{self, FailureDetail};

/// What joins the texts Google takes as one.
const JOINER: &str = "\n\n";

/// What Google attached to an item.
fn google(origin: Option<&Origin>) -> Option<&Value> {
    origin?.provider_metadata.as_ref()?.get("google")
}

/// `request` as the interaction Google is asked for, for `model`.
///
/// # Errors
/// An `invalid_request` failure for an effort above Google's highest thinking level, a file in the system
/// instruction or a call's result Google takes none of, a call's result that answers no call, or a file the host does
/// not have.
pub fn render(call: &Call, model: &Model, request: &Request) -> Result<Value, Failure> {
    let reference = model.reference();
    let mut effort = request.reasoning.map(|reasoning| reasoning.effort);
    let mut builder = Builder {
        call,
        reference: &reference,
        steps: Vec::new(),
        user: None,
        waiting: Vec::new(),
        targets: request
            .input
            .iter()
            .filter_map(|item| match item {
                Item::FunctionCall(made) => {
                    let id = google(made.origin.as_ref())
                        .and_then(|own| own["id"].as_str())
                        .unwrap_or(&made.call_id)
                        .to_owned();
                    Some((made.call_id.clone(), (made.name.clone(), id)))
                }
                _ => None,
            })
            .collect(),
    };
    let mut system: Vec<String> = request.instructions.iter().filter(|text| !text.is_empty()).cloned().collect();
    let mut leading = true;
    for item in &request.input {
        match item {
            Item::Message(Message::Developer { content } | Message::System { content }) if leading => {
                for part in content {
                    match part {
                        InputContent::InputText { text } => system.push(text.clone()),
                        _ => {
                            return Err(Failure::new(
                                FailureKind::InvalidRequest,
                                "Google's system instruction takes text alone, and an update holds a file",
                            ));
                        }
                    }
                }
            }
            Item::ConfigurationUpdate(update) => effort = Some(update.effort),
            _ => {
                leading = false;
                builder.add(item)?;
            }
        }
    }
    builder.flush();
    let mut body = json!({ "model": model.id, "input": builder.steps, "store": false, "stream": true });
    if !system.is_empty() {
        body["system_instruction"] = json!(system.join(JOINER));
    }
    let tools: Vec<Value> = request
        .tools
        .iter()
        .map(|tool| match tool {
            Tool::Function {
                name,
                description,
                parameters,
            } => json!({ "type": "function", "name": name, "description": description, "parameters": parameters }),
            Tool::WebSearch => json!({ "type": "google_search" }),
        })
        .collect();
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
    }
    if let Some(format) = &request.format {
        body["response_format"] = json!({ "type": "text", "mime_type": "application/json", "schema": format.schema });
    }
    if let Some(config) = generation_config(request, effort)? {
        body["generation_config"] = config;
    }
    Ok(body)
}

/// The request's settings as Google's `generation_config`, each where it is set; none where none is.
fn generation_config(request: &Request, effort: Option<ReasoningEffort>) -> Result<Option<Value>, Failure> {
    let mut config = serde_json::Map::new();
    if let Some(most) = request.max_output_tokens {
        config.insert("max_output_tokens".into(), json!(most));
    }
    if let Some(effort) = effort {
        let level = match effort {
            ReasoningEffort::None | ReasoningEffort::Minimal => "minimal",
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium => "medium",
            ReasoningEffort::High => "high",
            ReasoningEffort::Xhigh | ReasoningEffort::Max => {
                return Err(Failure::new(
                    FailureKind::InvalidRequest,
                    format!("Google's thinking levels go up to high, and the request asks for {effort:?}"),
                ));
            }
        };
        config.insert("thinking_level".into(), json!(level));
        config.insert("thinking_summaries".into(), json!("auto"));
    }
    if let Some(choice) = &request.tool_choice {
        config.insert(
            "tool_choice".into(),
            match choice {
                ToolChoice::Auto => json!("auto"),
                ToolChoice::None => json!("none"),
                ToolChoice::Required => json!("any"),
                ToolChoice::Function { name } => json!({ "allowed_tools": { "mode": "any", "tools": [name] } }),
            },
        );
    }
    Ok((!config.is_empty()).then_some(Value::Object(config)))
}

/// The steps of a conversation as they are added, the updates waiting for the end of the person's turn.
struct Builder<'a> {
    call: &'a Call,
    reference: &'a str,
    steps: Vec<Value>,
    /// The place of the last step, where it is the person's input.
    user: Option<usize>,
    waiting: Vec<Value>,
    /// Each call's function and Gemini's id of it, by its `call_id`, which its result names.
    targets: std::collections::HashMap<String, (String, String)>,
}

impl Builder<'_> {
    fn add(&mut self, item: &Item) -> Result<(), Failure> {
        match item {
            Item::Message(Message::User { content }) => {
                let blocks = self.blocks(content)?;
                self.push(json!({ "type": "user_input", "content": blocks }), true);
            }
            Item::Message(Message::Developer { content } | Message::System { content }) => {
                let (texts, files): (Vec<&InputContent>, Vec<&InputContent>) =
                    content.iter().partition(|part| matches!(part, InputContent::InputText { .. }));
                let text: Vec<&str> = texts
                    .iter()
                    .filter_map(|part| match part {
                        InputContent::InputText { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                self.waiting
                    .push(json!({ "type": "text", "text": super::system_update(&text.join(JOINER)) }));
                for file in files {
                    let block = self.block(file)?;
                    self.waiting.push(block);
                }
            }
            Item::Message(Message::Assistant { content, .. }) => {
                let blocks: Vec<Value> = content
                    .iter()
                    .map(|part| match part {
                        OutputContent::OutputText { text, .. } => json!({ "type": "text", "text": text }),
                        OutputContent::Refusal { refusal } => json!({ "type": "text", "text": refusal }),
                    })
                    .collect();
                self.push(json!({ "type": "model_output", "content": blocks }), false);
            }
            Item::FunctionCall(made) => {
                let id = self.targets.get(&made.call_id).map_or(made.call_id.clone(), |(_, id)| id.clone());
                let arguments = serde_json::from_str::<Value>(&made.arguments)
                    .ok()
                    .filter(Value::is_object)
                    .unwrap_or_else(|| json!({}));
                self.push(
                    json!({ "type": "function_call", "id": id, "name": made.name, "arguments": arguments }),
                    false,
                );
            }
            Item::FunctionCallOutput(output) => {
                let (name, id) = self.targets.get(&output.call_id).cloned().ok_or_else(|| {
                    Failure::new(
                        FailureKind::InvalidRequest,
                        format!(
                            "The output of {} answers no call of the conversation, so Google cannot name its function",
                            output.call_id
                        ),
                    )
                })?;
                let mut result = Vec::new();
                for part in &output.output {
                    if matches!(part, InputContent::InputFile { .. }) {
                        return Err(Failure::new(
                            FailureKind::InvalidRequest,
                            format!(
                                "The output of {} holds a file, and Google takes text and images alone in a function's result",
                                output.call_id
                            ),
                        ));
                    }
                    result.push(self.block(part)?);
                }
                let mut step = json!({ "type": "function_result", "call_id": id, "name": name, "result": result });
                if output.is_error {
                    step["is_error"] = json!(true);
                }
                self.push(step, true);
            }
            Item::Reasoning(reasoning) if reasoning.origin.model == self.reference => {
                let text = [reasoning.summary.join(JOINER), reasoning.content.join(JOINER)]
                    .into_iter()
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
                    .join(JOINER);
                if text.is_empty() && reasoning.signature.is_none() {
                    return Ok(());
                }
                let mut step = json!({ "type": "thought" });
                if let Some(signature) = &reasoning.signature {
                    step["signature"] = json!(signature);
                }
                if !text.is_empty() {
                    step["summary"] = json!([{ "type": "text", "text": text }]);
                }
                self.push(step, false);
            }
            Item::WebSearchCall(search) if search.origin.as_ref().is_some_and(|origin| origin.model == self.reference) => {
                self.search(search);
            }
            _ => {}
        }
        Ok(())
    }

    /// A search Google ran, as the two steps it was: its call with its queries, and its result.
    fn search(&mut self, search: &WebSearchCall) {
        let Some(own) = google(search.origin.as_ref()).cloned() else {
            return;
        };
        let Some(id) = own["id"].as_str() else { return };
        let queries: Vec<&str> = match &search.action {
            Some(WebSearchAction::Search { query, .. }) => vec![query.as_str()],
            _ => Vec::new(),
        };
        let mut called = json!({ "type": "google_search_call", "id": id, "arguments": { "queries": queries } });
        if let Some(signature) = own["signature"].as_str() {
            called["signature"] = json!(signature);
        }
        self.push(called, false);
        let result: Vec<Value> = own["search_suggestions"]
            .as_str()
            .map(|given| json!({ "search_suggestions": given }))
            .into_iter()
            .collect();
        let mut found = json!({ "type": "google_search_result", "call_id": id, "result": result });
        if let Some(signature) = own["result_signature"].as_str() {
            found["signature"] = json!(signature);
        }
        self.push(found, false);
    }

    /// Adds `step`, the updates waiting given before it where it is the model's.
    fn push(&mut self, step: Value, users: bool) {
        if !users {
            self.flush();
        }
        let input = step["type"] == "user_input";
        self.steps.push(step);
        self.user = input.then_some(self.steps.len() - 1);
    }

    /// Gives the updates waiting to the person's turn they stand in: at the end of its last input, else a step of
    /// their own.
    fn flush(&mut self) {
        if self.waiting.is_empty() {
            return;
        }
        let waiting = std::mem::take(&mut self.waiting);
        if let Some(at) = self.user
            && let Some(content) = self.steps[at]["content"].as_array_mut()
        {
            content.extend(waiting);
            return;
        }
        self.push(json!({ "type": "user_input", "content": waiting }), true);
    }

    fn blocks(&self, content: &[InputContent]) -> Result<Vec<Value>, Failure> {
        content.iter().map(|part| self.block(part)).collect()
    }

    /// A part of a message as Google's block: text, an image, a text file as its text, any other file a document.
    fn block(&self, part: &InputContent) -> Result<Value, Failure> {
        use base64::Engine;
        let inline = |reference: &str| {
            self.call
                .files
                .read(reference)
                .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes))
                .map_err(|missing| Failure::new(FailureKind::InvalidRequest, missing.to_string()))
        };
        Ok(match part {
            InputContent::InputText { text } => json!({ "type": "text", "text": text }),
            InputContent::InputImage { file, .. } => {
                json!({ "type": "image", "data": inline(&file.reference)?, "mime_type": file.media_type })
            }
            InputContent::InputFile { file } if super::is_text(&file.media_type) => {
                json!({ "type": "text", "text": super::file_as_text(self.call, file)? })
            }
            InputContent::InputFile { file } => {
                json!({ "type": "document", "data": inline(&file.reference)?, "mime_type": file.media_type })
            }
        })
    }
}

/// An item of the answer while its step may add to it.
#[derive(Debug)]
enum Open {
    Message {
        item: u32,
        text: String,
        annotations: Vec<UrlCitation>,
    },
    Thought {
        item: u32,
        text: String,
        signature: Option<String>,
    },
    Call {
        item: u32,
        google_id: Option<String>,
        name: String,
        arguments: String,
        started: Option<Value>,
    },
    Search {
        item: u32,
        google: Value,
        query: String,
    },
}

impl Open {
    fn item(&self) -> u32 {
        match self {
            Self::Message { item, .. } | Self::Thought { item, .. } | Self::Call { item, .. } | Self::Search { item, .. } => *item,
        }
    }
}

/// Google's interaction read into the vocabulary's events.
#[derive(Debug, Default)]
pub struct Reader {
    id: Option<String>,
    items: u32,
    /// The items the steps stream into, by the step's index.
    steps: std::collections::HashMap<u64, Open>,
    /// The items whose steps stopped, ended once the next step starts or the interaction ends.
    stopped: Vec<Open>,
    ended: bool,
}

impl Reader {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the interaction ended.
    #[must_use]
    pub fn ended(&self) -> bool {
        self.ended
    }

    fn item(&mut self) -> u32 {
        self.items += 1;
        self.items - 1
    }

    /// The events one of the interaction's events stands for.
    ///
    /// # Errors
    /// The failure an event that is no JSON, an `error`, or an interaction that failed stands for.
    pub fn read(&mut self, data: &str) -> Result<Vec<Event>, Failure> {
        let value: Value = serde_json::from_str(data).map_err(|_| {
            Failure::new(
                FailureKind::InvalidResponse,
                format!("The provider sent an event that is no JSON: {data}"),
            )
        })?;
        let mut out = Vec::new();
        let index = value["index"].as_u64().unwrap_or(0);
        match value["event_type"].as_str().unwrap_or_default() {
            "interaction.created" => self.id = value["interaction"]["id"].as_str().map(str::to_owned),
            "step.start" => {
                self.end_stopped(ItemStatus::Completed, &mut out);
                self.started(index, &value["step"], &mut out);
            }
            "step.delta" => self.changed(index, &value["delta"], &mut out),
            "step.stop" => {
                if let Some(open) = self.steps.remove(&index) {
                    self.stopped.push(open);
                }
            }
            "interaction.completed" => self.completed(&value["interaction"], &mut out)?,
            "error" => return Err(failure_of(&value["error"])),
            _ => {}
        }
        Ok(out)
    }

    fn started(&mut self, index: u64, step: &Value, out: &mut Vec<Event>) {
        let item = |reader: &mut Self| reader.item();
        let open = match step["type"].as_str().unwrap_or_default() {
            "model_output" => {
                let item = item(self);
                out.push(Event::Began {
                    index: item,
                    item: message(String::new(), Vec::new(), ItemStatus::InProgress),
                });
                let mut open = Open::Message {
                    item,
                    text: String::new(),
                    annotations: Vec::new(),
                };
                for block in step["content"].as_array().into_iter().flatten() {
                    added_text(&mut open, block, out);
                }
                open
            }
            "thought" => {
                let item = item(self);
                out.push(Event::Began {
                    index: item,
                    item: Item::Reasoning(thought(String::new(), None, ItemStatus::InProgress)),
                });
                let mut open = Open::Thought {
                    item,
                    text: String::new(),
                    signature: step["signature"]
                        .as_str()
                        .filter(|signature| !signature.is_empty())
                        .map(str::to_owned),
                };
                for block in step["summary"].as_array().into_iter().flatten() {
                    added_text(&mut open, block, out);
                }
                open
            }
            "function_call" => {
                let item = item(self);
                let name = step["name"].as_str().unwrap_or_default().to_owned();
                out.push(Event::Began {
                    index: item,
                    item: Item::FunctionCall(FunctionCall {
                        id: None,
                        call_id: String::new(),
                        name: name.clone(),
                        arguments: String::new(),
                        status: Some(ItemStatus::InProgress),
                        origin: None,
                    }),
                });
                Open::Call {
                    item,
                    google_id: step["id"].as_str().map(str::to_owned),
                    name,
                    arguments: String::new(),
                    started: step
                        .get("arguments")
                        .filter(|arguments| arguments.as_object().is_some_and(|given| !given.is_empty()))
                        .cloned(),
                }
            }
            "google_search_call" => {
                let item = item(self);
                out.push(Event::Began {
                    index: item,
                    item: Item::WebSearchCall(WebSearchCall {
                        id: format!("ws_{}", step["id"].as_str().unwrap_or_default()),
                        status: WebSearchStatus::InProgress,
                        action: None,
                        origin: None,
                    }),
                });
                Open::Search {
                    item,
                    google: json!({ "id": step["id"], "signature": step["signature"] }),
                    query: step["arguments"]["queries"][0].as_str().unwrap_or_default().to_owned(),
                }
            }
            "google_search_result" => {
                let call_id = step["call_id"].as_str().unwrap_or_default();
                if let Some(at) = self
                    .stopped
                    .iter()
                    .position(|open| matches!(open, Open::Search { google, .. } if google["id"] == call_id))
                {
                    let mut search = self.stopped.remove(at);
                    if let Open::Search { google, .. } = &mut search {
                        google["result_signature"] = step["signature"].clone();
                        google["search_suggestions"] = step["result"][0]["search_suggestions"].clone();
                    }
                    self.stopped.push(search);
                }
                return;
            }
            _ => return,
        };
        self.steps.insert(index, open);
    }

    fn changed(&mut self, index: u64, delta: &Value, out: &mut Vec<Event>) {
        let Some(open) = self.steps.get_mut(&index) else { return };
        match (open, delta["type"].as_str().unwrap_or_default()) {
            (open @ Open::Message { .. }, "text") => added_text(open, delta, out),
            (Open::Message { annotations, .. }, "text_annotation_delta") => annotations.extend(citations(&delta["annotations"])),
            (open @ Open::Thought { .. }, "thought_summary") => added_text(open, &delta["content"], out),
            (Open::Thought { signature, .. }, "thought_signature") => {
                if let Some(given) = delta["signature"].as_str().filter(|given| !given.is_empty()) {
                    *signature = Some(given.to_owned());
                }
            }
            (Open::Call { item, arguments, .. }, "arguments_delta") => {
                if let Some(given) = delta["arguments"].as_str().filter(|given| !given.is_empty()) {
                    arguments.push_str(given);
                    out.push(Event::Delta {
                        index: *item,
                        part: Part::Arguments,
                        text: given.to_owned(),
                    });
                }
            }
            _ => {}
        }
    }

    /// Ends every item whose step stopped, in the order they stand.
    fn end_stopped(&mut self, status: ItemStatus, out: &mut Vec<Event>) {
        let mut stopped = std::mem::take(&mut self.stopped);
        stopped.sort_by_key(Open::item);
        for open in stopped {
            out.push(ended(open, status));
        }
    }

    fn completed(&mut self, interaction: &Value, out: &mut Vec<Event>) -> Result<(), Failure> {
        self.ended = true;
        let status = interaction["status"].as_str().unwrap_or_default();
        let cut = status == "incomplete";
        if !matches!(status, "completed" | "requires_action" | "incomplete") {
            let first = &interaction["errors"][0];
            return Err(if status == "failed" && first.is_object() {
                failure_of(first)
            } else {
                Failure::new(
                    FailureKind::Server,
                    format!(
                        "The interaction ended {}",
                        if status.is_empty() { "without a status" } else { status }
                    ),
                )
            });
        }
        let open: Vec<Open> = self.steps.drain().map(|(_, open)| open).collect();
        self.stopped.extend(open);
        self.end_stopped(if cut { ItemStatus::Incomplete } else { ItemStatus::Completed }, out);
        let usage = interaction["usage"].is_object().then(|| usage_of(&interaction["usage"]));
        out.push(Event::Finished(Response {
            id: self.id.clone().unwrap_or_default(),
            model: String::new(),
            status: if cut {
                ResponseStatus::Incomplete
            } else {
                ResponseStatus::Completed
            },
            output: Vec::new(),
            context_tokens: usage.map(|usage| usage.input_tokens),
            usage,
            failure: None,
            incomplete: cut.then_some(Incomplete::MaxOutputTokens),
        }));
        Ok(())
    }
}

/// Text a block adds to the message or thought it streams into.
fn added_text(open: &mut Open, block: &Value, out: &mut Vec<Event>) {
    let Some(given) = block["text"].as_str().filter(|given| !given.is_empty()) else {
        return;
    };
    let (item, text, part) = match open {
        Open::Message { item, text, annotations } => {
            annotations.extend(citations(&block["annotations"]));
            (*item, text, Part::Text)
        }
        Open::Thought { item, text, .. } => (*item, text, Part::Summary),
        _ => return,
    };
    text.push_str(given);
    out.push(Event::Delta {
        index: item,
        part,
        text: given.to_owned(),
    });
}

fn citations(annotations: &Value) -> Vec<UrlCitation> {
    annotations
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|annotation| {
            Some(UrlCitation {
                url: annotation["url"].as_str().or_else(|| annotation["source"].as_str())?.to_owned(),
                title: annotation["title"].as_str().unwrap_or_default().to_owned(),
                start_index: annotation["start_index"]
                    .as_u64()
                    .and_then(|at| u32::try_from(at).ok())
                    .unwrap_or(0),
                end_index: annotation["end_index"].as_u64().and_then(|at| u32::try_from(at).ok()).unwrap_or(0),
            })
        })
        .collect()
}

fn message(text: String, annotations: Vec<UrlCitation>, status: ItemStatus) -> Item {
    Item::Message(Message::Assistant {
        id: None,
        content: if text.is_empty() {
            Vec::new()
        } else {
            vec![OutputContent::OutputText { text, annotations }]
        },
        status: Some(status),
        origin: None,
    })
}

fn thought(text: String, signature: Option<String>, status: ItemStatus) -> Reasoning {
    Reasoning {
        id: None,
        summary: if text.is_empty() { Vec::new() } else { vec![text] },
        content: Vec::new(),
        encrypted_content: None,
        signature,
        status: Some(status),
        origin: Origin {
            model: String::new(),
            provider_metadata: None,
        },
    }
}

/// The event that ends an item whose step stopped.
fn ended(open: Open, status: ItemStatus) -> Event {
    let index = open.item();
    let item = match open {
        Open::Message { text, annotations, .. } => message(text, annotations, status),
        Open::Thought { text, signature, .. } => Item::Reasoning(thought(text, signature, status)),
        Open::Call {
            google_id,
            name,
            arguments,
            started,
            ..
        } => Item::FunctionCall(FunctionCall {
            id: None,
            call_id: String::new(),
            name,
            arguments: if arguments.is_empty() {
                started.map_or_else(|| "{}".to_owned(), |given| given.to_string())
            } else {
                arguments
            },
            status: Some(status),
            origin: google_id.map(|id| Origin {
                model: String::new(),
                provider_metadata: Some(json!({ "google": { "id": id } })),
            }),
        }),
        Open::Search { google, query, .. } => Item::WebSearchCall(WebSearchCall {
            id: format!("ws_{}", google["id"].as_str().unwrap_or_default()),
            status: if status == ItemStatus::Completed {
                WebSearchStatus::Completed
            } else {
                WebSearchStatus::Failed
            },
            action: Some(WebSearchAction::Search {
                query,
                sources: Vec::new(),
            }),
            origin: Some(Origin {
                model: String::new(),
                provider_metadata: Some(json!({ "google": google })),
            }),
        }),
    };
    Event::Ended { index, item }
}

fn usage_of(given: &Value) -> Usage {
    let count = |name: &str| given[name].as_u64().unwrap_or(0);
    let thoughts = count("total_thought_tokens");
    let searches = given["grounding_tool_count"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|tool| tool["type"] == "google_search")
        .filter_map(|tool| tool["count"].as_u64())
        .sum();
    Usage {
        input_tokens: count("total_input_tokens"),
        cached_tokens: count("total_cached_tokens"),
        cache_write_tokens: 0,
        // Google's output leaves out the thoughts'.
        output_tokens: count("total_output_tokens") + thoughts,
        reasoning_tokens: thoughts,
        web_searches: searches,
        cost_nanodollars: None,
    }
}

/// The kind of failure one of Google's codes stands for.
fn failure_kind(code: &str) -> Option<FailureKind> {
    Some(match code {
        "RESOURCE_EXHAUSTED" | "resource_exhausted" => FailureKind::RateLimit,
        "UNAUTHENTICATED" | "unauthenticated" => FailureKind::Auth,
        "PERMISSION_DENIED" | "permission_denied" => FailureKind::Forbidden,
        "INVALID_ARGUMENT" | "invalid_argument" | "FAILED_PRECONDITION" | "NOT_FOUND" => FailureKind::InvalidRequest,
        "UNAVAILABLE" | "unavailable" | "INTERNAL" | "internal" => FailureKind::Server,
        "DEADLINE_EXCEEDED" | "deadline_exceeded" => FailureKind::Timeout,
        _ => return None,
    })
}

fn detail_of(error: &Value) -> FailureDetail {
    let message = error["message"]
        .as_str()
        .unwrap_or("The provider failed without saying why")
        .to_owned();
    let code = error["code"].as_str().or_else(|| error["status"].as_str()).map(str::to_owned);
    FailureDetail {
        kind: if super::context_too_long(&message) {
            Some(FailureKind::ContextOverflow)
        } else {
            code.as_deref().and_then(failure_kind)
        },
        message: Some(message),
        code,
        ..FailureDetail::default()
    }
}

fn failure_of(error: &Value) -> Failure {
    let detail = detail_of(error);
    let mut failure = Failure::new(detail.kind.unwrap_or(FailureKind::Server), detail.message.unwrap_or_default());
    failure.code = detail.code;
    failure
}

/// Google's Gemini API, reached with an API key.
#[derive(Debug, Clone, Copy)]
pub struct GoogleDriver;

/// Where the Gemini API is.
pub const BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

impl Driver for GoogleDriver {
    fn id(&self) -> &'static str {
        "google"
    }

    fn name(&self) -> &'static str {
        "Google"
    }

    fn connection(&self) -> Connection {
        Connection::ApiKey {
            variable: "GEMINI_API_KEY",
        }
    }

    fn stream<'a>(&'a self, call: &'a Call, model: &'a Model, request: &'a Request) -> BoxFuture<'a, Result<Source, Failure>> {
        Box::pin(async move {
            let key = call
                .api_key()?
                .ok_or_else(|| Failure::new(FailureKind::Auth, "No API key is set for Google"))?;
            let body = render(call, model, request)?;
            let made = http::Request::post(format!("{}/interactions", call.base_url(BASE_URL)))
                .header("x-goog-api-key", key)
                .header("accept", "text/event-stream")
                .header("content-type", "application/json")
                .body(bytes::Bytes::from(body.to_string()))
                .map_err(|error| Failure::new(FailureKind::InvalidRequest, error.to_string()))?;
            let response = wire::send(call, made).await?;
            if !response.status().is_success() {
                return Err(wire::failure_of(call, response, &|_: &http::HeaderMap, body: &str| {
                    serde_json::from_str::<Value>(body).ok().map(|value| detail_of(&value["error"]))
                })
                .await);
            }
            let source: Source = Box::pin(async_stream::stream! {
                let mut reader = Reader::new();
                let mut events = wire::events(response.into_body());
                while let Some(event) = events.next().await {
                    let event = match event {
                        Ok(event) => event,
                        Err(failure) => {
                            yield Err(failure);
                            return;
                        }
                    };
                    if event.event == "done" || event.data.trim() == "[DONE]" {
                        break;
                    }
                    match reader.read(&event.data) {
                        Ok(made) => {
                            for each in made {
                                yield Ok(each);
                            }
                        }
                        Err(failure) => {
                            yield Err(failure);
                            return;
                        }
                    }
                    if reader.ended() {
                        return;
                    }
                }
                yield Err(Failure::new(FailureKind::Transport, "The provider's stream ended before the interaction completed"));
            });
            Ok(source)
        })
    }
}
