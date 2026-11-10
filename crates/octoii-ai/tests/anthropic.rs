//! Anthropic's Messages API: a conversation rendered as it takes it, its stream read back, a paused answer asked to go
//! on, and a spent account's quota.

mod support;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use futures::future::BoxFuture;
use octoii_ai::catalogue::{Model, Reasoning as ReasoningFacts};
use octoii_ai::driver::Driver;
use octoii_ai::drivers::{ANTHROPIC, CLAUDE};
use octoii_ai::protocols::anthropic;
use octoii_ai::stream::TextStream;
use octoii_ai::transport::{Body, Transport, TransportError};
use octoii_ai::vocabulary::{
    ConfigurationUpdate, FailureKind, FunctionCall, FunctionCallOutput, InputContent, Item, Message, Origin, OutputContent,
    ReasoningConfig, ReasoningEffort, Request, Tool, ToolChoice,
};
use serde_json::json;

fn claude() -> Model {
    Model {
        max_output_tokens: Some(64_000),
        reasoning: Some(ReasoningFacts {
            efforts: None,
            default_effort: Some(ReasoningEffort::Medium),
            mandatory: false,
        }),
        ..support::model("anthropic", "claude-opus-5-5")
    }
}

fn own(blocks: &serde_json::Value) -> Origin {
    Origin {
        model: "anthropic/claude-opus-5-5".into(),
        provider_metadata: Some(json!({ "anthropic": { "blocks": blocks } })),
    }
}

fn text(words: &str) -> Vec<InputContent> {
    vec![InputContent::text(words)]
}

fn conversation() -> Request {
    Request {
        model: "anthropic/claude-opus-5-5".into(),
        instructions: Some("Be brief.".into()),
        input: vec![
            Item::Message(Message::Developer {
                content: text("You work in /repo."),
            }),
            Item::Message(Message::User { content: text("read a") }),
            Item::FunctionCall(FunctionCall {
                id: None,
                call_id: "call:1".into(),
                name: "read".into(),
                arguments: r#"{"path":"a"}"#.into(),
                status: None,
                origin: None,
            }),
            Item::FunctionCallOutput(FunctionCallOutput {
                call_id: "call:1".into(),
                output: text("# A"),
                is_error: false,
            }),
            Item::Message(Message::User {
                content: text("and summarize"),
            }),
            Item::Message(Message::Developer {
                content: text("the date changed"),
            }),
            Item::ConfigurationUpdate(ConfigurationUpdate {
                effort: ReasoningEffort::High,
            }),
            Item::Message(Message::Assistant {
                id: None,
                content: vec![OutputContent::OutputText {
                    text: "A is a title.".into(),
                    annotations: vec![],
                }],
                status: None,
                origin: Some(own(&json!([{ "type": "text", "text": "A is a title.", "citations": [] }]))),
            }),
        ],
        tools: vec![Tool::Function {
            name: "read".into(),
            description: "Reads a file.".into(),
            parameters: json!({ "type": "object" }),
        }],
        tool_choice: Some(ToolChoice::Auto),
        reasoning: Some(ReasoningConfig {
            effort: ReasoningEffort::Medium,
        }),
        format: None,
        max_output_tokens: None,
        prompt_cache_key: None,
    }
}

#[test]
fn a_conversation_goes_as_anthropic_takes_it_updates_as_system_messages_after_the_persons_turn() {
    let call = support::call(support::Provider::streaming(&[]));
    let (body, betas) = anthropic::render(&call, &claude(), &conversation()).expect("rendered");
    insta::assert_json_snapshot!("anthropic", body);
    assert_eq!(
        betas,
        Vec::<&str>::new(),
        "an effort changed is folded where the model takes none where it stands"
    );
}

#[test]
fn an_update_is_the_persons_tagged_text_for_a_model_that_takes_no_system_message() {
    let call = support::call(support::Provider::streaming(&[]));
    let mut model = claude();
    model.features.instruction_updates = Some(false);
    let (body, _) = anthropic::render(&call, &model, &conversation()).expect("rendered");
    let turn = &body["messages"][2];
    assert_eq!(turn["role"], "user");
    assert_eq!(turn["content"][0]["type"], "tool_result", "a call's result first in its turn");
    assert_eq!(turn["content"][1]["text"], "and summarize");
    assert_eq!(turn["content"][2]["text"], "<system-update>\nthe date changed\n</system-update>");
}

#[test]
fn a_call_forced_while_the_model_reasons_is_refused_before_anything_is_sent() {
    let call = support::call(support::Provider::streaming(&[]));
    let request = Request {
        tool_choice: Some(ToolChoice::Required),
        ..conversation()
    };
    let refused = anthropic::render(&call, &claude(), &request).expect_err("refused");
    assert_eq!(refused.kind, FailureKind::InvalidRequest);
}

/// A response as a provider gives it: its status, its headers and its body.
type Answer = (u16, Vec<(&'static str, &'static str)>, String);

/// A transport that answers each request with the next of its responses.
struct Answers {
    left: Mutex<VecDeque<Answer>>,
    sent: Mutex<Vec<serde_json::Value>>,
}

impl Transport for Answers {
    fn send(&self, request: http::Request<Bytes>) -> BoxFuture<'static, Result<http::Response<Body>, TransportError>> {
        self.sent
            .lock()
            .expect("held")
            .push(serde_json::from_slice(request.body()).expect("JSON"));
        let (status, headers, body) = self.left.lock().expect("held").pop_front().expect("an answer left");
        let mut response = http::Response::builder().status(status);
        for (name, value) in headers {
            response = response.header(name, value);
        }
        let body: Body = Box::pin(futures::stream::iter([Ok(Bytes::from(body))]));
        let response = response.body(body).expect("a response");
        Box::pin(async move { Ok(response) })
    }
}

fn events(events: &[serde_json::Value]) -> String {
    use std::fmt::Write as _;
    events.iter().fold(String::new(), |mut written, event| {
        let _ = write!(written, "event: {}\ndata: {event}\n\n", event["type"].as_str().unwrap_or_default());
        written
    })
}

#[tokio::test]
async fn a_stream_is_read_back_and_a_paused_answer_goes_on_from_its_blocks() {
    let first = events(&[
        json!({ "type": "message_start", "message": { "id": "msg_1", "usage": { "input_tokens": 10, "cache_read_input_tokens": 100, "output_tokens": 1 } } }),
        json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "thinking", "thinking": "", "signature": "" } }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "thinking_delta", "thinking": "search first" } }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "signature_delta", "signature": "sig" } }),
        json!({ "type": "content_block_stop", "index": 0 }),
        json!({ "type": "content_block_start", "index": 1, "content_block": { "type": "server_tool_use", "id": "srv_1", "name": "web_search", "input": {} } }),
        json!({ "type": "content_block_delta", "index": 1, "delta": { "type": "input_json_delta", "partial_json": "{\"query\":\"rust\"}" } }),
        json!({ "type": "content_block_stop", "index": 1 }),
        json!({ "type": "content_block_start", "index": 2, "content_block": { "type": "web_search_tool_result", "tool_use_id": "srv_1", "content": [{ "type": "web_search_result", "url": "https://rust-lang.org", "title": "Rust" }] } }),
        json!({ "type": "content_block_stop", "index": 2 }),
        json!({ "type": "message_delta", "delta": { "stop_reason": "pause_turn" }, "usage": { "output_tokens": 20, "server_tool_use": { "web_search_requests": 1 } } }),
        json!({ "type": "message_stop" }),
    ]);
    let second = events(&[
        json!({ "type": "message_start", "message": { "id": "msg_2", "usage": { "input_tokens": 300, "output_tokens": 1 } } }),
        json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "text", "text": "" } }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": "Rust is " } }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "citations_delta", "citation": { "type": "web_search_result_location", "url": "https://rust-lang.org", "title": "Rust", "cited_text": "fast" } } }),
        json!({ "type": "content_block_delta", "index": 0, "delta": { "type": "text_delta", "text": "fast." } }),
        json!({ "type": "content_block_stop", "index": 0 }),
        json!({ "type": "content_block_start", "index": 1, "content_block": { "type": "tool_use", "id": "toolu_1", "name": "read", "input": {} } }),
        json!({ "type": "content_block_delta", "index": 1, "delta": { "type": "input_json_delta", "partial_json": "{\"path\":" } }),
        json!({ "type": "content_block_delta", "index": 1, "delta": { "type": "input_json_delta", "partial_json": "\"b\"}" } }),
        json!({ "type": "content_block_stop", "index": 1 }),
        json!({ "type": "message_delta", "delta": { "stop_reason": "tool_use" }, "usage": { "output_tokens": 15 } }),
        json!({ "type": "message_stop" }),
    ]);
    let answers = Arc::new(Answers {
        left: Mutex::new(VecDeque::from([(200, vec![], first), (200, vec![], second)])),
        sent: Mutex::new(Vec::new()),
    });
    let mut call = support::call(support::Provider::streaming(&[]));
    call.transport = answers.clone();
    let model = claude();
    let source = ANTHROPIC.stream(&call, &model, &conversation()).await.expect("streaming");
    let (_, response) = support::run(TextStream::new(source, &model, 0, Arc::new(|| "minted".into()))).await;
    insta::assert_json_snapshot!("anthropic-stream", response);
    // The model went on from the blocks of its answer so far, sent back as its turn.
    let sent = answers.sent.lock().expect("held");
    let last = sent[1]["messages"].as_array().expect("messages").last().expect("a turn").clone();
    assert_eq!(last["role"], "assistant");
    assert_eq!(
        last["content"][0],
        json!({ "type": "thinking", "thinking": "search first", "signature": "sig" })
    );
    assert_eq!(last["content"][1]["input"], json!({ "query": "rust" }));
}

#[tokio::test]
async fn an_account_whose_unified_limits_are_spent_has_used_its_quota_till_their_reset() {
    let answers = Arc::new(Answers {
        left: Mutex::new(VecDeque::from([(
            429,
            vec![
                ("anthropic-ratelimit-unified-status", "rejected"),
                ("anthropic-ratelimit-unified-reset", "1800000000"),
                ("anthropic-ratelimit-unified-representative-claim", "five_hour"),
            ],
            r#"{"type":"error","error":{"type":"rate_limit_error","message":"limit reached"}}"#.to_owned(),
        )])),
        sent: Mutex::new(Vec::new()),
    });
    let mut call = support::call(support::Provider::streaming(&[]));
    call.transport = answers;
    call.credential = None;
    let Err(failure) = CLAUDE.stream(&call, &claude(), &conversation()).await else {
        panic!("refused")
    };
    assert_eq!(
        (failure.kind, failure.reset_at_ms, failure.window_minutes),
        (FailureKind::Quota, Some(1_800_000_000_000), Some(300))
    );
}
