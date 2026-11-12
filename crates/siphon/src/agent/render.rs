//! A model request rendered from a thread's window, every part of it from the log, so what the window begins with stays
//! as it was sent and every provider's cache of it holds: the system entries it begins with as developer messages, a
//! part for each section, their tools the request's tools; its compaction's recap; the effort the window is asked at,
//! as it began and as the changes before its first reply set it; then its conversation — a later system entry as the
//! sections it changed, a later change of the effort where it stands, each input as the person's message — or, where it
//! told of work the thread began, a system reminder — each reply as the items its response gave, each result as the
//! output its call gave, and a turn the person stopped as a note that they stopped it there. It is rendered for the
//! model asked, where its facts are known: a window that declares a search its model does not run leaves it out, and a
//! model that reads no images is told of each in words in its place. A request with an instruction asks it after the
//! window — a title, a recap — the tools declared as ever, so the window's cache holds, but none to be called.

use crate::threads::{Entry, Outcome, PERSON, Section, Window};
use octoii_ai::catalogue::Model;
use octoii_ai::vocabulary::{ConfigurationUpdate, InputContent, Item, Message, ReasoningConfig, Request, Tool, ToolChoice};

/// What the model is told where the person stopped a turn: what came before was cut off there, not finished.
pub const STOPPED_NOTE: &str = "The person stopped this turn here.";

/// The request of the model `reference`, whose facts are `served` where they are known, for what follows `window` of
/// the thread `thread`, which names the conversation a provider keeps its cache by; or for what `instruction` asks
/// after it, where one is given.
#[must_use]
pub fn request(thread: &str, reference: &str, served: Option<&Model>, window: &Window, instruction: Option<&str>) -> Request {
    let searches = served.is_none_or(|model| model.searches_web);
    let mut input = Vec::new();
    let mut tools = Vec::new();
    for entry in &window.told {
        if let Entry::System {
            sections, tools: declared, ..
        } = entry
        {
            tools.extend(declared.iter().filter(|tool| searches || **tool != Tool::WebSearch).cloned());
            if !sections.is_empty() {
                input.push(told(sections));
            }
        }
    }
    if let Some(recap) = &window.recap {
        input.push(developer(vec![InputContent::text(format!("<recap>\n{recap}\n</recap>"))]));
    }
    // The window's own first reply: a change of the effort before it is how the window is asked.
    let first_reply = window
        .conversation
        .iter()
        .enumerate()
        .position(|(at, entry)| at >= window.carried && matches!(entry, Entry::Reply { .. }))
        .unwrap_or(window.conversation.len());
    let mut effort = window.effort;
    for (at, entry) in window.conversation.iter().enumerate() {
        match entry {
            Entry::Config { changed, .. } => {
                let Some(change) = &changed.effort else { continue };
                if at < first_reply {
                    effort = change.to;
                } else if let Some(to) = change.to {
                    input.push(Item::ConfigurationUpdate(ConfigurationUpdate { effort: to }));
                }
            }
            // What told the thread of work it began — a terminal's command ending, a sub-thread's work in the
            // background — is a system reminder, not words of the person's, nor a task a thread gave.
            Entry::Input {
                actor, content, reply_to, ..
            } if actor != PERSON && reply_to.is_none() => input.push(developer(content.clone())),
            Entry::Input { content, .. } => input.push(Item::Message(Message::User { content: content.clone() })),
            Entry::System { sections, .. } if !sections.is_empty() => input.push(told_again(sections)),
            Entry::Reply { response, .. } => input.extend(response.output.iter().cloned()),
            Entry::Result { output, .. } => input.push(Item::FunctionCallOutput(output.clone())),
            Entry::TurnEnd {
                outcome: Outcome::Stopped, ..
            } => input.push(developer(vec![InputContent::text(STOPPED_NOTE)])),
            Entry::System { .. } | Entry::TurnEnd { .. } | Entry::Compaction { .. } | Entry::Discarded { .. } => {}
        }
    }
    if let Some(instruction) = instruction {
        input.push(Item::Message(Message::User {
            content: vec![InputContent::text(instruction)],
        }));
    }
    if served.is_some_and(|model| !model.reads_images) {
        for item in &mut input {
            tell_images(item, reference);
        }
    }
    let tool_choice = (instruction.is_some() && !tools.is_empty()).then_some(ToolChoice::None);
    Request {
        model: reference.to_owned(),
        instructions: None,
        input,
        tools,
        tool_choice,
        reasoning: effort.map(|effort| ReasoningConfig { effort }),
        format: None,
        max_output_tokens: None,
        prompt_cache_key: Some(thread.to_owned()),
    }
}

/// A system entry's sections as the model is sent them: a part for each, its text as the entry holds it, or a note of
/// a section that no longer applies.
fn told(sections: &[Section]) -> Item {
    let parts = sections
        .iter()
        .map(|section| match &section.text {
            Some(text) => InputContent::text(text.clone()),
            None => InputContent::text(format!("The section {} no longer applies.", section.name)),
        })
        .collect();
    developer(parts)
}

/// A later system entry's sections as the model is sent them: each changed section, named as the one it replaces, or
/// named as one that no longer applies.
fn told_again(sections: &[Section]) -> Item {
    let parts = sections
        .iter()
        .map(|section| match &section.text {
            Some(text) => InputContent::text(format!("The section {} is now:\n{text}", section.name)),
            None => InputContent::text(format!("The section {} no longer applies.", section.name)),
        })
        .collect();
    developer(parts)
}

fn developer(content: Vec<InputContent>) -> Item {
    Item::Message(Message::Developer { content })
}

/// `item`, a message or a call's output, with each image it holds told in words in its place, to the model
/// `reference`, which reads none.
fn tell_images(item: &mut Item, reference: &str) {
    let content = match item {
        Item::Message(Message::User { content } | Message::Developer { content } | Message::System { content }) => content,
        Item::FunctionCallOutput(output) => &mut output.output,
        _ => return,
    };
    for part in content {
        if matches!(part, InputContent::InputImage { .. }) {
            *part = InputContent::text(format!("[an image, not shown: the model {reference} reads no images]"));
        }
    }
}
