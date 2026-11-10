//! Chat Completions: a conversation rendered as it takes it, Z.ai's dialect, its chunks read back, and a server's
//! listing of its models.

mod support;

use std::sync::Arc;

use octoii_ai::catalogue::Reasoning as ReasoningFacts;
use octoii_ai::driver::Driver;
use octoii_ai::drivers::{OLLAMA, ZAI};
use octoii_ai::protocols::chat_completions;
use octoii_ai::stream::TextStream;
use octoii_ai::vocabulary::{
    FileRef, FunctionCall, FunctionCallOutput, ImageDetail, Incomplete, InputContent, Item, Message, Origin, OutputContent, Reasoning,
    ReasoningConfig, ReasoningEffort, Request, ResponseStatus, Tool, ToolChoice,
};
use serde_json::json;
use support::Provider;

fn text(words: &str) -> Vec<InputContent> {
    vec![InputContent::text(words)]
}

fn conversation() -> Request {
    let own = Origin {
        model: "ollama/qwen3".into(),
        provider_metadata: None,
    };
    Request {
        model: "ollama/qwen3".into(),
        instructions: Some("Be brief.".into()),
        input: vec![
            Item::Message(Message::Developer {
                content: text("You work in /repo."),
            }),
            Item::Message(Message::User {
                content: text("look at a"),
            }),
            Item::Reasoning(Reasoning {
                id: None,
                summary: vec![],
                content: vec!["reading it".into()],
                encrypted_content: None,
                signature: None,
                status: None,
                origin: own.clone(),
            }),
            Item::Message(Message::Assistant {
                id: None,
                content: vec![OutputContent::OutputText {
                    text: "Reading.".into(),
                    annotations: vec![],
                }],
                status: None,
                origin: Some(own),
            }),
            Item::FunctionCall(FunctionCall {
                id: None,
                call_id: "call_1".into(),
                name: "read".into(),
                arguments: r#"{"path":"a"}"#.into(),
                status: None,
                origin: None,
            }),
            Item::FunctionCallOutput(FunctionCallOutput {
                call_id: "call_1".into(),
                output: vec![
                    InputContent::text("an image"),
                    InputContent::InputImage {
                        file: FileRef {
                            reference: "sha256:image".into(),
                            media_type: "image/png".into(),
                            filename: None,
                        },
                        detail: ImageDetail::Auto,
                    },
                ],
                is_error: false,
            }),
            Item::Message(Message::User {
                content: text("what is it?"),
            }),
            Item::Message(Message::Developer {
                content: text("the date changed"),
            }),
        ],
        tools: vec![
            Tool::Function {
                name: "read".into(),
                description: "Reads a file.".into(),
                parameters: json!({ "type": "object" }),
            },
            Tool::WebSearch,
        ],
        tool_choice: Some(ToolChoice::Required),
        reasoning: Some(ReasoningConfig {
            effort: ReasoningEffort::Low,
        }),
        format: None,
        max_output_tokens: Some(1000),
        prompt_cache_key: None,
    }
}

#[test]
fn a_conversation_goes_as_chat_completions_takes_it() {
    let call = support::call(Provider::streaming(&[]));
    let body = chat_completions::render(&call, &support::model("ollama", "qwen3"), &conversation(), &OLLAMA.dialect).expect("rendered");
    insta::assert_json_snapshot!("chat-completions", body);
}

#[test]
fn zai_switches_its_models_thinking_and_takes_a_tool_choice_of_auto_alone() {
    let call = support::call(Provider::streaming(&[]));
    let model = octoii_ai::catalogue::Model {
        reasoning: Some(ReasoningFacts {
            efforts: None,
            default_effort: None,
            mandatory: false,
        }),
        ..support::model("zai", "glm-5.3")
    };
    let body = chat_completions::render(&call, &model, &conversation(), &ZAI.dialect).expect("rendered");
    assert_eq!(body["thinking"], json!({ "type": "enabled" }));
    assert_eq!(body["tool_choice"], "auto");
    let off = Request {
        reasoning: Some(ReasoningConfig {
            effort: ReasoningEffort::None,
        }),
        ..conversation()
    };
    let body = chat_completions::render(&call, &model, &off, &ZAI.dialect).expect("rendered");
    assert_eq!(body["thinking"], json!({ "type": "disabled" }));
    assert_eq!(body.get("reasoning_effort"), None);
}

#[tokio::test]
async fn chunks_are_read_back_into_the_vocabulary_reasoning_text_and_calls_each_an_item() {
    let mut pieces: Vec<String> = [
        json!({ "id": "chat_1", "choices": [{ "delta": { "reasoning_content": "think" } }] }),
        json!({ "id": "chat_1", "choices": [{ "delta": { "content": "Read" } }] }),
        json!({ "id": "chat_1", "choices": [{ "delta": { "content": "ing." } }] }),
        json!({ "id": "chat_1", "choices": [{ "delta": { "tool_calls": [{ "index": 0, "id": "call_2", "function": { "name": "read", "arguments": "{\"pa" } }] } }] }),
        json!({ "id": "chat_1", "choices": [{ "delta": { "tool_calls": [{ "index": 0, "function": { "arguments": "th\":\"b\"}" } }] }, "finish_reason": "length" }] }),
        json!({ "id": "chat_1", "choices": [], "usage": { "prompt_tokens": 50, "completion_tokens": 9, "prompt_tokens_details": { "cached_tokens": 40 } } }),
    ]
    .iter()
    .map(|chunk| format!("data: {chunk}\n\n"))
    .collect();
    pieces.push("data: [DONE]\n\n".into());
    let call = support::call(Provider::answering(200, pieces));
    let model = support::model("ollama", "qwen3");
    let source = OLLAMA.stream(&call, &model, &conversation()).await.expect("streaming");
    let (_, response) = support::run(TextStream::new(source, &model, 0, Arc::new(|| "minted".into()))).await;
    assert_eq!(
        (response.status, response.incomplete),
        (ResponseStatus::Incomplete, Some(Incomplete::MaxOutputTokens))
    );
    insta::assert_json_snapshot!("chat-completions-stream", response.output);
    assert_eq!(
        response.usage.map(|usage| (usage.input_tokens, usage.cached_tokens)),
        Some((50, 40))
    );
}

#[tokio::test]
async fn a_server_lists_the_models_it_holds_and_an_api_listing_none() {
    let call = support::call(Provider::answering(
        200,
        vec![r#"{"data":[{"id":"qwen3"},{"id":"llama4"}]}"#.into()],
    ));
    let listed = OLLAMA.list(&call).await.expect("listed").expect("a listing");
    assert_eq!(
        listed.iter().map(octoii_ai::catalogue::Model::reference).collect::<Vec<_>>(),
        ["test/qwen3", "test/llama4"]
    );
    assert!(ZAI.list(&call).await.expect("listed").is_none());
}
