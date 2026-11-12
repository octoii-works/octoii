//! A conversation as Anthropic's messages carry it, as Claude Code's sessions replicate it. Each prefix is known by a
//! hash, as a prefix cache knows what it holds: each message's fingerprint — what comes back however it is sent again:
//! who said it, its words, its calls and their results, never the thinking a model signs — chained from the system
//! prompt. And what a session standing after one of its messages is given of the rest: the results of the calls that
//! message made, as results; what it has not seen — turns another model answered — as history between
//! `<conversation-checkpoint>` tags, calls and results written out; then the person's latest words as they are.

use serde_json::{Value, json};
use siphon::tools::sha256_hex;

/// The hash a conversation under `system` begins from, before its first message.
#[must_use]
pub fn root(system: &str) -> String {
    sha256_hex(format!("system\u{0}{system}").as_bytes())
}

/// The hash of the prefix that ends with `message`, after the prefix of hash `before`.
#[must_use]
pub fn next(before: &str, message: &Value) -> String {
    sha256_hex(format!("{before}\u{0}{}", fingerprint(message)).as_bytes())
}

/// The hashes of the prefixes of `messages` under `system`: the i-th that of the first i + 1.
#[must_use]
pub fn chain(system: &str, messages: &[Value]) -> Vec<String> {
    let mut at = root(system);
    messages
        .iter()
        .map(|message| {
            at = next(&at, message);
            at.clone()
        })
        .collect()
}

/// The blocks of a message: its words alone as one text block.
#[must_use]
pub fn blocks(message: &Value) -> Vec<Value> {
    match &message["content"] {
        Value::String(text) => vec![json!({ "type": "text", "text": text })],
        Value::Array(blocks) => blocks.clone(),
        _ => Vec::new(),
    }
}

/// What of a message comes back the same however it is sent: who said it, then each block's own, adjacent texts as
/// one, as JSON whose keys are in order.
fn fingerprint(message: &Value) -> String {
    let mut parts: Vec<Value> = vec![message["role"].clone()];
    for block in blocks(message) {
        let Some(part) = part(&block) else { continue };
        if let (Some(Value::Array(last)), Value::Array(this)) = (parts.last_mut(), &part)
            && last.first() == Some(&json!("text"))
            && this.first() == Some(&json!("text"))
        {
            let joined = format!("{}{}", last[1].as_str().unwrap_or_default(), this[1].as_str().unwrap_or_default());
            last[1] = json!(joined);
            continue;
        }
        parts.push(part);
    }
    canonical(&Value::Array(parts))
}

/// `value` as JSON whose objects' keys are in order, so one value has one text however it was written.
#[must_use]
pub fn canonical(value: &Value) -> String {
    match value {
        Value::Array(items) => format!("[{}]", items.iter().map(canonical).collect::<Vec<_>>().join(",")),
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by_key(|(key, _)| *key);
            let entries: Vec<String> = entries
                .into_iter()
                .map(|(key, value)| format!("{}:{}", Value::String(key.clone()), canonical(value)))
                .collect();
            format!("{{{}}}", entries.join(","))
        }
        other => other.to_string(),
    }
}

/// What of a block is kept however it comes back: none of a model's thinking, whose signature is its own.
fn part(block: &Value) -> Option<Value> {
    Some(match block["type"].as_str()? {
        "text" => json!(["text", block["text"]]),
        "image" => {
            let source = &block["source"];
            let seen = if source["type"] == "base64" {
                json!(sha256_hex(source["data"].as_str().unwrap_or_default().as_bytes()))
            } else {
                source["url"].clone()
            };
            json!(["image", seen])
        }
        "document" => json!(["document", block["source"]]),
        "tool_use" => json!(["tool_use", block["id"], block["name"], block["input"]]),
        "tool_result" => {
            let content: Vec<Value> = match &block["content"] {
                Value::String(text) => vec![json!(["text", text])],
                Value::Array(blocks) => blocks.iter().filter_map(part).collect(),
                _ => Vec::new(),
            };
            json!(["tool_result", block["tool_use_id"], block["is_error"] == true, content])
        }
        _ => return None,
    })
}

/// What a session is given: the results of its own last calls, then the rest, in one message.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Told {
    pub results: Vec<Value>,
    pub rest: Vec<Value>,
}

/// What a session standing after `messages[at]` — none, at no message — is given of the conversation past it.
#[must_use]
pub fn told_after(messages: &[Value], at: Option<usize>) -> Told {
    let standing = at.and_then(|at| messages.get(at));
    let calls: Vec<Value> = standing
        .filter(|message| message["role"] == "assistant")
        .map(|message| {
            blocks(message)
                .into_iter()
                .filter(|block| block["type"] == "tool_use")
                .map(|block| block["id"].clone())
                .collect()
        })
        .unwrap_or_default();
    let from = at.map_or(0, |at| at + 1);
    let past: Vec<(Value, Value)> = messages[from.min(messages.len())..]
        .iter()
        .flat_map(|message| blocks(message).into_iter().map(|block| (message["role"].clone(), block)))
        .collect();
    let answered = past
        .iter()
        .take_while(|(role, block)| *role == "user" && block["type"] == "tool_result" && calls.contains(&block["tool_use_id"]))
        .count();
    let words = past[answered..]
        .iter()
        .rev()
        .take_while(|(role, block)| is_words(role, block))
        .count();
    let unseen = &past[answered..past.len() - words];
    let mut rest = if unseen.is_empty() { Vec::new() } else { checkpoint(unseen) };
    rest.extend(past[past.len() - words..].iter().map(|(_, block)| block.clone()));
    Told {
        results: past[..answered].iter().map(|(_, block)| block.clone()).collect(),
        rest,
    }
}

/// Whether a block is the person's own words or an attachment of theirs.
fn is_words(role: &Value, block: &Value) -> bool {
    *role == "user" && matches!(block["type"].as_str(), Some("text" | "image" | "document"))
}

/// How history a session has not seen begins.
const OPENING: &str = "<conversation-checkpoint>\nThe following is conversation history you have not seen. Treat it as prior conversation, not a new request. Continue with the messages after this checkpoint.";

/// `parts`, history a session has not seen, as blocks of words between checkpoint tags.
fn checkpoint(parts: &[(Value, Value)]) -> Vec<Value> {
    let text = |text: String| json!({ "type": "text", "text": text });
    let mut out = vec![text(OPENING.to_owned())];
    let mut role: Option<&str> = None;
    for (who, block) in parts {
        let Some(told) = told_block(block) else { continue };
        let who = who.as_str().unwrap_or("user");
        if role != Some(who) {
            if let Some(role) = role {
                out.push(text(format!("</{role}>")));
            }
            out.push(text(format!("<{who}>")));
            role = Some(who);
        }
        out.push(told);
    }
    if let Some(role) = role {
        out.push(text(format!("</{role}>")));
    }
    out.push(text("</conversation-checkpoint>".to_owned()));
    out
}

/// A block of history as a session reads it: words and attachments as they are, calls and results written out.
fn told_block(block: &Value) -> Option<Value> {
    match block["type"].as_str()? {
        "text" | "image" | "document" => Some(block.clone()),
        "tool_use" => {
            let call = json!({ "id": block["id"], "name": block["name"], "input": block["input"] });
            Some(json!({ "type": "text", "text": format!("<tool-call>\n{call}\n</tool-call>") }))
        }
        "tool_result" => {
            let content = if block["content"].is_null() {
                json!("")
            } else {
                block["content"].clone()
            };
            let result = json!({ "id": block["tool_use_id"], "is_error": block["is_error"] == true, "content": content });
            Some(json!({ "type": "text", "text": format!("<tool-result>\n{result}\n</tool-result>") }))
        }
        _ => None,
    }
}
