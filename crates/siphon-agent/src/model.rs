//! A thread's model, as Siphon asks it: a `siphon.model` request — a record of what to ask — rendered from the thread's
//! window when it is answered, sent through `octoii-ai`, and answered with the response as it came: completed, cut
//! short, or failed with what its provider said. What streams while it is answered is published on the thread, by the
//! request's key, for those that follow it — but of one that asks an instruction after the window, a title or a recap;
//! a stop cuts the stream, and the response answered is what had come.

use std::sync::Arc;

use bytes::Bytes;
use octoii_ai::models::Models;
use octoii_ai::stream::{Event, Part};
use octoii_ai::vocabulary::{Item, Response};
use serde::{Deserialize, Serialize};
use siphon_runtime::{Asked, Request, Unanswered};
use siphon_threads::Window;

use crate::render;

/// What a model request asks: what follows the window of the thread `thread`, by its id, up to its entry `until`, of
/// the model `model`, by its reference; or, where `instruction` is given, what it asks after them, answered in words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRequest {
    pub thread: String,
    pub until: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
}

impl Request for ModelRequest {
    const NAME: &'static str = "siphon.model";
    type Answer = Response;
}

/// What streams of a reply as it is written, as the thread's followers are told it: an item begun at its place in the
/// output, text added to one, and one whole.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Streamed<'a> {
    Began { index: u32, item: &'a Item },
    Delta { index: u32, part: &'static str, text: &'a str },
    Ended { index: u32, item: &'a Item },
}

/// The name a part text is added to is told by.
fn part_name(part: Part) -> &'static str {
    match part {
        Part::Text => "text",
        Part::Refusal => "refusal",
        Part::Reasoning => "reasoning",
        Part::Summary => "summary",
        Part::Arguments => "arguments",
    }
}

/// Answers the model request `asked` through `models`: the response as it came, what streamed published on its
/// thread; under a stop, what had come.
///
/// # Errors
/// `Failed` where the window cannot be read, or no longer holds the entry the request asks up to.
pub async fn answer_model(models: &Arc<Models>, asked: Asked<ModelRequest>) -> Result<Response, Unanswered> {
    let request = &asked.payload;
    let snapshot = asked.runtime.snapshot().map_err(|error| Unanswered::Failed(error.to_string()))?;
    let mut window = Window::read(&snapshot, &request.thread).map_err(|error| Unanswered::Failed(error.to_string()))?;
    let end = window
        .conversation
        .iter()
        .position(|entry| entry.id() == request.until)
        .ok_or_else(|| Unanswered::Failed(format!("the window no longer holds the entry {}", request.until)))?;
    window.conversation.truncate(end + 1);
    let served = models.model(&request.model);
    let rendered = render::request(
        &request.thread,
        &request.model,
        served.as_ref(),
        &window,
        request.instruction.as_deref(),
    );
    let mut stream = models.text(&rendered).await;
    loop {
        let event = tokio::select! {
            event = stream.next() => event,
            () = asked.cancelled.cancelled() => return Ok(stream.cancel()),
        };
        let told = match &event {
            Some(Event::Began { index, item }) => Streamed::Began { index: *index, item },
            Some(Event::Delta { index, part, text }) => Streamed::Delta {
                index: *index,
                part: part_name(*part),
                text,
            },
            Some(Event::Ended { index, item }) => Streamed::Ended { index: *index, item },
            Some(Event::Finished(response)) => return Ok(response.clone()),
            None => return Err(Unanswered::Failed("the model's stream ended before its response".into())),
        };
        // What a request asks after the window, a title or a recap, is no reply the thread's followers are shown.
        if request.instruction.is_none()
            && let Ok(data) = serde_json::to_vec(&told)
        {
            asked.publish(&request.thread, Bytes::from(data));
        }
    }
}

/// The words `response`'s messages gave, trimmed.
#[must_use]
pub fn words_of(response: &Response) -> String {
    let mut words = String::new();
    for item in &response.output {
        let Item::Message(octoii_ai::vocabulary::Message::Assistant { content, .. }) = item else {
            continue;
        };
        for part in content {
            if let octoii_ai::vocabulary::OutputContent::OutputText { text, .. } = part {
                words.push_str(text);
            }
        }
    }
    words.trim().to_owned()
}
