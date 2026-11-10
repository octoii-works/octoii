//! The Responses API, as the providers that speak it share it: a request rendered in its form — the model by its id,
//! the conversation's items as it takes them, the files inline, the effort changes folded into the effort in effect,
//! kept nowhere, streamed — and its stream read back into the vocabulary's events. Each provider differs in its
//! dialect: the role developer messages go as, whether it takes a cache's name and gives back its reasoning encrypted,
//! and how its codes are read.

use futures::StreamExt;
use futures::future::BoxFuture;
use serde_json::{Map, Value, json};

use crate::catalogue::Model;
use crate::driver::{Call, Connection, Driver};
use crate::stream::{Event, Part, Source};
use crate::vocabulary::{
    Failure, FailureKind, FunctionCall, Incomplete, InputContent, Item, ItemStatus, Message, Origin, OutputContent, Reasoning,
    ReasoningEffort, Request, Response, ResponseStatus, Tool, ToolChoice, UrlCitation, Usage, WebSearchAction, WebSearchCall,
    WebSearchStatus,
};
use crate::wire::{self, FailureDetail};

/// The role a provider takes a developer message in: its own, or the system's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeveloperRole {
    Developer,
    System,
}

/// How a provider speaks the Responses API: the role it takes updates in, a developer's or the system's; whether it takes the name of the
/// conversation for its cache, gives back its models' reasoning encrypted, so a request without state goes on from it,
/// and takes text files, else sent as the person's text; and the kind of failure its codes and words stand for, where
/// they say one.
#[derive(Debug, Clone, Copy)]
pub struct Dialect {
    pub developer_role: DeveloperRole,
    pub cache_key: bool,
    pub encrypted_reasoning: bool,
    pub takes_text_files: bool,
    pub failure_kind: fn(code: Option<&str>, message: &str) -> Option<FailureKind>,
}

/// `request` as the Responses API takes it for `model`, in `dialect`.
///
/// # Errors
/// An `invalid_request` failure where a file it names is not the host's.
pub fn render(call: &Call, model: &Model, request: &Request, dialect: &Dialect) -> Result<Value, Failure> {
    let reference = model.reference();
    let mut effort = request.reasoning.map(|reasoning| reasoning.effort);
    let mut input = Vec::new();
    for item in &request.input {
        if let Item::ConfigurationUpdate(update) = item {
            effort = Some(update.effort);
            continue;
        }
        if let Some(made) = rendered_item(call, item, &reference, dialect)? {
            input.push(made);
        }
    }
    let mut body = Map::new();
    body.insert("model".into(), json!(model.id));
    if let Some(instructions) = &request.instructions {
        body.insert("instructions".into(), json!(instructions));
    }
    body.insert("input".into(), Value::Array(input));
    if !request.tools.is_empty() {
        body.insert("tools".into(), Value::Array(request.tools.iter().map(rendered_tool).collect()));
    }
    if let Some(choice) = &request.tool_choice {
        body.insert("tool_choice".into(), rendered_choice(choice));
    }
    if let Some(effort) = effort {
        body.insert("reasoning".into(), json!({ "effort": effort_name(effort), "summary": "auto" }));
        if dialect.encrypted_reasoning {
            body.insert("include".into(), json!(["reasoning.encrypted_content"]));
        }
    }
    if let Some(format) = &request.format {
        body.insert(
            "text".into(),
            json!({ "format": { "type": "json_schema", "name": format.name, "schema": format.schema, "strict": true } }),
        );
    }
    if let Some(most) = request.max_output_tokens {
        body.insert("max_output_tokens".into(), json!(most));
    }
    if dialect.cache_key
        && let Some(key) = &request.prompt_cache_key
    {
        body.insert("prompt_cache_key".into(), json!(key));
    }
    body.insert("stream".into(), json!(true));
    body.insert("store".into(), json!(false));
    Ok(Value::Object(body))
}

fn effort_name(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::None => "none",
        ReasoningEffort::Minimal => "minimal",
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
        ReasoningEffort::Xhigh => "xhigh",
        ReasoningEffort::Max => "max",
    }
}

fn wrote_it(origin: Option<&Origin>, reference: &str) -> bool {
    origin.is_some_and(|origin| origin.model == reference)
}

/// An item as the Responses API takes it, or none: another model's reasoning, or the model's own with nothing to go
/// on from, is left out; an id another model's item carries is left out, as a provider takes ids of its own alone.
fn rendered_item(call: &Call, item: &Item, reference: &str, dialect: &Dialect) -> Result<Option<Value>, Failure> {
    Ok(Some(match item {
        Item::Message(Message::User { content }) => message("user", rendered_content(call, content, dialect)?),
        Item::Message(Message::Developer { content } | Message::System { content }) => {
            let role = if dialect.developer_role == DeveloperRole::Developer {
                "developer"
            } else {
                "system"
            };
            message(role, rendered_content(call, content, dialect)?)
        }
        Item::Message(Message::Assistant { id, content, origin, .. }) => {
            let mut made = json!({ "type": "message", "role": "assistant", "status": "completed", "content": content.iter().map(rendered_output).collect::<Vec<_>>() });
            if let Some(id) = id.as_ref().filter(|_| wrote_it(origin.as_ref(), reference)) {
                made["id"] = json!(id);
            }
            made
        }
        Item::FunctionCall(FunctionCall {
            id,
            call_id,
            name,
            arguments,
            origin,
            ..
        }) => {
            let mut made = json!({ "type": "function_call", "call_id": call_id, "name": name, "arguments": arguments });
            if let Some(id) = id.as_ref().filter(|_| wrote_it(origin.as_ref(), reference)) {
                made["id"] = json!(id);
            }
            made
        }
        Item::FunctionCallOutput(output) => {
            let made = match output.output.as_slice() {
                [InputContent::InputText { text }] => json!(text),
                parts => Value::Array(rendered_content(call, parts, dialect)?),
            };
            json!({ "type": "function_call_output", "call_id": output.call_id, "output": made })
        }
        Item::Reasoning(reasoning) => {
            let (Some(encrypted), true) = (&reasoning.encrypted_content, reasoning.origin.model == reference) else {
                return Ok(None);
            };
            let mut made = json!({
                "type": "reasoning",
                "summary": reasoning.summary.iter().map(|text| json!({ "type": "summary_text", "text": text })).collect::<Vec<_>>(),
                "encrypted_content": encrypted,
            });
            if let Some(id) = &reasoning.id {
                made["id"] = json!(id);
            }
            made
        }
        Item::WebSearchCall(search) if wrote_it(search.origin.as_ref(), reference) => {
            json!({ "type": "web_search_call", "id": search.id, "status": "completed" })
        }
        Item::WebSearchCall(_) | Item::ConfigurationUpdate(_) => return Ok(None),
    }))
}

fn message(role: &str, content: Vec<Value>) -> Value {
    json!({ "type": "message", "role": role, "content": Value::Array(content) })
}

fn rendered_content(call: &Call, content: &[InputContent], dialect: &Dialect) -> Result<Vec<Value>, Failure> {
    content
        .iter()
        .map(|part| {
            Ok(match part {
                InputContent::InputText { text } => json!({ "type": "input_text", "text": text }),
                InputContent::InputImage { file, detail } => {
                    let detail = match detail {
                        crate::vocabulary::ImageDetail::Auto => "auto",
                        crate::vocabulary::ImageDetail::High => "high",
                        crate::vocabulary::ImageDetail::Low => "low",
                    };
                    json!({ "type": "input_image", "image_url": super::data_url(call, file)?, "detail": detail })
                }
                InputContent::InputFile { file } if !dialect.takes_text_files && super::is_text(&file.media_type) => {
                    json!({ "type": "input_text", "text": super::file_as_text(call, file)? })
                }
                InputContent::InputFile { file } => json!({
                    "type": "input_file",
                    "file_data": super::data_url(call, file)?,
                    "filename": file.filename.as_deref().unwrap_or("file"),
                }),
            })
        })
        .collect()
}

fn rendered_output(part: &OutputContent) -> Value {
    match part {
        OutputContent::OutputText { text, .. } => json!({ "type": "output_text", "text": text, "annotations": [] }),
        OutputContent::Refusal { refusal } => json!({ "type": "refusal", "refusal": refusal }),
    }
}

fn rendered_tool(tool: &Tool) -> Value {
    match tool {
        Tool::Function {
            name,
            description,
            parameters,
        } => json!({ "type": "function", "name": name, "description": description, "parameters": parameters }),
        Tool::WebSearch => json!({ "type": "web_search" }),
    }
}

fn rendered_choice(choice: &ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => json!("auto"),
        ToolChoice::None => json!("none"),
        ToolChoice::Required => json!("required"),
        ToolChoice::Function { name } => json!({ "type": "function", "name": name }),
    }
}

/// What a provider's stream says, read as it comes into the vocabulary's events.
#[derive(Debug)]
pub struct Reader {
    dialect: Dialect,
    /// The searches the provider ran, which a usage counts.
    searches: u64,
}

impl Reader {
    #[must_use]
    pub fn new(dialect: Dialect) -> Self {
        Self { dialect, searches: 0 }
    }

    /// The event one of the provider's events stands for; none for one that says nothing the vocabulary keeps.
    pub fn event(&mut self, data: &str) -> Option<Result<Event, Failure>> {
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return Some(Err(Failure::new(
                FailureKind::InvalidResponse,
                format!("The provider sent an event that is no JSON: {data}"),
            )));
        };
        let index = || {
            value["output_index"]
                .as_u64()
                .and_then(|index| u32::try_from(index).ok())
                .unwrap_or(0)
        };
        let delta = |part| {
            Some(Ok(Event::Delta {
                index: index(),
                part,
                text: value["delta"].as_str().unwrap_or_default().to_owned(),
            }))
        };
        match value["type"].as_str()? {
            "response.output_item.added" => item_of(&value["item"]).map(|item| Ok(Event::Began { index: index(), item })),
            "response.output_item.done" => item_of(&value["item"]).map(|item| {
                if let Item::WebSearchCall(WebSearchCall {
                    action: Some(WebSearchAction::Search { .. }),
                    ..
                }) = &item
                {
                    self.searches += 1;
                }
                Ok(Event::Ended { index: index(), item })
            }),
            "response.output_text.delta" => delta(Part::Text),
            "response.refusal.delta" => delta(Part::Refusal),
            "response.reasoning_text.delta" => delta(Part::Reasoning),
            "response.reasoning_summary_text.delta" => delta(Part::Summary),
            "response.function_call_arguments.delta" => delta(Part::Arguments),
            "response.completed" | "response.incomplete" | "response.failed" => {
                Some(Ok(Event::Finished(self.response(&value["response"]))))
            }
            "error" => {
                let error = if value["error"].is_object() { &value["error"] } else { &value };
                Some(Err(self.failure(error)))
            }
            _ => None,
        }
    }

    fn response(&self, given: &Value) -> Response {
        let status = match given["status"].as_str() {
            Some("completed") => ResponseStatus::Completed,
            Some("incomplete") => ResponseStatus::Incomplete,
            Some("in_progress" | "queued") => ResponseStatus::InProgress,
            _ => ResponseStatus::Failed,
        };
        let usage = given["usage"].is_object().then(|| usage_of(&given["usage"], self.searches));
        Response {
            id: given["id"].as_str().unwrap_or_default().to_owned(),
            model: String::new(),
            status,
            output: Vec::new(),
            context_tokens: usage.map(|usage| usage.input_tokens),
            usage,
            failure: (status == ResponseStatus::Failed).then(|| self.failure(&given["error"])),
            incomplete: match given["incomplete_details"]["reason"].as_str() {
                Some("max_output_tokens") => Some(Incomplete::MaxOutputTokens),
                Some("content_filter") => Some(Incomplete::ContentFilter),
                _ => None,
            },
        }
    }

    /// The kind of failure the provider's code and words say, as the dialect reads them, else a context too long by
    /// its words; none where they say none.
    fn kind(&self, code: Option<&str>, message: &str) -> Option<FailureKind> {
        (self.dialect.failure_kind)(code, message).or_else(|| super::context_too_long(message).then_some(FailureKind::ContextOverflow))
    }

    /// The failure an error of the provider's stream stands for, `server` where it says no kind.
    fn failure(&self, error: &Value) -> Failure {
        let detail = detail_of(error);
        let mut failure = Failure::new(
            self.kind(detail.code.as_deref(), &detail.message).unwrap_or(FailureKind::Server),
            detail.message,
        );
        failure.code = detail.code;
        failure
    }

    /// What a body of the provider's says of a refusal before the stream.
    #[must_use]
    pub fn failure_detail(&self, body: &str) -> Option<FailureDetail> {
        let value: Value = serde_json::from_str(body).ok()?;
        if let Some(detail) = value["detail"].as_str() {
            return Some(FailureDetail {
                message: Some(detail.to_owned()),
                ..FailureDetail::default()
            });
        }
        let detail = detail_of(value.get("error")?);
        Some(FailureDetail {
            kind: self.kind(detail.code.as_deref(), &detail.message),
            message: Some(detail.message),
            code: detail.code,
            ..FailureDetail::default()
        })
    }
}

/// An error as a provider writes it: its code, else its type, and its words, naming the field it concerns where it
/// names one.
struct ErrorSaid {
    code: Option<String>,
    message: String,
}

fn detail_of(error: &Value) -> ErrorSaid {
    let message = error["message"].as_str().unwrap_or("The provider failed without saying why");
    ErrorSaid {
        code: error["code"].as_str().or_else(|| error["type"].as_str()).map(str::to_owned),
        message: match error["param"].as_str() {
            Some(param) => format!("{message} ({param})"),
            None => message.to_owned(),
        },
    }
}

fn usage_of(given: &Value, searches: u64) -> Usage {
    let count = |value: &Value| value.as_u64().unwrap_or(0);
    Usage {
        input_tokens: count(&given["input_tokens"]),
        cached_tokens: count(&given["input_tokens_details"]["cached_tokens"]),
        cache_write_tokens: count(&given["input_tokens_details"]["cache_write_tokens"]),
        output_tokens: count(&given["output_tokens"]),
        reasoning_tokens: count(&given["output_tokens_details"]["reasoning_tokens"]),
        web_searches: searches,
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a cost of dollars, never below 0"
        )]
        cost_nanodollars: given["cost"].as_f64().map(|dollars| (dollars.max(0.0) * 1e9).round() as u64),
    }
}

fn status_of(given: &Value) -> Option<ItemStatus> {
    match given["status"].as_str()? {
        "in_progress" => Some(ItemStatus::InProgress),
        "completed" => Some(ItemStatus::Completed),
        "incomplete" => Some(ItemStatus::Incomplete),
        _ => None,
    }
}

fn text_of(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

fn texts_of(parts: &Value) -> Vec<String> {
    parts
        .as_array()
        .map(|parts| parts.iter().map(|part| text_of(&part["text"])).collect())
        .unwrap_or_default()
}

/// An item of the provider's output in the vocabulary; none for a kind the vocabulary keeps none of.
fn item_of(given: &Value) -> Option<Item> {
    let id = given["id"].as_str().map(str::to_owned);
    Some(match given["type"].as_str()? {
        "message" => Item::Message(Message::Assistant {
            id,
            content: given["content"]
                .as_array()
                .map(|parts| parts.iter().filter_map(output_of).collect())
                .unwrap_or_default(),
            status: status_of(given),
            origin: None,
        }),
        "function_call" => Item::FunctionCall(FunctionCall {
            id,
            call_id: text_of(&given["call_id"]),
            name: text_of(&given["name"]),
            arguments: text_of(&given["arguments"]),
            status: status_of(given),
            origin: None,
        }),
        "reasoning" => Item::Reasoning(Reasoning {
            id,
            summary: texts_of(&given["summary"]),
            content: texts_of(&given["content"]),
            encrypted_content: given["encrypted_content"].as_str().map(str::to_owned),
            signature: None,
            status: status_of(given),
            origin: Origin {
                model: String::new(),
                provider_metadata: None,
            },
        }),
        "web_search_call" | "openrouter:web_search" => Item::WebSearchCall(WebSearchCall {
            id: id.unwrap_or_default(),
            status: match given["status"].as_str() {
                Some("completed") => WebSearchStatus::Completed,
                Some("searching") => WebSearchStatus::Searching,
                Some("failed") => WebSearchStatus::Failed,
                _ => WebSearchStatus::InProgress,
            },
            action: action_of(&given["action"]),
            origin: None,
        }),
        _ => return None,
    })
}

fn output_of(part: &Value) -> Option<OutputContent> {
    match part["type"].as_str()? {
        "output_text" => Some(OutputContent::OutputText {
            text: text_of(&part["text"]),
            annotations: part["annotations"]
                .as_array()
                .map(|annotations| {
                    annotations
                        .iter()
                        .filter(|annotation| annotation["type"] == "url_citation")
                        .map(|annotation| UrlCitation {
                            url: text_of(&annotation["url"]),
                            title: text_of(&annotation["title"]),
                            start_index: annotation["start_index"]
                                .as_u64()
                                .and_then(|at| u32::try_from(at).ok())
                                .unwrap_or(0),
                            end_index: annotation["end_index"].as_u64().and_then(|at| u32::try_from(at).ok()).unwrap_or(0),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "refusal" => Some(OutputContent::Refusal {
            refusal: text_of(&part["refusal"]),
        }),
        _ => None,
    }
}

fn action_of(given: &Value) -> Option<WebSearchAction> {
    Some(match given["type"].as_str()? {
        "search" => WebSearchAction::Search {
            query: given["query"]
                .as_str()
                .map_or_else(|| given["queries"][0].as_str().unwrap_or_default().to_owned(), str::to_owned),
            sources: given["sources"]
                .as_array()
                .map(|sources| sources.iter().map(|source| text_of(&source["url"])).collect())
                .unwrap_or_default(),
        },
        "open_page" => WebSearchAction::OpenPage {
            url: given["url"].as_str().map(str::to_owned),
        },
        "find_in_page" | "find" => WebSearchAction::FindInPage {
            url: text_of(&given["url"]),
            pattern: text_of(&given["pattern"]),
        },
        _ => return None,
    })
}

/// A driver whose provider speaks the Responses API at `base_url`, reached with an API key: its id, its name, the
/// variable its key is read from where the host keeps none, and its dialect.
#[derive(Debug, Clone, Copy)]
pub struct ResponsesDriver {
    pub id: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    pub key_variable: &'static str,
    pub dialect: Dialect,
}

impl Driver for ResponsesDriver {
    fn id(&self) -> &'static str {
        self.id
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn connection(&self) -> Connection {
        Connection::ApiKey {
            variable: self.key_variable,
        }
    }

    fn stream<'a>(&'a self, call: &'a Call, model: &'a Model, request: &'a Request) -> BoxFuture<'a, Result<Source, Failure>> {
        Box::pin(async move {
            let key = call
                .api_key()?
                .ok_or_else(|| Failure::new(FailureKind::Auth, format!("No API key is set for {}", self.name)))?;
            let body = render(call, model, request, &self.dialect)?;
            let request = http::Request::post(format!("{}/responses", call.base_url(self.base_url)))
                .header("authorization", format!("Bearer {key}"))
                .header("accept", "text/event-stream")
                .header("content-type", "application/json")
                .body(bytes::Bytes::from(body.to_string()))
                .map_err(|error| Failure::new(FailureKind::InvalidRequest, error.to_string()))?;
            let response = wire::send(call, request).await?;
            let mut reader = Reader::new(self.dialect);
            if !response.status().is_success() {
                return Err(wire::failure_of(call, response, &|_: &http::HeaderMap, body: &str| reader.failure_detail(body)).await);
            }
            let events = wire::events(response.into_body()).filter_map(move |event| {
                let made = match event {
                    Ok(event) => reader.event(&event.data),
                    Err(failure) => Some(Err(failure)),
                };
                async move { made }
            });
            let source: Source = Box::pin(events);
            Ok(source)
        })
    }
}
