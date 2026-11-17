//! The browser's tool, as the model calls it: what reads a page goes ahead at once, what changes one after what came
//! before it, and its actions told in its schema.

use std::path::PathBuf;

use octoii::machine::Base;
use octoii::machine::Host;
use octoii::testing::Beak;
use octoii::tools::{Effect, Tools};
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn the_browser_tool_reads_a_page_at_once_and_works_one_after_what_came_before() {
    let beak = Beak::new().expect("Beak");
    let commands = octoii::testing::commands(&beak);
    let browser = siphon_browser::Browser::start(siphon_browser::BrowserHost {
        commands,
        machine: Host::default(),
        program: None,
        profile: None,
        downloads: None,
    });
    let tools = Tools::new().with(siphon_browser::BrowserTool { browser });
    let base = Base {
        workspace: PathBuf::from("/work"),
        home: None,
    };
    assert_eq!(tools.effect("browser", &json!({ "action": "read" }), &base), Ok(Effect::Reads));
    assert_eq!(
        tools.effect("browser", &json!({ "action": "click", "target": { "ref": "e3" } }), &base),
        Ok(Effect::Browses)
    );
    assert_eq!(
        tools.effect("browser", &json!({ "action": "preview", "path": "report.html" }), &base),
        Ok(Effect::Browses)
    );
    let schema = &tools.declared()[0].parameters;
    assert!(schema.to_string().contains("\"navigate\""), "its actions told");
}

#[test]
fn text_actions_reject_mixed_and_missing_parameters() {
    let schema = schemars::schema_for!(siphon_browser::Action).to_value();
    let validator = jsonschema::validator_for(&schema).unwrap();
    for value in [
        json!({"action": "type", "text": "hello"}),
        json!({"action": "fill", "target": {"selector": "input"}, "text": ""}),
        json!({"action": "press", "key": "Enter"}),
    ] {
        assert!(validator.is_valid(&value), "{value}");
        assert!(serde_json::from_value::<siphon_browser::Action>(value).is_ok());
    }
    for value in [
        json!({"action": "type", "text": "hello", "clear": true}),
        json!({"action": "type", "text": "hello", "submit": false}),
        json!({"action": "fill", "text": "hello"}),
        json!({"action": "press", "key": "Enter", "text": "hello"}),
    ] {
        assert!(!validator.is_valid(&value), "{value}");
        assert!(serde_json::from_value::<siphon_browser::Action>(value).is_err());
    }
}
