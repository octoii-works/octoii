//! The vocabulary as a log keeps it: each item its JSON, read back as it was written.

use octoii_ai::vocabulary::{
    FileRef, FunctionCall, FunctionCallOutput, InputContent, Item, Message, Origin, OutputContent, Reasoning, ReasoningEffort, Response,
    ResponseStatus, Usage,
};
use serde_json::json;

fn round_trip(item: &Item, expected: &serde_json::Value) {
    assert_eq!(&serde_json::to_value(item).expect("written"), expected);
    assert_eq!(&serde_json::from_value::<Item>(expected.clone()).expect("read"), item);
}

#[test]
fn items_are_written_as_their_type_and_role_say_and_read_back_as_they_were() {
    let image = FileRef {
        reference: "sha256:ab".into(),
        media_type: "image/png".into(),
        filename: None,
    };
    round_trip(
        &Item::Message(Message::User {
            content: vec![
                InputContent::text("look"),
                InputContent::InputImage {
                    file: image,
                    detail: octoii_ai::vocabulary::ImageDetail::Auto,
                },
            ],
        }),
        &json!({"type": "message", "role": "user", "content": [
            {"type": "input_text", "text": "look"},
            {"type": "input_image", "file": {"ref": "sha256:ab", "media_type": "image/png"}, "detail": "auto"},
        ]}),
    );
    let origin = Origin {
        model: "anthropic/claude-opus-5-5".into(),
        provider_metadata: None,
    };
    round_trip(
        &Item::Message(Message::Assistant {
            id: None,
            content: vec![OutputContent::OutputText {
                text: "done".into(),
                annotations: vec![],
            }],
            status: None,
            origin: Some(origin.clone()),
        }),
        &json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "done"}],
            "origin": {"model": "anthropic/claude-opus-5-5"}}),
    );
    round_trip(
        &Item::FunctionCall(FunctionCall {
            id: None,
            call_id: "c1".into(),
            name: "read".into(),
            arguments: r#"{"path":"a"}"#.into(),
            status: None,
            origin: None,
        }),
        &json!({"type": "function_call", "call_id": "c1", "name": "read", "arguments": "{\"path\":\"a\"}"}),
    );
    round_trip(
        &Item::FunctionCallOutput(FunctionCallOutput {
            call_id: "c1".into(),
            output: vec![InputContent::text("no such file")],
            is_error: true,
        }),
        &json!({"type": "function_call_output", "call_id": "c1", "output": [{"type": "input_text", "text": "no such file"}], "is_error": true}),
    );
    round_trip(
        &Item::Reasoning(Reasoning {
            id: Some("r1".into()),
            summary: vec!["thought".into()],
            content: vec![],
            encrypted_content: Some("e".into()),
            signature: None,
            status: None,
            origin,
        }),
        &json!({"type": "reasoning", "id": "r1", "summary": ["thought"], "encrypted_content": "e",
            "origin": {"model": "anthropic/claude-opus-5-5"}}),
    );
    round_trip(
        &Item::ConfigurationUpdate(octoii_ai::vocabulary::ConfigurationUpdate {
            effort: ReasoningEffort::High,
        }),
        &json!({"type": "configuration_update", "effort": "high"}),
    );
}

#[test]
fn a_response_reads_back_as_it_was_written_and_a_failure_says_whether_asking_again_may_help() {
    let response = Response {
        id: "resp_1".into(),
        model: "deepseek/deepseek-v4.1-flash".into(),
        status: ResponseStatus::Completed,
        output: vec![],
        usage: Some(Usage {
            input_tokens: 10,
            cached_tokens: 4,
            output_tokens: 2,
            cost_nanodollars: Some(1200),
            ..Usage::default()
        }),
        failure: None,
        incomplete: None,
        context_tokens: None,
    };
    let written = serde_json::to_string(&response).expect("written");
    assert_eq!(serde_json::from_str::<Response>(&written).expect("read"), response);
    let failure: octoii_ai::vocabulary::Failure =
        serde_json::from_value(json!({"kind": "rate_limit", "message": "slow down", "retry_after_ms": 2000})).expect("read");
    assert!(failure.retryable());
    let overflow: octoii_ai::vocabulary::Failure =
        serde_json::from_value(json!({"kind": "context_overflow", "message": "too long"})).expect("read");
    assert!(!overflow.retryable());
}
