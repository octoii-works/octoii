//! `browser`: the thread's browser, through its one interface — an action of the browser's, as JSON, on the thread's
//! current page unless it names one. What reads a page goes ahead at once; what changes one goes after what came
//! before it, as every call that acts does.

use octoii_ai::vocabulary::InputContent;
use siphon_machine::paths::Base;
use siphon_tools::{Context, Effect, Refusal, Told, Tool};

use crate::{Action, Browser};

/// `browser`, on the thread's browser.
#[derive(Debug, Clone)]
pub struct BrowserTool {
    pub browser: Browser,
}

impl Tool for BrowserTool {
    const NAME: &'static str = "browser";
    const DESCRIPTION: &'static str = "Works the thread's browser, which the person sees beside the chat: open, tabs, switch and close pages; navigate, back, forward, reload; read a page as a screen reader does — each element's ref, as e12, new refs marked * — look at it, or measure it; click, hover, type, press, select, scroll, drag, upload and answer a dialog on an element by its ref, a CSS selector, or a point; evaluate JavaScript in the page or in a frame of another site; wait for text, an address, an element or a quiet network; read its console and downloads; or call raw CDP where nothing else does. Each action that changes a page waits till it settles and says whether it navigated, changed the page or changed nothing.";
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
