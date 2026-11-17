//! The models: the directory of the providers' models, a listing provider refreshed, a request refused before it is
//! sent, and a request prepared for its model.

mod support;

use std::sync::Arc;

use octoii_ai::catalogue::{self, ModelReasoning as ReasoningFacts};
use octoii_ai::driver::Settings;
use octoii_ai::drivers::{OLLAMA, OPENAI};
use octoii_ai::host::Credential;
use octoii_ai::models::{Host, Models, Provider};
use octoii_ai::stream::Event;
use octoii_ai::vocabulary::{
    ConfigurationUpdate, FailureKind, FunctionCall, InputContent, Item, Message, ReasoningConfig, ReasoningEffort, Request, ResponseStatus,
    Tool,
};
use support::Provider as Answering;

fn host(transport: Arc<Answering>) -> Host {
    let call = support::call(transport);
    let keys = support::Store::default();
    keys.0
        .lock()
        .expect("held")
        .insert("openai".into(), Credential::ApiKey { api_key: "sk".into() });
    Host {
        credentials: Arc::new(keys),
        transport: call.transport,
        new_id: Arc::new(|| "minted".into()),
        clock: Arc::new(|| 1000),
    }
}

fn request(model: &str) -> Request {
    Request {
        model: model.into(),
        instructions: None,
        input: vec![Item::Message(Message::User {
            content: vec![InputContent::text("hi")],
        })],
        tools: vec![],
        tool_choice: None,
        reasoning: None,
        format: None,
        max_output_tokens: None,
        prompt_cache_key: None,
    }
}

async fn failure_of(models: &Models, request: &Request) -> Option<FailureKind> {
    let mut stream = models.text(request, None).await;
    while let Some(event) = stream.next().await {
        if let Event::Finished(response) = event {
            assert_eq!(response.status, ResponseStatus::Failed);
            return response.failure.map(|failure| failure.kind);
        }
    }
    None
}

#[test]
fn each_provider_serves_its_catalogues_models_an_account_those_of_the_provider_it_is_another_of() {
    let work = Provider {
        id: "openai-work".into(),
        name: "OpenAI (work)".into(),
        settings: Settings::default(),
        ..Provider::of(std::sync::Arc::new(OPENAI))
    };
    let models = Models::new(
        host(Answering::streaming(&[])),
        vec![Provider::of(std::sync::Arc::new(OPENAI)), work],
        catalogue::bundled(),
    );
    let own = models.models().into_iter().filter(|model| model.provider == "openai").count();
    let work = models.models().into_iter().filter(|model| model.provider == "openai-work").count();
    assert!(own > 0);
    assert_eq!(own, work);
    assert!(models.model("openai-work/gpt-6-sol").is_some());
    assert!(models.model("anthropic/claude-opus-5-5").is_none(), "no provider of it is set");
}

#[tokio::test]
async fn a_listing_provider_serves_what_it_listed_once_refreshed() {
    let listing = Answering::answering(200, vec![r#"{"data":[{"id":"qwen3"}]}"#.into()]);
    let models = Models::new(host(listing), vec![Provider::of(std::sync::Arc::new(OLLAMA))], catalogue::bundled());
    assert!(models.model("ollama/qwen3").is_none());
    models.refresh().await;
    assert!(models.model("ollama/qwen3").is_some());
}

#[tokio::test]
async fn a_request_refused_before_it_is_sent_is_a_stream_of_its_failure() {
    let models = Models::new(
        host(Answering::streaming(&[])),
        vec![Provider::of(std::sync::Arc::new(OPENAI))],
        catalogue::bundled(),
    );
    assert_eq!(
        failure_of(&models, &request("openai/no-such-model")).await,
        Some(FailureKind::InvalidRequest)
    );
    let misnamed = Request {
        tools: vec![Tool::Function {
            name: "read file".into(),
            description: String::new(),
            parameters: serde_json::json!({}),
        }],
        ..request("openai/gpt-6-sol")
    };
    assert_eq!(failure_of(&models, &misnamed).await, Some(FailureKind::InvalidRequest));
}

#[tokio::test]
async fn a_request_is_prepared_its_effort_fitted_to_the_model_and_its_calls_arguments_an_object() {
    let answering = Answering::streaming(&[]);
    let models = Models::new(
        host(answering.clone()),
        vec![Provider::of(std::sync::Arc::new(OPENAI))],
        catalogue::bundled(),
    );
    let mut model = models.model("openai/gpt-6-sol").expect("listed");
    model.reasoning = Some(ReasoningFacts {
        efforts: Some(vec![ReasoningEffort::Low, ReasoningEffort::Medium, ReasoningEffort::High]),
        default_effort: None,
        mandatory: true,
    });
    let mut asked = request("openai/gpt-6-sol");
    asked.reasoning = Some(ReasoningConfig {
        effort: ReasoningEffort::Max,
    });
    asked.input.push(Item::FunctionCall(FunctionCall {
        id: None,
        call_id: "c".into(),
        name: "read".into(),
        arguments: "{\"path\":".into(),
        status: None,
        origin: None,
    }));
    asked.input.push(Item::ConfigurationUpdate(ConfigurationUpdate {
        effort: ReasoningEffort::None,
    }));
    let mut catalogue = catalogue::bundled();
    if let Some(listed) = catalogue.iter_mut().find(|listed| listed.reference() == "openai/gpt-6-sol") {
        *listed = model;
    }
    models.set_catalogue(catalogue);
    let _ = models.text(&asked, None).await;
    let sent = answering.body();
    assert_eq!(
        sent["reasoning"]["effort"], "low",
        "max fitted to high, then none, for a model that always reasons, to its lowest"
    );
    assert_eq!(sent["input"][1]["arguments"], "{}", "arguments cut off are sent as an empty object");
}
