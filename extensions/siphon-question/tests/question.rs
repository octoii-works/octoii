//! What the person may answer a question with, and what the model is told of their answer.

use octoii_ai::vocabulary::InputContent;
use serde_json::json;
use siphon::tools::Tool;
use siphon_question::{QuestionArgs, QuestionTool};

fn questions() -> QuestionArgs {
    serde_json::from_value(json!({ "questions": [
        { "question": "Which database?", "options": [{ "label": "Postgres" }, { "label": "SQLite" }] },
        { "question": "Anything else?" },
        { "question": "Which features?", "options": [{ "label": "auth" }, { "label": "search" }], "multiple": true }
    ] }))
    .expect("questions")
}

fn text(told: &[InputContent]) -> String {
    told.iter()
        .filter_map(|part| match part {
            InputContent::InputText { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn an_answer_fits_the_choices_each_question_offers() {
    let schema = QuestionTool.asks(&questions()).expect("a schema");
    let validator = jsonschema::validator_for(&schema).expect("a validator");
    assert!(
        !validator.is_valid(&json!({ "answers": [{ "chosen": ["MySQL"] }] })),
        "no such choice"
    );
    assert!(
        !validator.is_valid(&json!({ "answers": [{ "chosen": ["Postgres", "SQLite"] }] })),
        "one of the first"
    );
    assert!(validator.is_valid(&json!({ "answers": [{ "chosen": ["SQLite"] }, null, { "chosen": ["auth", "search"], "text": "both" }] })));
    assert!(validator.is_valid(&json!({ "dismissed": true })));
}

#[test]
fn the_model_is_told_each_question_and_what_was_answered() {
    let answer = json!({ "answers": [{ "chosen": ["SQLite"] }, null, { "chosen": ["auth", "search"], "text": "both" }] });
    assert_eq!(
        text(&QuestionTool.answered(&questions(), answer).content),
        "Which database?\n→ SQLite\nAnything else?\n→ (unanswered)\nWhich features?\n→ auth, search; both"
    );
    assert_eq!(
        text(&QuestionTool.answered(&questions(), json!({ "dismissed": true })).content),
        "the person dismissed the questions, answering none"
    );
}

#[test]
fn structured_answers_preserve_choices_skips_dismissal_and_original_text() {
    let schema = QuestionTool::output_schema().expect("an output schema");
    let validator = jsonschema::validator_for(&schema).expect("a valid schema");
    for answer in [
        json!({ "answers": [{ "chosen": ["SQLite"] }, null, { "chosen": ["auth", "search"], "text": "  both\nplease  " }] }),
        json!({ "answers": [] }),
        json!({ "dismissed": true }),
        json!({}),
    ] {
        let told = QuestionTool.answered(&questions(), answer.clone());
        assert_eq!(told.structured_content, Some(answer.clone()));
        assert!(validator.is_valid(&answer), "{answer}");
    }
}
