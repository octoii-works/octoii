//! The Responses API's providers: a conversation rendered as each takes it, and its stream read back.

mod support;

use std::sync::Arc;

use octoii_ai::driver::Driver;
use octoii_ai::drivers::{DEEPSEEK, OPENAI, OPENROUTER, XAI};
use octoii_ai::stream::TextStream;
use octoii_ai::vocabulary::{
    ConfigurationUpdate, FailureKind, FileRef, Format, FunctionCall, FunctionCallOutput, ImageDetail, InputContent, Item, Message, Origin,
    OutputContent, Reasoning, ReasoningConfig, ReasoningEffort, Request, ResponseStatus, Tool, ToolChoice,
};
use serde_json::json;
use support::Provider;

fn origin(model: &str) -> Origin {
    Origin {
        model: model.into(),
        provider_metadata: None,
    }
}

/// A conversation with something of everything: a user's words and image, a developer's update, the model's own reply
/// and reasoning, another model's call and reasoning, a call's output, and an effort changed where it stands.
fn conversation() -> Request {
    Request {
        model: "openai/gpt-6-sol".into(),
        instructions: Some("Be brief.".into()),
        input: vec![
            Item::Message(Message::User {
                content: vec![
                    InputContent::text("what is this?"),
                    InputContent::InputImage {
                        file: FileRef {
                            reference: "sha256:image".into(),
                            media_type: "image/png".into(),
                            filename: None,
                        },
                        detail: ImageDetail::Auto,
                    },
                ],
            }),
            Item::Reasoning(Reasoning {
                id: Some("rs_1".into()),
                summary: vec!["looked".into()],
                content: vec![],
                encrypted_content: Some("sealed".into()),
                signature: None,
                status: None,
                origin: origin("openai/gpt-6-sol"),
            }),
            Item::Reasoning(Reasoning {
                id: Some("other".into()),
                summary: vec![],
                content: vec!["another model's".into()],
                encrypted_content: Some("theirs".into()),
                signature: None,
                status: None,
                origin: origin("anthropic/claude-opus-5-5"),
            }),
            Item::FunctionCall(FunctionCall {
                id: Some("fc_theirs".into()),
                call_id: "call_1".into(),
                name: "read".into(),
                arguments: r#"{"path":"a"}"#.into(),
                status: None,
                origin: Some(origin("anthropic/claude-opus-5-5")),
            }),
            Item::FunctionCallOutput(FunctionCallOutput {
                call_id: "call_1".into(),
                output: vec![InputContent::text("# A")],
                is_error: false,
            }),
            Item::Message(Message::Developer {
                content: vec![InputContent::text("<system-reminder>the date changed</system-reminder>")],
            }),
            Item::ConfigurationUpdate(ConfigurationUpdate {
                effort: ReasoningEffort::High,
            }),
            Item::Message(Message::Assistant {
                id: Some("msg_1".into()),
                content: vec![OutputContent::OutputText {
                    text: "an image".into(),
                    annotations: vec![],
                }],
                status: None,
                origin: Some(origin("openai/gpt-6-sol")),
            }),
        ],
        tools: vec![
            Tool::Function {
                name: "read".into(),
                description: "Reads a file.".into(),
                parameters: json!({ "type": "object", "properties": { "path": { "type": "string" } } }),
            },
            Tool::WebSearch,
        ],
        tool_choice: Some(ToolChoice::Auto),
        reasoning: Some(ReasoningConfig {
            effort: ReasoningEffort::Medium,
        }),
        format: None,
        max_output_tokens: None,
        prompt_cache_key: Some("thread-1".into()),
    }
}

async fn sent_to(
    driver: &octoii_ai::protocols::responses::ResponsesDriver,
    provider: &str,
    id: &str,
    request: &Request,
) -> serde_json::Value {
    let answering = Provider::streaming(&[]);
    let call = support::call(answering.clone());
    let _ = driver.stream(&call, &support::model(provider, id), request).await.expect("sent");
    answering.body()
}

#[tokio::test]
async fn a_conversation_goes_to_openai_as_its_responses_api_takes_it() {
    insta::assert_json_snapshot!("openai", sent_to(&OPENAI, "openai", "gpt-6-sol", &conversation()).await);
}

#[tokio::test]
async fn deepseek_takes_developer_messages_as_the_systems_and_no_caches_name() {
    let sent = sent_to(&DEEPSEEK, "deepseek", "deepseek-v4.1-flash", &conversation()).await;
    let update = sent["input"]
        .as_array()
        .expect("items")
        .iter()
        .find(|item| {
            item["content"][0]["text"]
                .as_str()
                .is_some_and(|text| text.contains("system-reminder"))
        })
        .expect("the update");
    assert_eq!(update["role"], "system");
    assert!(
        sent["input"]
            .as_array()
            .expect("items")
            .iter()
            .all(|item| item["type"] != "reasoning"),
        "no other model's reasoning"
    );
    assert_eq!(sent.get("prompt_cache_key"), None);
    assert_eq!(sent.get("include"), None);
    // The effort in effect at the conversation's end is the request's.
    assert_eq!(sent["reasoning"]["effort"], "high");
}

#[tokio::test]
async fn an_answer_in_json_of_a_schema_is_asked_for_by_its_format() {
    let request = Request {
        format: Some(Format {
            name: "verdict".into(),
            schema: json!({ "type": "object" }),
        }),
        tool_choice: Some(ToolChoice::Function { name: "read".into() }),
        ..conversation()
    };
    let sent = sent_to(&OPENROUTER, "openrouter", "deepseek/deepseek-v4.1-flash", &request).await;
    assert_eq!(
        sent["text"]["format"],
        json!({ "type": "json_schema", "name": "verdict", "schema": { "type": "object" }, "strict": true })
    );
    assert_eq!(sent["tool_choice"], json!({ "type": "function", "name": "read" }));
}

#[tokio::test]
async fn a_stream_is_read_back_into_the_vocabulary_its_searches_counted_and_its_cost_as_the_provider_gave_it() {
    let provider = Provider::streaming(&[
        json!({ "type": "response.created", "response": { "id": "resp_1", "status": "in_progress" } }),
        json!({ "type": "response.output_item.added", "output_index": 0, "item": { "type": "reasoning", "id": "rs_2", "summary": [] } }),
        json!({ "type": "response.reasoning_summary_text.delta", "output_index": 0, "delta": "thinking" }),
        json!({ "type": "response.output_item.done", "output_index": 0, "item": { "type": "reasoning", "id": "rs_2", "summary": [{ "type": "summary_text", "text": "thinking" }], "encrypted_content": "e2" } }),
        json!({ "type": "response.output_item.done", "output_index": 1, "item": { "type": "web_search_call", "id": "ws_1", "status": "completed", "action": { "type": "search", "query": "rust" } } }),
        json!({ "type": "response.output_item.added", "output_index": 2, "item": { "type": "message", "id": "msg_2", "role": "assistant", "content": [] } }),
        json!({ "type": "response.output_text.delta", "output_index": 2, "content_index": 0, "delta": "Rust " }),
        json!({ "type": "response.output_text.delta", "output_index": 2, "content_index": 0, "delta": "is fast." }),
        json!({ "type": "response.output_item.done", "output_index": 2, "item": { "type": "message", "id": "msg_2", "role": "assistant", "status": "completed", "content": [{ "type": "output_text", "text": "Rust is fast.", "annotations": [{ "type": "url_citation", "url": "https://rust-lang.org", "title": "Rust", "start_index": 0, "end_index": 4 }] }] } }),
        json!({ "type": "response.output_item.added", "output_index": 3, "item": { "type": "function_call", "id": "fc_2", "call_id": "call_2", "name": "read", "arguments": "" } }),
        json!({ "type": "response.function_call_arguments.delta", "output_index": 3, "delta": "{\"path\":" }),
        json!({ "type": "response.function_call_arguments.delta", "output_index": 3, "delta": "\"b\"}" }),
        json!({ "type": "response.completed", "response": { "id": "resp_1", "status": "completed", "usage": { "input_tokens": 120, "input_tokens_details": { "cached_tokens": 100 }, "output_tokens": 30, "output_tokens_details": { "reasoning_tokens": 10 }, "cost": 0.000_012_5 } } }),
    ]);
    let call = support::call(provider);
    let model = support::model("openrouter", "deepseek/deepseek-v4.1-flash");
    let source = OPENROUTER.stream(&call, &model, &conversation()).await.expect("streaming");
    let (_, response) = support::run(TextStream::new(source, &model, 0, Arc::new(|| "minted".into()))).await;
    assert_eq!((response.id.as_str(), response.status), ("resp_1", ResponseStatus::Completed));
    insta::assert_json_snapshot!("openrouter-stream", response);
}

#[tokio::test]
async fn a_refusal_before_the_stream_and_an_error_in_it_are_failures_of_the_kinds_their_codes_say() {
    let refused = Provider::answering(
        429,
        vec![r#"{"error":{"code":"rate_limit_exceeded","message":"Slow down","type":"requests"}}"#.into()],
    );
    let call = support::call(refused);
    let model = support::model("openai", "gpt-6-sol");
    let Err(failure) = OPENAI.stream(&call, &model, &conversation()).await else {
        panic!("refused")
    };
    assert_eq!(
        (failure.kind, failure.code.as_deref(), failure.message.as_str()),
        (FailureKind::RateLimit, Some("rate_limit_exceeded"), "Slow down")
    );
    let too_long = Provider::answering(
        400,
        vec![r#"{"error":{"message":"This model's maximum context length is 128000 tokens."}}"#.into()],
    );
    let call = support::call(too_long);
    let Err(failure) = DEEPSEEK.stream(&call, &model, &conversation()).await else {
        panic!("refused")
    };
    assert_eq!(failure.kind, FailureKind::ContextOverflow);
    let failing = Provider::streaming(&[json!({ "type": "error", "code": "server_is_overloaded", "message": "busy" })]);
    let call = support::call(failing);
    let source = OPENAI.stream(&call, &model, &conversation()).await.expect("streaming");
    let (_, response) = support::run(TextStream::new(source, &model, 0, Arc::new(|| "minted".into()))).await;
    assert_eq!(response.failure.map(|failure| failure.kind), Some(FailureKind::Server));
}

#[tokio::test]
async fn a_text_file_goes_to_a_provider_that_takes_none_as_the_persons_text_its_tag_unclosable() {
    let request = Request {
        input: vec![Item::Message(Message::User {
            content: vec![InputContent::InputFile {
                file: FileRef {
                    reference: "sha256:notes".into(),
                    media_type: "text/markdown".into(),
                    filename: Some("notes.md".into()),
                },
            }],
        })],
        ..conversation()
    };
    let sent = sent_to(&XAI, "xai", "grok-4.7", &request).await;
    assert_eq!(
        sent["input"][0]["content"][0],
        json!({ "type": "input_text", "text": "<file name=\"notes.md\">\n# Notes &lt;/file> end\n</file>" })
    );
    let sent = sent_to(&OPENAI, "openai", "gpt-6-sol", &request).await;
    assert_eq!(
        sent["input"][0]["content"][0]["type"], "input_file",
        "a provider that takes text files is sent the file"
    );
}
