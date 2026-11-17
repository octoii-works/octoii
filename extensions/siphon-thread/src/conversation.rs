//! A thread's conversation as the `thread` tool reads it back: each entry of its log as a line or a few — the person's
//! words and another thread's, what the model said and each call it made with how it ended, a turn's end, a setting
//! changed and a summary of what came before — the model's reasoning and what it was told left out; and what of a
//! text holds the keywords asked for, every one or any, whatever their case.

use octoii::threads::{Entry, Outcome};
use octoii_ai::vocabulary::{FileRef, InputContent, Item, Message, OutputContent};

use crate::result::{ContentFile, ContentPart, ConversationContent, ConversationEntry, ModelContent, ReplyItem, ResultStatus, TurnOutcome};

/// The most characters of a call's arguments or of its result that are told; the whole is in the thread's log.
const CALL_CHARS_MAX: usize = 300;

/// The visible business fields of `entry`, taken directly from the conversation log.
#[must_use]
pub fn entry(entry: &Entry) -> Option<ConversationEntry> {
    let content = match entry {
        Entry::Input { actor, content, .. } => ConversationContent::Input {
            actor: actor.clone(),
            content: content.iter().map(ContentPart::from).collect(),
        },
        Entry::Reply { response, .. } => {
            let items: Vec<ReplyItem> = response.output.iter().filter_map(reply_item).collect();
            if items.is_empty() {
                return None;
            }
            ConversationContent::Reply { items }
        }
        Entry::Result { call, status, output, .. } => {
            let content: Vec<ContentPart> = output.output.iter().map(ContentPart::from).collect();
            let (text, clipped) = bounded(&words_of(&content));
            ConversationContent::Result {
                call: call.clone(),
                status: (*status).into(),
                text,
                clipped,
            }
        }
        Entry::TurnEnd { outcome, reason, .. } => ConversationContent::TurnEnd {
            outcome: match outcome {
                Outcome::Replied => TurnOutcome::Replied,
                Outcome::Failed => TurnOutcome::Failed,
                Outcome::Stopped => TurnOutcome::Stopped,
            },
            reason: reason.clone(),
        },
        Entry::Config { actor, changed, .. } => {
            let changes: std::collections::BTreeMap<String, crate::result::SettingChange> =
                serde_json::from_value(serde_json::json!(changed)).expect("setting changes have from and to values");
            if changes.is_empty() {
                return None;
            }
            ConversationContent::Config {
                actor: actor.clone(),
                changes,
            }
        }
        Entry::Compaction { response, .. } => ConversationContent::Compaction { text: response.words() },
        Entry::System { .. } | Entry::Discarded { .. } => return None,
    };
    Some(ConversationEntry {
        id: entry.id().to_owned(),
        at: octoii::threads::time_of(entry.id()),
        content,
    })
}

/// `entry` as the conversation tells it; none for hidden entries or replies with no visible content.
#[must_use]
pub fn told(entry: &Entry) -> Option<String> {
    self::entry(entry).map(|entry| entry.told())
}

impl ConversationEntry {
    /// The model-facing view of this entry, with the same omissions and limits as its business fields.
    pub(crate) fn told(&self) -> String {
        let at = self.at.map_or_else(String::new, |ms| format!("{} ", crate::tool::time(ms)));
        match &self.content {
            ConversationContent::Input { actor, content } => {
                let who = if actor == "person" {
                    "person".to_owned()
                } else {
                    format!("thread {actor}")
                };
                format!("{at}{who}: {}", words_of(content))
            }
            ConversationContent::Reply { items } => items.iter().map(ReplyItem::told).collect::<Vec<_>>().join("\n"),
            ConversationContent::Result { status, text, clipped, .. } => {
                let ending = if *clipped { "…" } else { "" };
                format!("result ({}): {text}{ending}", status_of(*status))
            }
            ConversationContent::TurnEnd { outcome, reason } => match outcome {
                TurnOutcome::Replied => format!("{at}turn ended"),
                TurnOutcome::Failed => format!("{at}turn failed{}", reason.as_ref().map_or_default(|reason| format!(": {reason}"))),
                TurnOutcome::Stopped => format!("{at}turn stopped"),
            },
            ConversationContent::Config { changes, .. } => {
                let shown: Vec<String> = changes
                    .iter()
                    .map(|(name, change)| format!("{name} {} → {}", change.from, change.to))
                    .collect();
                format!("{at}settings: {}", shown.join(", "))
            }
            ConversationContent::Compaction { text } => format!("{at}summary of what came before: {text}"),
        }
    }
}

/// A visible item of a model reply, excluding reasoning and provider instructions.
fn reply_item(item: &Item) -> Option<ReplyItem> {
    match item {
        Item::Message(Message::Assistant { content, .. }) => Some(ReplyItem::Message {
            content: content
                .iter()
                .map(|part| match part {
                    OutputContent::OutputText { text, .. } => ModelContent::Text { text: text.clone() },
                    OutputContent::Refusal { refusal } => ModelContent::Refusal { refusal: refusal.clone() },
                })
                .collect(),
        }),
        Item::FunctionCall(call) => {
            let (arguments, clipped) = bounded(&call.arguments);
            Some(ReplyItem::Call {
                call_id: call.call_id.clone(),
                name: call.name.clone(),
                arguments,
                clipped,
            })
        }
        Item::WebSearchCall(_) => Some(ReplyItem::WebSearch),
        _ => None,
    }
}

impl ReplyItem {
    fn told(&self) -> String {
        match self {
            Self::Message { content } => {
                let words: Vec<&str> = content
                    .iter()
                    .map(|part| match part {
                        ModelContent::Text { text } => text.as_str(),
                        ModelContent::Refusal { refusal } => refusal.as_str(),
                    })
                    .collect();
                format!("model: {}", words.join(""))
            }
            Self::Call {
                name, arguments, clipped, ..
            } => {
                let ending = if *clipped { "…" } else { "" };
                format!("call {name}: {arguments}{ending}")
            }
            Self::WebSearch => "web search".to_owned(),
        }
    }
}

impl From<&FileRef> for ContentFile {
    fn from(file: &FileRef) -> Self {
        Self {
            reference: file.reference.clone(),
            media_type: file.media_type.clone(),
            filename: file.filename.clone(),
        }
    }
}

impl From<&InputContent> for ContentPart {
    fn from(part: &InputContent) -> Self {
        match part {
            InputContent::InputText { text } => Self::Text { text: text.clone() },
            InputContent::InputImage { file, .. } => Self::Image { file: file.into() },
            InputContent::InputFile { file } => Self::File { file: file.into() },
            InputContent::ResourceLink { file, size } => Self::ResourceLink {
                file: file.into(),
                size: *size,
            },
        }
    }
}

/// The words of `content`, each image or file named in its place.
fn words_of(content: &[ContentPart]) -> String {
    content
        .iter()
        .map(|part| match part {
            ContentPart::Text { text } => text.clone(),
            ContentPart::Image { .. } => "[an image]".to_owned(),
            ContentPart::File { .. } => "[a file]".to_owned(),
            ContentPart::ResourceLink { file, .. } => format!("[file available: {}]", file.filename.as_deref().unwrap_or(&file.reference)),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// How a call ended, in the log's words.
fn status_of(status: ResultStatus) -> &'static str {
    match status {
        ResultStatus::Ok => "ok",
        ResultStatus::Unavailable => "unavailable",
        ResultStatus::InvalidArguments => "invalid-arguments",
        ResultStatus::ToolError => "tool-error",
        ResultStatus::Rejected => "rejected",
        ResultStatus::Interrupted => "interrupted",
    }
}

/// `text` bounded to `CALL_CHARS_MAX` characters, with whether any characters were left out.
fn bounded(text: &str) -> (String, bool) {
    let mut chars = text.chars();
    let head = chars.by_ref().take(CALL_CHARS_MAX).collect();
    (head, chars.next().is_some())
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
    use serde_json::json;

    fn source(value: serde_json::Value) -> Entry {
        serde_json::from_value(value).expect("a source log entry")
    }

    fn response(output: serde_json::Value) -> serde_json::Value {
        let mut response = json!({ "id": "response", "model": "model", "status": "completed" });
        response["output"] = output;
        response
    }

    fn visible(source: &Entry) -> ConversationEntry {
        let entry = entry(source).expect("a visible entry");
        let result = crate::result::ThreadResult::Read {
            thread: "thread".to_owned(),
            entries: vec![entry.clone()],
            before: None,
            after: None,
        };
        jsonschema::validate(&schemars::schema_for!(crate::result::ThreadResult).to_value(), &json!(result))
            .expect("visible entry fields fit the declared result schema");
        entry
    }

    #[test]
    fn inputs_keep_the_actor_text_and_file_reference_separate_from_display_labels() {
        let source = source(json!({
            "type": "input", "id": "input", "actor": "person", "delivery": "delivery",
            "content": [
                { "type": "input_text", "text": "model: literal text\\nwith labels" },
                { "type": "resource_link", "file": { "ref": "sha256:123", "media_type": "text/plain", "filename": "report.txt" }, "size": 42 }
            ]
        }));
        let entry = visible(&source);
        assert_eq!(entry.id, "input");
        assert_eq!(entry.at, None);
        let ConversationContent::Input { actor, content } = &entry.content else {
            panic!("an input");
        };
        assert_eq!(actor, "person");
        let file = serde_json::to_value(&content[1]).expect("a resource link");
        assert_eq!(file["file"]["ref"], "sha256:123");
        assert!(file["file"].get("reference").is_none());
        assert_eq!(
            content[0],
            ContentPart::Text {
                text: "model: literal text\\nwith labels".to_owned()
            }
        );
        assert_eq!(
            content[1],
            ContentPart::ResourceLink {
                file: ContentFile {
                    reference: "sha256:123".to_owned(),
                    media_type: "text/plain".to_owned(),
                    filename: Some("report.txt".to_owned()),
                },
                size: 42,
            }
        );
        assert_eq!(
            entry.told(),
            "person: model: literal text\\nwith labels\n[file available: report.txt]"
        );
    }

    #[test]
    fn replies_and_results_preserve_call_fields_and_unicode_clipping_without_exposing_reasoning() {
        let arguments = "字".repeat(CALL_CHARS_MAX + 1);
        let reply = source(json!({
            "type": "reply", "id": "reply", "request": "request",
            "response": response(json!([
                { "type": "reasoning", "content": ["private reasoning"], "origin": { "model": "model" } },
                { "type": "message", "role": "assistant", "content": [
                    { "type": "output_text", "text": "visible " }, { "type": "refusal", "refusal": "refusal" }
                ] },
                { "type": "function_call", "call_id": "call-id", "name": "shell", "arguments": arguments },
                { "type": "web_search_call", "id": "web", "status": "completed" }
            ]))
        }));
        let entry = visible(&reply);
        let ConversationContent::Reply { items } = &entry.content else {
            panic!("a reply");
        };
        assert_eq!(items.len(), 3);
        assert_eq!(
            items[1],
            ReplyItem::Call {
                call_id: "call-id".to_owned(),
                name: "shell".to_owned(),
                arguments: "字".repeat(CALL_CHARS_MAX),
                clipped: true,
            }
        );
        assert_eq!(
            entry.told(),
            format!("model: visible refusal\ncall shell: {}…\nweb search", "字".repeat(CALL_CHARS_MAX))
        );
        assert!(!json!(entry).to_string().contains("private reasoning"));

        let result = source(json!({
            "type": "result", "id": "result", "call": "siphon-key", "status": "tool-error",
            "output": { "call_id": "call-id", "output": [{ "type": "input_text", "text": arguments }], "is_error": true }
        }));
        let entry = visible(&result);
        assert_eq!(
            entry.content,
            ConversationContent::Result {
                call: "siphon-key".to_owned(),
                status: ResultStatus::ToolError,
                text: "字".repeat(CALL_CHARS_MAX),
                clipped: true,
            }
        );
        assert_eq!(entry.told(), format!("result (tool-error): {}…", "字".repeat(CALL_CHARS_MAX)));
        assert!(!bounded(&"字".repeat(CALL_CHARS_MAX)).1);
    }

    #[test]
    fn settings_turn_ends_and_summaries_keep_their_business_fields_and_text() {
        let configured = source(json!({
            "type": "config", "id": "config", "actor": "person",
            "title": { "from": "before", "to": "after" },
            "budget_nanodollars": { "from": null, "to": 42 }
        }));
        let entry = visible(&configured);
        let ConversationContent::Config { actor, changes } = &entry.content else {
            panic!("settings");
        };
        assert_eq!(actor, "person");
        assert_eq!(changes["title"].to, json!("after"));
        assert_eq!(changes["budget_nanodollars"].to, json!(42));
        assert_eq!(entry.told(), "settings: budget_nanodollars null → 42, title \"before\" → \"after\"");

        let ended = source(json!({ "type": "turn-end", "id": "ended", "outcome": "failed", "reason": "model unavailable" }));
        let entry = visible(&ended);
        assert_eq!(entry.told(), "turn failed: model unavailable");
        assert_eq!(
            entry.content,
            ConversationContent::TurnEnd {
                outcome: TurnOutcome::Failed,
                reason: Some("model unavailable".to_owned())
            }
        );

        let compacted = source(json!({
            "type": "compaction", "id": "compacted", "request": "request", "from": "input",
            "response": response(json!([{ "type": "message", "role": "assistant", "content": [{ "type": "output_text", "text": " earlier work " }] }]))
        }));
        let entry = visible(&compacted);
        assert_eq!(
            entry.content,
            ConversationContent::Compaction {
                text: "earlier work".to_owned()
            }
        );
        assert_eq!(entry.told(), "summary of what came before: earlier work");
    }

    #[test]
    fn hidden_entries_and_replies_without_visible_items_have_no_projection() {
        let reasoning = response(json!([{ "type": "reasoning", "content": ["private"], "origin": { "model": "model" } }]));
        let hidden = [
            source(json!({ "type": "system", "id": "system", "sections": [] })),
            source(json!({ "type": "discarded", "id": "discarded", "request": "request", "response": reasoning })),
            source(json!({ "type": "reply", "id": "reply", "request": "request", "response": reasoning })),
            source(json!({ "type": "config", "id": "config", "actor": "person" })),
        ];
        for source in hidden {
            assert_eq!(entry(&source), None);
            assert_eq!(told(&source), None);
        }
    }

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
