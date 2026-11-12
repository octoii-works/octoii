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
    const DESCRIPTION: &'static str = "Works the thread's browser, which the person sees beside the chat, one action a call, on its current page — switch makes another current:\n- pages: open, tabs, switch, close; navigate, back, forward, reload.\n- seeing: read gives the page as a screen reader does, each element with a ref such as e12, new refs marked *; look shows it; info measures it.\n- acting on an element by its ref, a CSS selector or a point: click, hover, type, press, select, scroll, drag, upload, dialog.\n- evaluate runs JavaScript in the page or in a frame; wait waits for text, an address, an element or a quiet network; console and downloads read what the page logged and saved; cdp calls the Chrome DevTools Protocol where nothing else does.\nAn action that changes a page waits till it settles and says whether it navigated, changed the page or changed nothing. Name a page in your reply as :page{id=<its id>}, or ::page{id=<its id>} for its card.";
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
