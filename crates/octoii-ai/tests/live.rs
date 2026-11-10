//! Requests of a real model, run only when asked — `cargo test -p octoii-ai --test live -- --ignored` — with the key in
//! the environment: OpenRouter's DeepSeek V4.1 Flash, as the cheapest model that reasons and calls tools.

use std::sync::Arc;

use bytes::Bytes;
use octoii_ai::catalogue;
use octoii_ai::driver::{Call, Driver, Settings};
use octoii_ai::drivers::OPENROUTER;
use octoii_ai::host::{Credential, Credentials, Files, NoSuchFile, Unavailable};
use octoii_ai::stream::{Event, TextStream};
use octoii_ai::transport::Network;
use octoii_ai::vocabulary::{
    FunctionCallOutput, InputContent, Item, Message, OutputContent, ReasoningConfig, ReasoningEffort, Request, Response, ResponseStatus,
    Tool, ToolChoice,
};
use serde_json::json;

struct NoFiles;

/// The key in the environment, as the host's credentials.
struct Environment;

impl Credentials for Environment {
    fn read(&self, _: &str) -> Result<Option<Credential>, Unavailable> {
        Ok(Some(Credential::ApiKey {
            api_key: std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY is set"),
        }))
    }

    fn keep(&self, _: &str, _: &Credential) -> Result<(), Unavailable> {
        Ok(())
    }

    fn forget(&self, _: &str) -> Result<(), Unavailable> {
        Ok(())
    }
}

impl Files for NoFiles {
    fn read(&self, reference: &str) -> Result<Bytes, NoSuchFile> {
        Err(NoSuchFile(reference.into()))
    }
}

fn call() -> Call {
    Call {
        provider: "openrouter".into(),
        settings: Settings::default(),
        credentials: Arc::new(Environment),
        transport: Arc::new(Network::default()),
        files: Arc::new(NoFiles),
        now_ms: 0,
    }
}

fn model() -> catalogue::Model {
    let mut model = catalogue::bundled()
        .into_iter()
        .find(|model| model.provider == "deepseek" && model.id.contains("flash"))
        .expect("the catalogue lists DeepSeek's flash model");
    "deepseek/deepseek-v4.1-flash".clone_into(&mut model.id);
    model.provider = "openrouter".into();
    model
}

async fn ask(request: &Request) -> Response {
    let call = call();
    let model = model();
    let source = OPENROUTER.stream(&call, &model, request).await.expect("the request went");
    let mut stream = TextStream::new(source, &model, 0, Arc::new(|| "minted".into()));
    let mut deltas = 0;
    loop {
        match stream.next().await.expect("a response") {
            Event::Delta { .. } => deltas += 1,
            Event::Finished(response) => {
                eprintln!("{deltas} deltas; {}", serde_json::to_string_pretty(&response).expect("JSON"));
                return response;
            }
            _ => {}
        }
    }
}

fn user(text: &str) -> Item {
    Item::Message(Message::User {
        content: vec![InputContent::text(text)],
    })
}

#[tokio::test]
#[ignore = "asks a real model, at a cost"]
async fn a_real_model_answers_calls_a_tool_and_goes_on_from_its_result() {
    let mut request = Request {
        model: String::new(),
        instructions: Some("You are terse.".into()),
        input: vec![user("Say the word ok and nothing else.")],
        tools: vec![],
        tool_choice: None,
        reasoning: Some(ReasoningConfig {
            effort: ReasoningEffort::Low,
        }),
        format: None,
        max_output_tokens: None,
        prompt_cache_key: Some("octoii-ai-live".into()),
    };
    let answered = ask(&request).await;
    assert_eq!(answered.status, ResponseStatus::Completed);
    let words: String = answered
        .output
        .iter()
        .filter_map(|item| match item {
            Item::Message(Message::Assistant { content, .. }) => Some(content),
            _ => None,
        })
        .flatten()
        .filter_map(|part| match part {
            OutputContent::OutputText { text, .. } => Some(text.as_str()),
            OutputContent::Refusal { .. } => None,
        })
        .collect();
    assert!(words.to_lowercase().contains("ok"), "{words}");
    assert!(answered.usage.is_some_and(|usage| usage.input_tokens > 0));

    request.input = vec![user("Read the file notes.md with the read tool.")];
    request.tools = vec![Tool::Function {
        name: "read".into(),
        description: "Reads a file by its path.".into(),
        parameters: json!({ "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] }),
    }];
    request.tool_choice = Some(ToolChoice::Required);
    let called = ask(&request).await;
    let call = called
        .output
        .iter()
        .find_map(|item| match item {
            Item::FunctionCall(call) => Some(call.clone()),
            _ => None,
        })
        .expect("a call of read");
    assert_eq!(call.name, "read");
    assert!(call.arguments.contains("notes.md"), "{}", call.arguments);

    request.input.extend(called.output.clone());
    request.input.push(Item::FunctionCallOutput(FunctionCallOutput {
        call_id: call.call_id,
        output: vec![InputContent::text("# Notes\nThe secret word is lantern.")],
        is_error: false,
    }));
    request.tool_choice = Some(ToolChoice::None);
    let answered = ask(&request).await;
    assert!(
        serde_json::to_string(&answered.output)
            .expect("JSON")
            .to_lowercase()
            .contains("lantern")
    );
}
