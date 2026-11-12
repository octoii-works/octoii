//! The browser's tool, as the model calls it: what reads a page goes ahead at once, what changes one after what came
//! before it, and its actions told in its schema.

use std::path::PathBuf;

use beak::Beak;
use serde_json::json;
use siphon::machine::Base;
use siphon::machine::Host;
use siphon::tools::{Effect, Tools};

#[tokio::test(flavor = "multi_thread")]
async fn the_browser_tool_reads_a_page_at_once_and_works_one_after_what_came_before() {
    let beak = Beak::new().expect("Beak");
    let commands = siphon::testing::commands(&beak);
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
    let schema = &tools.declared()[0].parameters;
    assert!(schema.to_string().contains("\"navigate\""), "its actions told");
}
