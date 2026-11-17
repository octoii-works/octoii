//! Delivered files stay references across provider protocols, never model file inputs.
mod support;

use octoii_ai::protocols::{anthropic, chat_completions, google, responses};
use octoii_ai::vocabulary::{FileRef, FunctionCall, FunctionCallOutput, InputContent, Item, Message, Request};

#[test]
fn every_provider_describes_resource_links_without_opening_the_file() {
    let provider = support::Provider::answering(200, vec![]);
    let mut call = support::call(provider);
    call.files = None;
    let model = support::model("test", "model");
    let link = InputContent::ResourceLink {
        file: FileRef {
            reference: "sha256:not-readable".into(),
            media_type: "application/zip".into(),
            filename: Some("archive.zip".into()),
        },
        size: 123,
    };
    let request = Request {
        model: model.reference(),
        input: vec![
            Item::Message(Message::User {
                content: vec![link.clone()],
            }),
            Item::FunctionCall(FunctionCall {
                id: None,
                call_id: "call".into(),
                name: "present_file".into(),
                arguments: "{}".into(),
                status: None,
                origin: None,
            }),
            Item::FunctionCallOutput(FunctionCallOutput {
                call_id: "call".into(),
                output: vec![link],
                is_error: false,
            }),
        ],
        instructions: None,
        tools: vec![],
        tool_choice: None,
        reasoning: None,
        format: None,
        max_output_tokens: Some(1024),
        prompt_cache_key: None,
    };
    let bodies = [
        responses::render(
            &call,
            &model,
            &request,
            &responses::Dialect {
                developer_role: responses::DeveloperRole::Developer,
                cache_key: false,
                encrypted_reasoning: false,
                takes_text_files: true,
                failure_kind: |_, _| None,
            },
        )
        .expect("Responses link"),
        chat_completions::render(
            &call,
            &model,
            &request,
            &chat_completions::Dialect {
                thinking: chat_completions::Thinking::ReasoningEffort,
                tool_choices: true,
            },
        )
        .expect("Chat Completions link"),
        anthropic::render(&call, &model, &request).expect("Anthropic link").0,
        google::render(&call, &model, &request).expect("Google link"),
    ];
    for body in bodies {
        let encoded = body.to_string();
        assert!(
            encoded.contains("archive.zip") && encoded.contains("sha256:not-readable"),
            "{encoded}"
        );
        assert!(encoded.contains("contents not included"), "{encoded}");
        assert!(!encoded.contains("base64") && !encoded.contains("data:"), "{encoded}");
    }
}
