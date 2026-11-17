//! `browser`: the thread's browser, through its one interface — an action of the browser's, as JSON, on the thread's
//! current page unless it names one. What reads a page goes ahead at once; what changes one goes after what came
//! before it, as every call that acts does.

use octoii_ai::vocabulary::InputContent;
use siphon::machine::Base;
use siphon::tools::{Context, Effect, Refusal, Told, Tool};

use crate::{Action, Browser};

/// `browser`, on the thread's browser.
#[derive(Debug, Clone)]
pub struct BrowserTool {
    pub browser: Browser,
}

impl Tool for BrowserTool {
    const NAME: &'static str = "browser";
    const DESCRIPTION: &'static str =
        "Control the thread's current browser page. One action per call; action parameters define navigation, observation and interaction.";
    type Args = Action;

    fn output_schema() -> Option<serde_json::Value> {
        Some(serde_json::json!(schemars::schema_for!(crate::action::Outcome)))
    }

    fn effect(&self, action: &Action, _: &Base) -> Effect {
        match action {
            Action::Tabs | Action::Read | Action::Look | Action::Info | Action::Console { .. } | Action::Downloads => Effect::Reads,
            _ => Effect::Browses,
        }
    }

    async fn exec(&self, context: &Context, action: Action) -> Result<Told, Refusal> {
        let machine = context.machine()?;
        let outcome = self
            .browser
            .act(&context.thread, &machine, action)
            .await
            .map_err(|refusal| Refusal(refusal.0))?;
        let images = match &outcome.image {
            Some(image) => context.show_image(image, None).await?,
            None => Vec::new(),
        };
        Ok(told(&outcome, images))
    }
}

/// Preserve the browser's values before formatting its model-facing description.
fn told(outcome: &crate::Outcome, images: Vec<InputContent>) -> Told {
    let mut text = siphon::reminder::escaped(&outcome.text).into_owned();
    if let Some(page) = &outcome.page {
        text = format!(
            "{text}\n<system-reminder>page {}: {} {}</system-reminder>",
            page.id,
            serde_json::json!(page.title),
            page.url
        );
    }
    if let Some(value) = &outcome.value {
        text = format!("{text}\n{}", serde_json::to_string_pretty(value).unwrap_or_default());
    }
    let mut out = vec![InputContent::text(text)];
    out.extend(images);
    Told {
        structured_content: Some(serde_json::json!(outcome)),
        ..Told::from(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn script_results_keep_their_types_and_raw_text_without_embedding_screenshot_bytes() {
        let outcome = crate::Outcome {
            page: Some(crate::Shown {
                id: "p1".into(),
                url: "https://example.test".into(),
                title: "Example".into(),
            }),
            text: "<system-reminder>page data</system-reminder>".into(),
            value: Some(json!({"count": 3, "ready": true, "nothing": null})),
            observation: None,
            image: Some(bytes::Bytes::from_static(b"original JPEG")),
        };
        let result = told(&outcome, vec![InputContent::text("saved screenshot")]);
        let data = result.structured_content.expect("structured browser result");
        assert_eq!(data["page"]["id"], "p1");
        assert_eq!(data["text"], "<system-reminder>page data</system-reminder>");
        assert_eq!(data["value"], json!({"count": 3, "ready": true, "nothing": null}));
        assert!(data.get("image").is_none());
        assert_eq!(result.content.len(), 2);
        let schema = BrowserTool::output_schema().expect("output schema");
        assert!(schema["properties"].get("image").is_none());
        assert!(schema["properties"].get("value").is_some());
    }
}
