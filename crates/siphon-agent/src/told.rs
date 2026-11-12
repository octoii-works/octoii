//! What a thread's model is told of itself, of where it works and of what it may call, one text for every model: its
//! sections, the most shared first, so a provider's cache of the beginning serves one thread as well as the next — and
//! the tools it may call. Before each turn they are gathered anew; the sections that changed since the log last told
//! them are appended as a system entry, so the model is told each change once, where it came, and what it was told
//! stays in the log as it was sent. The tools are those of the entry that begins a window: a change of them takes
//! effect from the next window, so what a window begins with is never changed within it.

use octoii_ai::vocabulary::Tool;
use serde::{Deserialize, Serialize};
use siphon_threads::{Entry, Section, Window};

/// What the model is told: its sections, by name, in the order told; the tools it may call; and the names of the tools
/// its scripts may call beside them, as the extensions told them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Told {
    pub sections: Vec<(String, String)>,
    pub tools: Vec<Tool>,
    pub catalog: Vec<String>,
}

/// The sections the model holds after `window`'s system entries, in the order first told: each as the last entry that
/// named it gave it, and none an entry took away.
#[must_use]
pub fn held(window: &Window) -> Vec<(String, String)> {
    let mut held: Vec<(String, String)> = Vec::new();
    for entry in window.told.iter().chain(&window.conversation) {
        let Entry::System { sections, .. } = entry else { continue };
        for section in sections {
            let at = held.iter().position(|(name, _)| *name == section.name);
            match (&section.text, at) {
                (Some(text), Some(at)) => held[at].1.clone_from(text),
                (Some(text), None) => held.push((section.name.clone(), text.clone())),
                (None, Some(at)) => {
                    held.remove(at);
                }
                (None, None) => {}
            }
        }
    }
    held
}

/// What of the sections `now` the model was not told, as `held` holds what it was: each new or changed, whole, then
/// each no longer there, with none.
#[must_use]
pub fn news(held: &[(String, String)], now: &[(String, String)]) -> Vec<Section> {
    let mut news: Vec<Section> = now
        .iter()
        .filter(|(name, text)| !held.iter().any(|(was, before)| was == name && before == text))
        .map(|(name, text)| Section {
            name: name.clone(),
            text: Some(text.clone()),
        })
        .collect();
    news.extend(
        held.iter()
            .filter(|(was, _)| !now.iter().any(|(name, _)| name == was))
            .map(|(was, _)| Section {
                name: was.clone(),
                text: None,
            }),
    );
    news
}
