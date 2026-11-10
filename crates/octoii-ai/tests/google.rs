//! Google's Interactions API: a conversation rendered as steps, and an interaction's steps read back.

mod support;

use std::sync::Arc;

use octoii_ai::driver::Driver;
use octoii_ai::drivers::GOOGLE;
use octoii_ai::protocols::google;
use octoii_ai::stream::TextStream;
use octoii_ai::vocabulary::{
    FailureKind, FunctionCall, FunctionCallOutput, Incomplete, InputContent, Item, Message, Origin, OutputContent, Reasoning,
    ReasoningConfig, ReasoningEffort, Request, ResponseStatus, Tool, ToolChoice,
};
use serde_json::json;
use support::Provider;

fn own(metadata: serde_json::Value) -> Origin {
    Origin {
        model: "google/gemini-4-argon".into(),
        provider_metadata: Some(metadata),
    }
}

fn conversation() -> Request {
    Request {
        model: "google/gemini-4-argon".into(),
        instructions: Some("Be brief.".into()),
        input: vec![
            Item::Message(Message::Developer {
                content: vec![InputContent::text("You work in /repo.")],
            }),
            Item::Message(Message::User {
                content: vec![InputContent::text("read a")],
            }),
            Item::Reasoning(Reasoning {
                id: None,
                summary: vec!["to read".into()],
                content: vec![],
                encrypted_content: None,
                signature: Some("thought-sig".into()),
                status: None,
                origin: own(json!({})),
            }),
            Item::FunctionCall(FunctionCall {
                id: None,
                call_id: "minted-1".into(),
                name: "read".into(),
                arguments: r#"{"path":"a"}"#.into(),
                status: None,
                origin: Some(own(json!({ "google": { "id": "g1" } }))),
            }),
            Item::FunctionCallOutput(FunctionCallOutput {
                call_id: "minted-1".into(),
                output: vec![InputContent::text("# A")],
                is_error: false,
            }),
            Item::Message(Message::Developer {
                content: vec![InputContent::text("the date changed")],
            }),
            Item::Message(Message::Assistant {
                id: None,
                content: vec![OutputContent::OutputText {
                    text: "A title.".into(),
                    annotations: vec![],
                }],
                status: None,
                origin: None,
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
        tool_choice: Some(ToolChoice::Function { name: "read".into() }),
        reasoning: Some(ReasoningConfig {
            effort: ReasoningEffort::Medium,
        }),
        format: None,
        max_output_tokens: Some(2048),
        prompt_cache_key: None,
    }
}

#[test]
fn a_conversation_goes_to_google_as_steps_its_calls_by_geminis_ids_and_its_updates_in_the_persons_turn() {
    let call = support::call(Provider::streaming(&[]));
    let body = google::render(&call, &support::model("google", "gemini-4-argon"), &conversation()).expect("rendered");
    insta::assert_json_snapshot!("google", body);
}

#[test]
fn an_effort_above_googles_highest_thinking_level_is_refused() {
    let call = support::call(Provider::streaming(&[]));
    let request = Request {
        reasoning: Some(ReasoningConfig {
            effort: ReasoningEffort::Max,
        }),
        ..conversation()
    };
    let refused = google::render(&call, &support::model("google", "gemini-4-argon"), &request).expect_err("refused");
    assert_eq!(refused.kind, FailureKind::InvalidRequest);
}

#[tokio::test]
async fn an_interactions_steps_come_back_as_items_each_ended_as_the_next_begins_or_the_end_cuts_it() {
    let events = [
        json!({ "event_type": "interaction.created", "interaction": { "id": "int_1" } }),
        json!({ "event_type": "step.start", "index": 0, "step": { "type": "thought" } }),
        json!({ "event_type": "step.delta", "index": 0, "delta": { "type": "thought_summary", "content": { "type": "text", "text": "plan" } } }),
        json!({ "event_type": "step.delta", "index": 0, "delta": { "type": "thought_signature", "signature": "s1" } }),
        json!({ "event_type": "step.stop", "index": 0 }),
        json!({ "event_type": "step.start", "index": 1, "step": { "type": "function_call", "id": "g2", "name": "read", "arguments": {} } }),
        json!({ "event_type": "step.delta", "index": 1, "delta": { "type": "arguments_delta", "arguments": "{\"path\":\"b\"}" } }),
        json!({ "event_type": "step.stop", "index": 1 }),
        json!({ "event_type": "step.start", "index": 2, "step": { "type": "model_output", "content": [] } }),
        json!({ "event_type": "step.delta", "index": 2, "delta": { "type": "text", "text": "Reading b" } }),
        json!({ "event_type": "interaction.completed", "interaction": { "status": "incomplete", "usage": { "total_input_tokens": 80, "total_cached_tokens": 60, "total_output_tokens": 12, "total_thought_tokens": 8 } } }),
    ];
    let call = support::call(Provider::streaming(&events));
    let model = support::model("google", "gemini-4-argon");
    let source = GOOGLE.stream(&call, &model, &conversation()).await.expect("streaming");
    let (_, response) = support::run(TextStream::new(source, &model, 0, Arc::new(|| "minted".into()))).await;
    assert_eq!(
        (response.status, response.incomplete),
        (ResponseStatus::Incomplete, Some(Incomplete::MaxOutputTokens))
    );
    assert_eq!(
        response.usage.map(|usage| (usage.output_tokens, usage.reasoning_tokens)),
        Some((20, 8))
    );
    insta::assert_json_snapshot!("google-stream", response.output);
    let failed = support::call(Provider::streaming(&[
        json!({ "event_type": "interaction.completed", "interaction": { "status": "failed", "errors": [{ "code": "RESOURCE_EXHAUSTED", "message": "quota" }] } }),
    ]));
    let source = GOOGLE.stream(&failed, &model, &conversation()).await.expect("streaming");
    let (_, response) = support::run(TextStream::new(source, &model, 0, Arc::new(|| "minted".into()))).await;
    assert_eq!(response.failure.map(|failure| failure.kind), Some(FailureKind::RateLimit));
}
