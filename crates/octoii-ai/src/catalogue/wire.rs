//! The catalogue as its JSON is written, for the catalogue's own readers too: models of every kind, their prices as
//! decimal strings of US dollars, Octoii's facts under `octoii`. What the crate reads of it is its text models' facts.

use serde::Deserialize;

use super::pricing::{Override, Prices, Pricing, Rate};
use super::{Features, InputLimits, Model, Reasoning};
use crate::vocabulary::ReasoningEffort;

#[derive(Deserialize)]
struct Catalogue {
    models: Vec<Listed>,
}

#[derive(Deserialize)]
struct Listed {
    id: String,
    name: String,
    context_length: Option<u64>,
    max_completion_tokens: Option<u64>,
    architecture: Architecture,
    reasoning: Option<ListedReasoning>,
    supported_parameters: Option<Vec<String>>,
    pricing: Option<ListedPricing>,
    octoii: Facts,
}

#[derive(Deserialize)]
struct Architecture {
    input_modalities: Vec<String>,
}

#[derive(Deserialize)]
struct ListedReasoning {
    supported_efforts: Option<Vec<ReasoningEffort>>,
    default_effort: Option<ReasoningEffort>,
    #[serde(default)]
    mandatory: bool,
}

#[derive(Deserialize)]
struct Facts {
    provider: String,
    kinds: Vec<String>,
    release_date: Option<String>,
    #[serde(default)]
    hosted_tools: Vec<String>,
    #[serde(default)]
    input_limits: ListedLimits,
    #[serde(default)]
    features: ListedFeatures,
}

#[derive(Default, Deserialize)]
struct ListedLimits {
    request_bytes: Option<u64>,
    image_bytes: Option<u64>,
    image_side_pixels: Option<u64>,
    images: Option<u64>,
}

#[derive(Default, Deserialize)]
struct ListedFeatures {
    instruction_updates: Option<bool>,
    effort_updates: Option<bool>,
    tool_choices: Option<Vec<String>>,
}

#[derive(Default, Deserialize)]
struct ListedPrices {
    prompt: Option<String>,
    completion: Option<String>,
    input_cache_read: Option<String>,
    input_cache_write: Option<String>,
    internal_reasoning: Option<String>,
    web_search: Option<String>,
    request: Option<String>,
}

#[derive(Deserialize)]
struct ListedPricing {
    #[serde(flatten)]
    prices: ListedPrices,
    discount: Option<f64>,
    #[serde(default)]
    overrides: Vec<ListedOverride>,
}

#[derive(Deserialize)]
struct ListedOverride {
    min_prompt_tokens: Option<u64>,
    utc_start: Option<u32>,
    utc_end: Option<u32>,
    utc_days: Option<Vec<Day>>,
    #[serde(flatten)]
    prices: ListedPrices,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Day {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl ListedPrices {
    fn read(&self) -> Prices {
        let rate = |text: &Option<String>| text.as_deref().map(Rate::of_dollars);
        Prices {
            prompt: rate(&self.prompt),
            completion: rate(&self.completion),
            cache_read: rate(&self.input_cache_read),
            cache_write: rate(&self.input_cache_write),
            reasoning: rate(&self.internal_reasoning),
            web_search: rate(&self.web_search),
            request: rate(&self.request),
        }
    }
}

impl ListedPricing {
    fn read(&self) -> Pricing {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a fraction of one, in millionths"
        )]
        let discount_millionths = (self.discount.unwrap_or(0.0).clamp(0.0, 1.0) * 1_000_000.0).round() as u32;
        Pricing {
            base: self.prices.read(),
            overrides: self
                .overrides
                .iter()
                .map(|each| Override {
                    min_prompt_tokens: each.min_prompt_tokens,
                    utc_start: each.utc_start,
                    utc_end: each.utc_end,
                    utc_days: each.utc_days.as_ref().map(|days| days.iter().map(|day| *day as u8).collect()),
                    prices: each.prices.read(),
                })
                .collect(),
            discount_millionths,
        }
    }
}

impl Listed {
    fn into_model(self) -> Model {
        let reads = |modality: &str| self.architecture.input_modalities.iter().any(|each| each == modality);
        Model {
            reads_images: reads("image"),
            reads_files: reads("file"),
            pricing: self.pricing.as_ref().map(ListedPricing::read),
            id: self.id,
            provider: self.octoii.provider,
            name: self.name,
            context_length: self.context_length,
            max_output_tokens: self.max_completion_tokens,
            reasoning: self.reasoning.map(|reasoning| Reasoning {
                efforts: reasoning.supported_efforts,
                default_effort: reasoning.default_effort,
                mandatory: reasoning.mandatory,
            }),
            parameters: self.supported_parameters,
            release_date: self.octoii.release_date,
            searches_web: self.octoii.hosted_tools.iter().any(|tool| tool == "web_search"),
            limits: InputLimits {
                request_bytes: self.octoii.input_limits.request_bytes,
                image_bytes: self.octoii.input_limits.image_bytes,
                image_side_pixels: self.octoii.input_limits.image_side_pixels,
                images: self.octoii.input_limits.images,
            },
            features: Features {
                instruction_updates: self.octoii.features.instruction_updates,
                effort_updates: self.octoii.features.effort_updates,
                tool_choices: self.octoii.features.tool_choices,
            },
        }
    }
}

pub(super) fn models(json: &str) -> Result<Vec<Model>, serde_json::Error> {
    let catalogue: Catalogue = serde_json::from_str(json)?;
    Ok(catalogue
        .models
        .into_iter()
        .filter(|model| model.octoii.kinds.iter().any(|kind| kind == "text"))
        .map(Listed::into_model)
        .collect())
}
