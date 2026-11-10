//! The models, as one handle every caller shares: the directory of its providers' models — which models each serves,
//! from its listing or else the catalogue, each with the catalogue's facts — and each request sent through what every
//! driver shares: its model found, the request checked against it and prepared for it, the driver's stream made
//! whole. The directory changes as providers list their models anew and as the providers are set again; a reader keeps
//! the directory it read, so a change never pulls the floor from under a request.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::catalogue::{self, Model};
use crate::driver::{Call, Driver, Settings};
use crate::host::{Credentials, Files};
use crate::stream::{Mint, Source, TextStream};
use crate::transport::Transport;
use crate::vocabulary::{Failure, FailureKind, FunctionCall, InputContent, Item, Message, ReasoningConfig, ReasoningEffort, Request, Tool};

/// A provider models are reached through: its id — the first part of its models' references, and the key of its
/// credential — its name for people, its driver, its settings, and the provider whose catalogue entries its models
/// take, as an account added of a provider takes that provider's.
#[derive(Clone)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub driver: &'static dyn Driver,
    pub settings: Settings,
    pub catalogue: String,
}

impl std::fmt::Debug for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Provider")
            .field("id", &self.id)
            .field("driver", &self.driver.id())
            .finish_non_exhaustive()
    }
}

impl Provider {
    /// The provider of `driver` under the driver's own id and name, its catalogue entries the driver's.
    #[must_use]
    pub fn of(driver: &'static dyn Driver) -> Self {
        Self {
            id: driver.id().to_owned(),
            name: driver.name().to_owned(),
            driver,
            settings: Settings::default(),
            catalogue: driver.id().to_owned(),
        }
    }
}

/// What the host gives the models: its credentials, the files a conversation names, where requests go, the ids it
/// mints, and its clock, in milliseconds since 1970.
#[derive(Clone)]
pub struct Host {
    pub credentials: Arc<dyn Credentials>,
    pub files: Arc<dyn Files>,
    pub transport: Arc<dyn Transport>,
    pub mint: Mint,
    pub clock: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl std::fmt::Debug for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Host").finish_non_exhaustive()
    }
}

/// The directory as it was built: the providers, and each provider's models by their references, in its order.
#[derive(Debug, Default)]
struct Directory {
    providers: Vec<Provider>,
    models: Vec<Model>,
}

/// The models of the providers, and the requests sent to them.
#[derive(Debug)]
pub struct Models {
    host: Host,
    catalogue: RwLock<Arc<Vec<Model>>>,
    /// What each listing provider listed last, by its id.
    listings: RwLock<HashMap<String, Vec<Model>>>,
    directory: RwLock<Arc<Directory>>,
}

/// The longest name a function may have, of letters, digits, `_` and `-`, as every provider takes.
const FUNCTION_NAME_MAX: usize = 64;

impl Models {
    /// The models of `providers`, from `catalogue`, through what `host` gives.
    #[must_use]
    pub fn new(host: Host, providers: Vec<Provider>, catalogue: Vec<Model>) -> Self {
        let models = Self {
            host,
            catalogue: RwLock::new(Arc::new(catalogue)),
            listings: RwLock::new(HashMap::new()),
            directory: RwLock::new(Arc::default()),
        };
        models.build(providers);
        models
    }

    /// The providers, in their order.
    #[must_use]
    pub fn providers(&self) -> Vec<Provider> {
        self.directory.read().providers.clone()
    }

    /// Sets the providers anew, each its catalogue's models or its listing's.
    pub fn set_providers(&self, providers: Vec<Provider>) {
        self.build(providers);
    }

    /// Sets the catalogue anew, as a newer one is fetched.
    pub fn set_catalogue(&self, catalogue: Vec<Model>) {
        *self.catalogue.write() = Arc::new(catalogue);
        self.build(self.providers());
    }

    /// Every provider's models, in the providers' order.
    #[must_use]
    pub fn models(&self) -> Vec<Model> {
        self.directory.read().models.clone()
    }

    /// The model `reference` names; none where no provider serves it.
    #[must_use]
    pub fn model(&self, reference: &str) -> Option<Model> {
        self.directory
            .read()
            .models
            .iter()
            .find(|model| model.reference() == reference)
            .cloned()
    }

    /// The directory made of `providers`: a listing provider's models those it listed, each with the catalogue's facts
    /// where the catalogue knows it; any other's those its catalogue entries name.
    fn build(&self, providers: Vec<Provider>) {
        let catalogue = Arc::clone(&self.catalogue.read());
        let listings = self.listings.read();
        let mut models = Vec::new();
        for provider in &providers {
            let known = catalogue.iter().filter(|model| model.provider == provider.catalogue);
            match listings.get(&provider.id) {
                Some(listed) => models.extend(listed.iter().map(|model| {
                    let facts = catalogue
                        .iter()
                        .find(|known| known.provider == provider.catalogue && known.id == model.id);
                    Model {
                        provider: provider.id.clone(),
                        ..facts.cloned().unwrap_or_else(|| model.clone())
                    }
                })),
                None => models.extend(known.map(|model| Model {
                    provider: provider.id.clone(),
                    ..model.clone()
                })),
            }
        }
        *self.directory.write() = Arc::new(Directory { providers, models });
    }

    /// Asks each provider whose driver lists its models for them; one that fails to answer keeps what it listed last.
    pub async fn refresh(&self) {
        for provider in self.providers() {
            let call = self.call(&provider);
            if let Ok(Some(listed)) = provider.driver.list(&call).await {
                self.listings.write().insert(provider.id.clone(), listed);
            }
        }
        self.build(self.providers());
    }

    /// A call of `provider`'s, made now.
    #[must_use]
    pub fn call(&self, provider: &Provider) -> Call {
        Call {
            provider: provider.id.clone(),
            settings: provider.settings.clone(),
            credentials: Arc::clone(&self.host.credentials),
            transport: Arc::clone(&self.host.transport),
            files: Arc::clone(&self.host.files),
            now_ms: (self.host.clock)(),
        }
    }

    /// The stream of the answer to `request`: its model found, the request checked against it and prepared for it,
    /// sent by its driver and made whole; a request that fails before it is sent is a stream of that failure alone.
    pub async fn text(&self, request: &Request) -> TextStream {
        let directory = Arc::clone(&self.directory.read());
        let found = catalogue::split_reference(&request.model).and_then(|(provider, _)| {
            let provider = directory.providers.iter().find(|each| each.id == provider)?;
            let model = directory.models.iter().find(|model| model.reference() == request.model)?;
            Some((provider.clone(), model.clone()))
        });
        let Some((provider, model)) = found else {
            let failure = Failure::new(
                FailureKind::InvalidRequest,
                format!("No provider serves the model {}", request.model),
            );
            return self.failed(&unknown(&request.model), failure);
        };
        let call = self.call(&provider);
        let sent = match check(&model, request) {
            Ok(()) => provider.driver.stream(&call, &model, &prepared(&model, request)).await,
            Err(failure) => Err(failure),
        };
        match sent {
            Ok(source) => TextStream::new(source, &model, call.now_ms, Arc::clone(&self.host.mint)),
            Err(failure) => self.failed(&model, failure),
        }
    }

    fn failed(&self, model: &Model, failure: Failure) -> TextStream {
        let source: Source = Box::pin(futures::stream::iter([Err(failure)]));
        TextStream::new(source, model, (self.host.clock)(), Arc::clone(&self.host.mint))
    }
}

/// A model no provider serves, as a failure names it.
fn unknown(reference: &str) -> Model {
    let (provider, id) = catalogue::split_reference(reference).unwrap_or(("", reference));
    Model {
        id: id.to_owned(),
        provider: provider.to_owned(),
        name: id.to_owned(),
        context_length: None,
        max_output_tokens: None,
        reads_images: false,
        reads_files: false,
        reasoning: None,
        parameters: None,
        pricing: None,
        release_date: None,
        searches_web: false,
        limits: catalogue::InputLimits::default(),
        features: catalogue::Features::default(),
    }
}

/// Checks `request` against `model`: each function's name one every provider takes; a web search only for a model
/// that searches itself; images and files only for a model that reads them, and no more images than it takes.
fn check(model: &Model, request: &Request) -> Result<(), Failure> {
    let refused = |words: String| Err(Failure::new(FailureKind::InvalidRequest, words));
    let reference = model.reference();
    for tool in &request.tools {
        match tool {
            Tool::Function { name, .. }
                if name.is_empty()
                    || name.len() > FUNCTION_NAME_MAX
                    || !name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-') =>
            {
                return refused(format!(
                    "The function \"{name}\" has a name providers refuse: a name is at most {FUNCTION_NAME_MAX} letters, digits, _ and -"
                ));
            }
            Tool::WebSearch if !model.searches_web => return refused(format!("The model {reference} does not search the web itself")),
            _ => {}
        }
    }
    let mut images = 0;
    for part in request.input.iter().flat_map(content_of) {
        match part {
            InputContent::InputImage { .. } if !model.reads_images => return refused(format!("The model {reference} reads no images")),
            InputContent::InputImage { .. } => images += 1,
            InputContent::InputFile { file } if !model.reads_files && !crate::protocols::is_text(&file.media_type) => {
                return refused(format!("The model {reference} reads no files of the type {}", file.media_type));
            }
            _ => {}
        }
    }
    if let Some(most) = model.limits.images.filter(|most| images > *most) {
        return Err(Failure::new(
            FailureKind::PayloadTooLarge,
            format!("The request holds {images} images, and the model {reference} takes at most {most}"),
        ));
    }
    Ok(())
}

/// The input content an item holds: a message's of the person, a developer or the system, or a call's output.
fn content_of(item: &Item) -> &[InputContent] {
    match item {
        Item::Message(Message::User { content } | Message::Developer { content } | Message::System { content }) => content,
        Item::FunctionCallOutput(output) => &output.output,
        _ => &[],
    }
}

/// `request` prepared for `model`: its effort and each change of it fitted to what the model takes — none for a model
/// that does not reason — and each call's arguments an object, as a provider reads them.
fn prepared(model: &Model, request: &Request) -> Request {
    let fitted = |effort: ReasoningEffort| fitted_effort(model, effort);
    let input = request
        .input
        .iter()
        .filter_map(|item| match item {
            Item::ConfigurationUpdate(update) => {
                fitted(update.effort).map(|effort| Item::ConfigurationUpdate(crate::vocabulary::ConfigurationUpdate { effort }))
            }
            Item::FunctionCall(call) if serde_json::from_str::<serde_json::Value>(&call.arguments).is_ok_and(|given| given.is_object()) => {
                Some(item.clone())
            }
            Item::FunctionCall(call) => Some(Item::FunctionCall(FunctionCall {
                arguments: "{}".to_owned(),
                ..call.clone()
            })),
            _ => Some(item.clone()),
        })
        .collect();
    Request {
        input,
        reasoning: request
            .reasoning
            .and_then(|reasoning| fitted(reasoning.effort))
            .map(|effort| ReasoningConfig { effort }),
        ..request.clone()
    }
}

/// The effort `model` takes nearest to `asked`: the highest it takes at or below it, else the lowest it takes — every
/// effort where it lists none, but `none` where it always reasons; none for a model that does not reason.
fn fitted_effort(model: &Model, asked: ReasoningEffort) -> Option<ReasoningEffort> {
    const EFFORTS: [ReasoningEffort; 7] = [
        ReasoningEffort::None,
        ReasoningEffort::Minimal,
        ReasoningEffort::Low,
        ReasoningEffort::Medium,
        ReasoningEffort::High,
        ReasoningEffort::Xhigh,
        ReasoningEffort::Max,
    ];
    let how = model.reasoning.as_ref()?;
    let is_taken = |effort: &ReasoningEffort| match &how.efforts {
        Some(listed) => listed.contains(effort),
        None => *effort != ReasoningEffort::None || !how.mandatory,
    };
    let taken: Vec<ReasoningEffort> = EFFORTS.into_iter().filter(is_taken).collect();
    taken
        .iter()
        .rev()
        .find(|effort| **effort <= asked)
        .or_else(|| taken.first())
        .copied()
        .or(Some(asked))
}
