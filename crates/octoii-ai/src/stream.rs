//! A text request's stream, made whole and safe. A driver streams its provider's answer in the vocabulary's terms — an
//! item begun, text added to one, an item whole, the response finished — and this makes of it what every caller may
//! trust: items numbered from 0 without a gap in the order they began; each stamped with the model that wrote it; each
//! call a `call_id` of its own, one empty or another call's given a new one from the host's mint — every other id the
//! provider gave carried as it came, as its data. Whatever ends it — the response, a failure, a provider gone silent
//! or done too soon — every item begun is ended, then the response is told whole, with every item in its output and
//! its cost where the provider gave none and the catalogue prices it. Cancelled, it gives what had come.

use std::collections::{HashMap, HashSet, VecDeque};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures::{Stream, StreamExt};

use crate::catalogue::{Conditions, Model, Pricing};
use crate::vocabulary::{Failure, FailureKind, Incomplete, Item, ItemStatus, Message, Origin, OutputContent, Response, ResponseStatus};

/// How long a response may take to begin.
pub const START_TIMEOUT: Duration = Duration::from_mins(5);
/// How long a response may fall silent, as a model may reason that long without a word.
pub const IDLE_TIMEOUT: Duration = Duration::from_mins(15);

/// What text is added to: an answer's words, a refusal's, the reasoning, its summary, or a call's arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Text,
    Refusal,
    Reasoning,
    Summary,
    Arguments,
}

/// What happens as a response is written: an item begun at its place in the output, text added to it, it whole, and
/// the response, whole, last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Began { index: u32, item: Item },
    Delta { index: u32, part: Part, text: String },
    Ended { index: u32, item: Item },
    Finished(Response),
}

/// What a driver streams: its provider's answer as events, the places its provider's; a failure ends it.
pub type Source = Pin<Box<dyn Stream<Item = Result<Event, Failure>> + Send>>;

/// Where new ids come from, unique and rising, as the host mints them.
pub type Mint = Arc<dyn Fn() -> String + Send + Sync>;

/// An item as it is written: its parts so far, and whether it is whole.
#[derive(Debug)]
struct Written {
    item: Item,
    whole: bool,
}

/// A text request's stream, made whole.
pub struct TextStream {
    source: Option<Source>,
    reference: String,
    pricing: Option<Pricing>,
    sent_at_ms: i64,
    mint: Mint,
    /// The place each of the provider's items has, by the provider's place.
    places: HashMap<u32, u32>,
    written: Vec<Written>,
    call_ids: HashSet<String>,
    begun: bool,
    /// Events made ready, to be given before the source is read again.
    ready: VecDeque<Event>,
    finished: bool,
}

impl std::fmt::Debug for TextStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextStream")
            .field("reference", &self.reference)
            .field("items", &self.written.len())
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

impl TextStream {
    /// The stream of `source`, the answer of `model` to a request sent at `sent_at_ms`.
    #[must_use]
    pub fn new(source: Source, model: &Model, sent_at_ms: i64, mint: Mint) -> Self {
        Self {
            source: Some(source),
            reference: model.reference(),
            pricing: model.pricing.clone(),
            sent_at_ms,
            mint,
            places: HashMap::new(),
            written: Vec::new(),
            call_ids: HashSet::new(),
            begun: false,
            ready: VecDeque::new(),
            finished: false,
        }
    }

    /// The next event; none after the response.
    pub async fn next(&mut self) -> Option<Event> {
        loop {
            if let Some(event) = self.ready.pop_front() {
                return Some(event);
            }
            if self.finished {
                return None;
            }
            let source = self.source.as_mut()?;
            let waited = if self.begun { IDLE_TIMEOUT } else { START_TIMEOUT };
            match tokio::time::timeout(waited, source.next()).await {
                Err(_) => self.fail(Failure::new(
                    FailureKind::Timeout,
                    format!("the provider said nothing for {}s", waited.as_secs()),
                )),
                Ok(None) => self.fail(Failure::new(
                    FailureKind::InvalidResponse,
                    "the provider's stream ended before its response",
                )),
                Ok(Some(Err(failure))) => self.fail(failure),
                Ok(Some(Ok(event))) => {
                    self.begun = true;
                    self.take(event);
                }
            }
        }
    }

    /// The response so far, its stream let go of, as a caller that stops it keeps it: every item begun ended, and the
    /// response cut off with what had come.
    #[must_use]
    pub fn cancel(mut self) -> Response {
        self.source = None;
        let output = self.end_all();
        Response {
            id: (self.mint)(),
            model: self.reference,
            status: ResponseStatus::Incomplete,
            output,
            usage: None,
            failure: None,
            incomplete: Some(Incomplete::Cancelled),
            context_tokens: None,
        }
    }

    fn take(&mut self, event: Event) {
        match event {
            Event::Began { index, item } => {
                let place = self.place(index);
                let item = self.stamped(item);
                self.written[place as usize] = Written {
                    item: item.clone(),
                    whole: false,
                };
                self.ready.push_back(Event::Began { index: place, item });
            }
            Event::Delta { index, part, text } => {
                let Some(place) = self.places.get(&index).copied() else { return };
                added(&mut self.written[place as usize].item, part, &text);
                self.ready.push_back(Event::Delta { index: place, part, text });
            }
            Event::Ended { index, item } => {
                let place = self.place(index);
                let item = self.stamped_as_begun(place, item);
                self.written[place as usize] = Written {
                    item: item.clone(),
                    whole: true,
                };
                self.ready.push_back(Event::Ended { index: place, item });
            }
            Event::Finished(response) => self.finish(response),
        }
    }

    /// The place of the provider's item at `index`, given it as it first comes.
    fn place(&mut self, index: u32) -> u32 {
        if let Some(place) = self.places.get(&index) {
            return *place;
        }
        let place = u32::try_from(self.written.len()).expect("fewer items than places");
        self.places.insert(index, place);
        self.written.push(Written {
            item: Item::Message(Message::Assistant {
                id: None,
                content: Vec::new(),
                status: None,
                origin: None,
            }),
            whole: false,
        });
        place
    }

    /// `item` stamped with the model that wrote it, a call given a `call_id` of its own.
    fn stamped(&mut self, mut item: Item) -> Item {
        let origin = |origin: &mut Option<Origin>, reference: &str| {
            let given = origin.get_or_insert_with(|| Origin {
                model: String::new(),
                provider_metadata: None,
            });
            if given.model.is_empty() {
                reference.clone_into(&mut given.model);
            }
        };
        match &mut item {
            Item::Message(Message::Assistant { origin: given, .. }) => origin(given, &self.reference),
            Item::FunctionCall(call) => {
                origin(&mut call.origin, &self.reference);
                if call.call_id.is_empty() || !self.call_ids.insert(call.call_id.clone()) {
                    call.call_id = (self.mint)();
                    self.call_ids.insert(call.call_id.clone());
                }
            }
            Item::Reasoning(reasoning) if reasoning.origin.model.is_empty() => self.reference.clone_into(&mut reasoning.origin.model),
            Item::WebSearchCall(search) => origin(&mut search.origin, &self.reference),
            _ => {}
        }
        item
    }

    /// `item` whole as it ends at `place`: a call keeps the `call_id` it began with.
    fn stamped_as_begun(&mut self, place: u32, mut item: Item) -> Item {
        let begun = match &self.written[place as usize].item {
            Item::FunctionCall(call) if !call.call_id.is_empty() => Some(call.call_id.clone()),
            _ => None,
        };
        if let (Some(begun), Item::FunctionCall(call)) = (begun, &mut item) {
            call.call_id = begun;
            call.origin.get_or_insert_with(|| Origin {
                model: self.reference.clone(),
                provider_metadata: None,
            });
            return item;
        }
        self.stamped(item)
    }

    /// Ends every item begun and not ended, cut off; the output, whole.
    fn end_all(&mut self) -> Vec<Item> {
        for (place, written) in self.written.iter_mut().enumerate() {
            if written.whole {
                continue;
            }
            cut(&mut written.item);
            written.whole = true;
            self.ready.push_back(Event::Ended {
                index: u32::try_from(place).expect("fewer items than places"),
                item: written.item.clone(),
            });
        }
        self.written.iter().map(|written| written.item.clone()).collect()
    }

    fn finish(&mut self, mut response: Response) {
        response.output = self.end_all();
        response.model.clone_from(&self.reference);
        if let (Some(usage), Some(pricing)) = (&mut response.usage, &self.pricing)
            && usage.cost_nanodollars.is_none()
        {
            let conditions = Conditions {
                context_tokens: response.context_tokens,
                sent_at_ms: self.sent_at_ms,
            };
            usage.cost_nanodollars = pricing.cost(usage, conditions);
        }
        self.ready.push_back(Event::Finished(response));
        self.finished = true;
        self.source = None;
    }

    fn fail(&mut self, failure: Failure) {
        let response = Response {
            id: (self.mint)(),
            model: self.reference.clone(),
            status: ResponseStatus::Failed,
            output: Vec::new(),
            usage: None,
            failure: Some(failure),
            incomplete: None,
            context_tokens: None,
        };
        self.finish(response);
    }
}

/// `text` added to `item`'s `part`.
fn added(item: &mut Item, part: Part, text: &str) {
    match (item, part) {
        (Item::Message(Message::Assistant { content, .. }), Part::Text) => match content.last_mut() {
            Some(OutputContent::OutputText { text: words, .. }) => words.push_str(text),
            _ => content.push(OutputContent::OutputText {
                text: text.to_owned(),
                annotations: Vec::new(),
            }),
        },
        (Item::Message(Message::Assistant { content, .. }), Part::Refusal) => match content.last_mut() {
            Some(OutputContent::Refusal { refusal }) => refusal.push_str(text),
            _ => content.push(OutputContent::Refusal { refusal: text.to_owned() }),
        },
        (Item::Reasoning(reasoning), Part::Reasoning) => push_onto(&mut reasoning.content, text),
        (Item::Reasoning(reasoning), Part::Summary) => push_onto(&mut reasoning.summary, text),
        (Item::FunctionCall(call), Part::Arguments) => call.arguments.push_str(text),
        _ => {}
    }
}

fn push_onto(parts: &mut Vec<String>, text: &str) {
    match parts.last_mut() {
        Some(last) => last.push_str(text),
        None => parts.push(text.to_owned()),
    }
}

/// `item` cut off where it stood.
fn cut(item: &mut Item) {
    match item {
        Item::Message(Message::Assistant { status, .. }) => *status = Some(ItemStatus::Incomplete),
        Item::FunctionCall(call) => call.status = Some(ItemStatus::Incomplete),
        Item::Reasoning(reasoning) => reasoning.status = Some(ItemStatus::Incomplete),
        _ => {}
    }
}
