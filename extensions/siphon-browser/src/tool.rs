//! `browser`: the thread's browser, through its one interface — an action of the browser's, as JSON, on the thread's
//! current page unless it names one. What reads a page goes ahead at once; what changes one goes after what came
//! before it, as every call that acts does.

use octoii_ai::vocabulary::InputContent;
use siphon::machine::paths::Base;
use siphon::tools::{Context, Effect, Refusal, Told, Tool};

use crate::{Action, Browser};

/// `browser`, on the thread's browser.
#[derive(Debug, Clone)]
pub struct BrowserTool {
    pub browser: Browser,
}

impl Tool for BrowserTool {
    const NAME: &'static str = "browser";
    const DESCRIPTION: &'static str = "Control the thread's browser, which the person sees beside the chat. One action per call, on the current page; switch changes the current page.\n- Pages: open, tabs, switch, close, navigate, back, forward, reload.\n- Seeing: read returns the page's accessibility tree, each element with a ref such as e12, refs new since the last read marked *; look returns a screenshot; info returns the URL, title, size and scroll.\n- Acting on an element, by ref, CSS selector or point: click, hover, type, press, select, scroll, drag, upload, dialog.\n- Other: evaluate runs JavaScript in the page or a frame; wait waits for text, a URL, an element or an idle network; console and downloads return the page's logs and downloads; cdp sends a raw Chrome DevTools Protocol command when no other action can.\nAn action that changes the page waits for it to settle, then says whether it navigated, changed the page or changed nothing. Link a page in your reply as :page{id=<id>}, or as ::page{id=<id>} on its own line to show its card.";
    type Args = Action;

    fn effect(&self, action: &Action, _: &Base) -> Effect {
        match action {
            Action::Tabs | Action::Read | Action::Look | Action::Info | Action::Console { .. } | Action::Downloads => Effect::Reads,
            _ => Effect::Browses,
        }
    }

    async fn exec(&self, context: &Context, action: Action) -> Result<Told, Refusal> {
        let outcome = self
            .browser
            .act(&context.thread, action)
            .await
            .map_err(|refusal| Refusal(refusal.0))?;
        let mut text = outcome.text;
        if let Some(page) = &outcome.page {
            text = format!("{text}\n[page {}: {} {}]", page.id, serde_json::json!(page.title), page.url);
        }
        if let Some(value) = &outcome.value {
            text = format!("{text}\n{}", serde_json::to_string_pretty(value).unwrap_or_default());
        }
        let mut out = vec![InputContent::text(text)];
        if let Some(image) = outcome.image {
            out.extend(context.show_image(&image, None).await?);
        }
        Ok(Told::from(out))
    }
}
