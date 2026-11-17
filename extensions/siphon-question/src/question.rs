//! `question`: what the model asks the person — some questions at once, each with options to choose one of, or
//! several, the person free to answer in words of their own instead; any left unanswered, or all of them dismissed. The
//! person answers it, as its call's request; the JSON Schema of their answer is made from the questions it asks.

use std::fmt::Write as _;

use octoii::machine::Base;
use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use octoii::tools::{Context, Effect, Refusal, Told, Tool};

/// An option.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Choice {
    /// The option, in a few words.
    pub label: String,
    /// What the option means, where the label is not enough.
    pub description: Option<String>,
}

/// A question.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct Asked {
    /// The question, as the person will read it.
    pub question: String,
    /// A label of one or two words, shown above the question.
    pub header: Option<String>,
    /// Options to choose from; omit for a free-text answer.
    #[serde(default)]
    pub options: Vec<Choice>,
    /// Allow choosing several options.
    #[serde(default)]
    pub multiple: bool,
}

/// The arguments of `question`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct QuestionArgs {
    /// One to eight questions.
    #[schemars(length(min = 1, max = 8))]
    pub questions: Vec<Asked>,
}

/// The person's original answers; omitted entries and null entries remain unanswered.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuestionResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answers: Option<Vec<Option<QuestionAnswer>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dismissed: Option<bool>,
}

/// Selected labels and free text, preserved without parsing the model-facing summary.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuestionAnswer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chosen: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// `question`.
#[derive(Debug, Clone, Copy, Default)]
pub struct QuestionTool;

impl Tool for QuestionTool {
    const NAME: &'static str = "question";
    const DESCRIPTION: &'static str = "Ask one to eight questions, with single-choice or multiple-choice options, or free-text answers. Returns the person's answers, skipped questions or dismissal.";
    type Args = QuestionArgs;

    fn output_schema() -> Option<Value> {
        Some(schemars::schema_for!(QuestionResult).to_value())
    }

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

    fn answered(&self, args: &QuestionArgs, answer: Value) -> Told {
        Told {
            content: answer_content(args, &answer),
            structured_content: Some(answer),
            ..Told::default()
        }
    }
}

fn answer_content(args: &QuestionArgs, answer: &Value) -> Vec<InputContent> {
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
