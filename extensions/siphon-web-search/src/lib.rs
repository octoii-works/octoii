//! The web search as an extension of Siphon's: the search a thread's model's provider runs, given where its model's
//! facts say the provider runs it.

use octoii_ai::vocabulary::Tool;
use siphon::{Extension, Registrar};

/// The web search, as a host adds it.
#[derive(Debug, Clone, Copy, Default)]
pub struct WebSearchExtension;

impl Extension for WebSearchExtension {
    fn name(&self) -> &'static str {
        "web-search"
    }

    fn title(&self) -> &'static str {
        "Web search"
    }

    fn description(&self) -> &'static str {
        "The web search a model's provider runs, for models that have it."
    }

    fn build(self: Box<Self>, registrar: &mut Registrar<'_>) {
        registrar.native(Tool::WebSearch);
    }
}
