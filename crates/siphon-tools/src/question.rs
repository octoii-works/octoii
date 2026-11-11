//! `question`: what the model asks the person — some questions at once, each with options to choose one of, or
//! several, the person free to answer in words of their own instead; any left unanswered, or all of them dismissed. The
//! person answers it, as its call's request; the JSON Schema of their answer is made from the questions it asks.

use std::fmt::Write as _;

use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use siphon_machine::paths::Base;

use crate::effect::Effect;
use crate::told::Told;
use crate::tool::{Context, Refusal, Tool};

/// An option a question offers.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Choice {
    /// What the person picks, in a few words.
    pub label: String,
    /// What it means, where the label alone does not say.
    pub description: Option<String>,
}

/// A question.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Asked {
    pub question: String,
    /// What it is about, in a word or two, shown above it.
    pub header: Option<String>,
    /// The options to choose from; none, the person answers in words.
    #[serde(default)]
    pub options: Vec<Choice>,
    /// Whether the person may choose several.
    #[serde(default)]
    pub multiple: bool,
}

/// The arguments of `question`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct QuestionArgs {
    #[schemars(length(min = 1, max = 8))]
    pub questions: Vec<Asked>,
}

/// `question`.
#[derive(Debug, Clone, Copy, Default)]
pub struct QuestionTool;

impl Tool for QuestionTool {
    const NAME: &'static str = "question";
    const DESCRIPTION: &'static str = "Asks the person some questions at once, each with options to choose one of, or several with multiple, or with none to answer in words; they may answer in words of their own instead of an option, leave any unanswered, or dismiss them all. Ask only what you cannot find out yourself, and what changes what you do.";
    type Args = QuestionArgs;

    fn effect(&self, _: &QuestionArgs, _: &Base) -> Effect {
        Effect::Reads
    }

    fn exec(&self, _: &Context, _: QuestionArgs) -> impl std::future::Future<Output = Result<Told, Refusal>> + Send {
        std::future::ready(Err(Refusal::new("a question is answered by the person, not run")))
    }

    fn asks(&self, args: &QuestionArgs) -> Option<Value> {
        let each: Vec<Value> = args
            .questions
            .iter()
            .map(|asked| {
                let labels: Vec<&str> = asked.options.iter().map(|choice| choice.label.as_str()).collect();
                let most = if asked.multiple { labels.len() } else { 1 };
                json!({ "anyOf": [
                    { "type": "null" },
                    { "type": "object", "additionalProperties": false, "properties": {
                        "chosen": { "type": "array", "items": { "enum": labels }, "maxItems": most, "uniqueItems": true },
                        "text": { "type": "string" }
                    } }
                ] })
            })
            .collect();
        Some(json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "answers": { "type": "array", "prefixItems": each, "items": false },
                "dismissed": { "type": "boolean" }
            }
        }))
    }

    fn answered(&self, args: &QuestionArgs, answer: Value) -> Vec<InputContent> {
        if answer["dismissed"].as_bool() == Some(true) {
            return vec![InputContent::text("the person dismissed the questions, answering none")];
        }
        let mut out = String::new();
        for (at, asked) in args.questions.iter().enumerate() {
            let given = &answer["answers"][at];
            let chosen: Vec<&str> = given["chosen"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            let words = given["text"].as_str().filter(|text| !text.trim().is_empty());
            let said = match (chosen.is_empty(), words) {
                (false, Some(words)) => format!("{}; {words}", chosen.join(", ")),
                (false, None) => chosen.join(", "),
                (true, Some(words)) => words.to_owned(),
                (true, None) => "(unanswered)".to_owned(),
            };
            let _ = writeln!(out, "{}\n→ {said}", asked.question);
        }
        vec![InputContent::text(out.trim_end())]
    }
}
