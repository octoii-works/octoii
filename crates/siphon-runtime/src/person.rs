//! Requests a person answers: no handler acts on them; the person is shown what is asked and what the answer is to fit,
//! a JSON Schema, and their answer is given to the request once it fits.

use schemars::JsonSchema;

use crate::request::Request;

/// A request a person answers.
pub trait AskedOfPerson: Request
where
    Self::Answer: JsonSchema,
{
    /// The JSON Schema the answer to this request fits: the answer's own by default; one made from what is asked, as
    /// the answer to a question is one of the choices it offers.
    fn answer_schema(&self) -> serde_json::Value {
        schemars::schema_for!(Self::Answer).to_value()
    }
}

/// Why a person's answer was not given.
#[derive(Debug, thiserror::Error)]
pub enum NotGiven {
    #[error("no such request waits")]
    NoSuchRequest,
    #[error("a person does not answer the request {0}")]
    NotAskedOfPerson(String),
    /// The answer does not fit what the request asks, each problem said.
    #[error("the answer does not fit: {}", .0.join("; "))]
    Unfit(Vec<String>),
    #[error(transparent)]
    Damaged(#[from] anyhow::Error),
    #[error(transparent)]
    Beak(#[from] beak::Error),
}

/// The problems `answer` has against `schema`; none where it fits.
pub(crate) fn problems(schema: &serde_json::Value, answer: &serde_json::Value) -> anyhow::Result<Vec<String>> {
    let validator = jsonschema::validator_for(schema).map_err(|error| anyhow::anyhow!("the answer's schema: {error}"))?;
    Ok(validator
        .iter_errors(answer)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect())
}
