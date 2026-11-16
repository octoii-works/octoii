//! A thread's conversation as the `thread` tool reads it back: each entry of its log as a line or a few — the person's
//! words and another thread's, what the model said and each call it made with how it ended, a turn's end, a setting
//! changed and a summary of what came before — the model's reasoning and what it was told left out; and what of a
//! text holds the keywords asked for, every one or any, whatever their case.

use octoii_ai::vocabulary::{InputContent, Item, Message, OutputContent};
use siphon::threads::{CallStatus, Entry, Outcome};

/// The most characters of a call's arguments or of its result that are told; the whole is in the thread's log.
const CALL_CHARS_MAX: usize = 300;

/// `entry` as the conversation tells it, what the person or a thread said and what a turn's end and a setting changed
/// with their time, as the entry's id says it; none for what the model was told and a reply that came to nothing.
#[must_use]
pub fn told(entry: &Entry) -> Option<String> {
    let at = siphon::threads::time_of(entry.id()).map_or_else(String::new, |ms| format!("{} ", crate::tool::time(ms)));
    match entry {
        Entry::Input { actor, content, .. } => {
            let who = if actor == "person" {
                "person".to_owned()
            } else {
                format!("thread {actor}")
            };
            Some(format!("{at}{who}: {}", words_of(content)))
        }
        Entry::Reply { response, .. } => {
            let said: Vec<String> = response.output.iter().filter_map(said).collect();
            (!said.is_empty()).then(|| said.join("\n"))
        }
        Entry::Result { status, output, .. } => Some(format!("result ({}): {}", status_of(*status), cut(&words_of(&output.output)))),
        Entry::TurnEnd { outcome, reason, .. } => Some(match (outcome, reason) {
            (Outcome::Replied, _) => format!("{at}turn ended"),
            (Outcome::Failed, reason) => format!("{at}turn failed{}", reason.as_ref().map_or_default(|reason| format!(": {reason}"))),
            (Outcome::Stopped, _) => format!("{at}turn stopped"),
        }),
        Entry::Config { changed, .. } => changes_of(changed).map(|changes| format!("{at}settings: {changes}")),
        Entry::Compaction { response, .. } => Some(format!("{at}summary of what came before: {}", response.words())),
        Entry::System { .. } | Entry::Discarded { .. } => None,
    }
}

/// What the model's `item` says: its words, or a call it made, with its arguments; none for its reasoning.
fn said(item: &Item) -> Option<String> {
    match item {
        Item::Message(Message::Assistant { content, .. }) => {
            let words: Vec<&str> = content
                .iter()
                .map(|part| match part {
                    OutputContent::OutputText { text, .. } => text.as_str(),
                    OutputContent::Refusal { refusal } => refusal.as_str(),
                })
                .collect();
            Some(format!("model: {}", words.join("")))
        }
        Item::FunctionCall(call) => Some(format!("call {}: {}", call.name, cut(&call.arguments))),
        Item::WebSearchCall(_) => Some("web search".to_owned()),
        _ => None,
    }
}

/// The words of `content`, each image or file named in its place.
fn words_of(content: &[InputContent]) -> String {
    content
        .iter()
        .map(|part| match part {
            InputContent::InputText { text } => text.clone(),
            InputContent::InputImage { .. } => "[an image]".to_owned(),
            InputContent::InputFile { .. } => "[a file]".to_owned(),
            InputContent::ResourceLink { file, .. } => format!("[file available: {}]", file.filename.as_deref().unwrap_or(&file.reference)),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// How a call ended, in the log's words.
fn status_of(status: CallStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// What a change of settings changed, each field from what it was to what it is; none where it changed none.
fn changes_of(changed: &impl serde::Serialize) -> Option<String> {
    let serde_json::Value::Object(fields) = serde_json::to_value(changed).ok()? else {
        return None;
    };
    let shown: Vec<String> = fields
        .iter()
        .map(|(name, change)| format!("{name} {} → {}", change["from"], change["to"]))
        .collect();
    (!shown.is_empty()).then(|| shown.join(", "))
}

/// `text` cut to `CALL_CHARS_MAX` characters, an ellipsis where it was cut.
fn cut(text: &str) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(CALL_CHARS_MAX).collect();
    if chars.next().is_some() { format!("{head}…") } else { head }
}

/// Keywords, and how a text is to hold them: every one, or any.
#[derive(Debug, Clone, Default)]
pub struct Keywords {
    pub words: Vec<String>,
    pub matching: Matching,
}

/// How a text holds keywords: every one of them — AND — or any one — OR.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Matching {
    #[default]
    All,
    Any,
}

impl Keywords {
    /// The keywords of `text`, separated by spaces, to be held as `matching` says.
    #[must_use]
    pub fn of(text: &str, matching: Matching) -> Self {
        let words = text.split_whitespace().map(str::to_owned).collect();
        Self { words, matching }
    }

    /// Whether there are none.
    #[must_use]
    pub fn are_none(&self) -> bool {
        self.words.is_empty()
    }

    /// The words of `text` around the first keyword it holds, on one line — where it holds them as they are to be
    /// held, whatever their case; none where it does not, or there are no keywords.
    #[must_use]
    pub fn around(&self, text: &str) -> Option<String> {
        let text_folded = folded(text);
        let found: Vec<Option<usize>> = self.words.iter().map(|word| text_folded.find(&folded(word))).collect();
        let held = match self.matching {
            Matching::All => found.iter().all(Option::is_some),
            Matching::Any => found.iter().any(Option::is_some),
        };
        let first = found.into_iter().flatten().min().filter(|_| held && !self.are_none())?;
        let at = text_folded[..first].chars().count();
        let start = at.saturating_sub(AROUND_BEFORE);
        let chars: Vec<char> = text.chars().collect();
        let end = chars.len().min(start + AROUND_CHARS);
        let words: String = chars[start..end]
            .iter()
            .map(|&char| if char == '\n' { ' ' } else { char })
            .collect();
        let before = if start > 0 { "…" } else { "" };
        let after = if end < chars.len() { "…" } else { "" };
        Some(format!("{before}{words}{after}"))
    }
}

/// How many characters of a text a search tells around a match, and how many of them before it.
const AROUND_CHARS: usize = 240;
const AROUND_BEFORE: usize = 80;

/// `text` with each character in lower case — its first, where its lower case is more than one — so that it holds as
/// many characters as `text`, each where `text` holds it.
fn folded(text: &str) -> String {
    text.chars().map(|char| char.to_lowercase().next().unwrap_or(char)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_preserves_unicode_and_bounds_the_excerpt_around_the_match() {
        let text = format!("{}ÉCOLE 中文\n结尾{}", "前".repeat(300), "后".repeat(300));
        let excerpt = Keywords::of("école 中文", Matching::All).around(&text).unwrap();
        assert!(excerpt.contains("ÉCOLE 中文 结尾"));
        assert!(excerpt.starts_with('…') && excerpt.ends_with('…'));
        assert_eq!(excerpt.chars().count(), AROUND_CHARS + 2);
        assert!(Keywords::of("école missing", Matching::All).around(&text).is_none());
        assert!(Keywords::of("école missing", Matching::Any).around(&text).is_some());
        assert!(Keywords::of("  ", Matching::All).around(&text).is_none());
    }
}
