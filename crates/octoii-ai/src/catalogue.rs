//! The catalogue of models: what each text model is — what it reads, how much it takes, how it reasons, what it costs,
//! and what sets it apart — as Octoii's catalogue describes it, bundled with the crate and fetched anew. Models are
//! data: no code tells one model from another by its id, but by these facts. A fact left out is unknown.

mod pricing;
mod wire;

pub use pricing::{Conditions, Override, Prices, Pricing, Rate};

use crate::vocabulary::ReasoningEffort;

/// A text model: its provider's id of it and its provider, which make its reference `provider/id`; its name for
/// people; and its facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub id: String,
    pub provider: String,
    pub name: String,
    /// The tokens a request and its response may take together.
    pub context_length: Option<u64>,
    /// The tokens a response may take.
    pub max_output_tokens: Option<u64>,
    /// Whether it reads images, and files such as PDFs, besides text.
    pub reads_images: bool,
    pub reads_files: bool,
    /// How it reasons; none for a model that does not.
    pub reasoning: Option<Reasoning>,
    /// The request's fields it takes, by their names, such as `structured_outputs`; none, it takes every one.
    pub parameters: Option<Vec<String>>,
    /// Its prices; none for a model that cannot be priced, as one a subscription pays for.
    pub pricing: Option<Pricing>,
    /// When its provider released it, as YYYY-MM-DD.
    pub release_date: Option<String>,
    /// Whether its provider runs a web search for it.
    pub searches_web: bool,
    pub limits: InputLimits,
    pub features: Features,
}

impl Model {
    /// The model's reference, `provider/id`.
    #[must_use]
    pub fn reference(&self) -> String {
        format!("{}/{}", self.provider, self.id)
    }

    /// Whether it takes the request's field `parameter`.
    #[must_use]
    pub fn takes(&self, parameter: &str) -> bool {
        self.parameters
            .as_ref()
            .is_none_or(|parameters| parameters.iter().any(|each| each == parameter))
    }
}

/// How a model reasons: the efforts it takes, highest first — every one where none is listed — the one it reasons at
/// where a request names none, and whether it always reasons, taking no effort `none`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reasoning {
    pub efforts: Option<Vec<ReasoningEffort>>,
    pub default_effort: Option<ReasoningEffort>,
    pub mandatory: bool,
}

/// The largest a model's input may be, where its provider says.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputLimits {
    pub request_bytes: Option<u64>,
    pub image_bytes: Option<u64>,
    pub image_side_pixels: Option<u64>,
    pub images: Option<u64>,
}

/// What sets a text model apart from its provider's others: whether it takes instructions updated after the person's
/// first turn where they stand, and an effort changed where it stands; and the tool choices it takes. Each is unknown
/// where no source says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Features {
    pub instruction_updates: Option<bool>,
    pub effort_updates: Option<bool>,
    pub tool_choices: Option<Vec<String>>,
}

/// The provider and the provider's id of the model `reference` names: the provider is what stands before the first
/// `/`, so an id may hold more. None for a reference without a provider or an id.
#[must_use]
pub fn split_reference(reference: &str) -> Option<(&str, &str)> {
    let (provider, id) = reference.split_once('/')?;
    (!provider.is_empty() && !id.is_empty()).then_some((provider, id))
}

/// The catalogue bundled with the crate, as it was when the crate was built.
const BUNDLED: &str = include_str!("catalogue.json");

/// The catalogue's text models, from its JSON: those of other kinds left out.
///
/// # Errors
/// Where the JSON is no catalogue.
pub fn parse(json: &str) -> Result<Vec<Model>, serde_json::Error> {
    wire::models(json)
}

/// The catalogue bundled with the crate.
///
/// # Panics
/// Where the bundled catalogue is no catalogue, which its tests rule out.
#[must_use]
pub fn bundled() -> Vec<Model> {
    parse(BUNDLED).expect("the bundled catalogue reads")
}
