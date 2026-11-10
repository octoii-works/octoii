//! A text stream made whole: items numbered without a gap and stamped, calls given ids of their own, every item ended
//! whatever ends the stream, a cost where the catalogue prices it, and what had come when it is cancelled.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use futures::StreamExt;
use octoii_ai::catalogue::{Features, InputLimits, Model, Prices, Pricing, Rate};
use octoii_ai::stream::{Event, Mint, Part, Source, TextStream};
use octoii_ai::vocabulary::{
    Failure, FailureKind, FunctionCall, Incomplete, Item, ItemStatus, Message, OutputContent, Response, ResponseStatus, Usage,
};

fn model() -> Model {
    Model {
        id: "m".into(),
        provider: "p".into(),
        name: "M".into(),
        context_length: None,
        max_output_tokens: None,
        reads_images: false,
        reads_files: false,
        reasoning: None,
        parameters: None,
        pricing: Some(Pricing {
            base: Prices {
                prompt: Some(Rate::of_dollars("0.000001")),
                completion: Some(Rate::of_dollars("0.000002")),
                ..Prices::default()
            },
            ..Pricing::default()
        }),
        release_date: None,
        searches_web: false,
        limits: InputLimits::default(),
        features: Features::default(),
    }
}

fn mint() -> Mint {
    let next = Arc::new(AtomicU32::new(0));
    Arc::new(move || format!("minted-{}", next.fetch_add(1, Ordering::Relaxed)))
}

fn source(events: Vec<Result<Event, Failure>>) -> Source {
    Box::pin(futures::stream::iter(events))
}

fn assistant() -> Item {
    Item::Message(Message::Assistant {
        id: Some("msg_1".into()),
        content: vec![],
        status: None,
        origin: None,
    })
}

fn call(call_id: &str) -> Item {
    Item::FunctionCall(FunctionCall {
        id: None,
        call_id: call_id.into(),
        name: "read".into(),
        arguments: String::new(),
        status: None,
        origin: None,
    })
}

fn finished(usage: Option<Usage>) -> Event {
    Event::Finished(Response {
        id: "resp_1".into(),
        model: String::new(),
        status: ResponseStatus::Completed,
        output: vec![],
        usage,
        failure: None,
        incomplete: None,
        context_tokens: None,
    })
}

async fn all(mut stream: TextStream) -> Vec<Event> {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }
    events
}

fn response(events: &[Event]) -> &Response {
    match events.last() {
        Some(Event::Finished(response)) => response,
        other => panic!("no response last: {other:?}"),
    }
}

#[tokio::test]
async fn items_are_numbered_without_a_gap_stamped_with_their_model_and_the_response_holds_them_with_its_cost() {
    let events = all(TextStream::new(
        source(vec![
            Ok(Event::Began {
                index: 0,
                item: assistant(),
            }),
            Ok(Event::Delta {
                index: 0,
                part: Part::Text,
                text: "hel".into(),
            }),
            Ok(Event::Delta {
                index: 0,
                part: Part::Text,
                text: "lo".into(),
            }),
            Ok(Event::Began {
                index: 3,
                item: call("c1"),
            }),
            Ok(Event::Delta {
                index: 3,
                part: Part::Arguments,
                text: "{}".into(),
            }),
            Ok(Event::Ended {
                index: 3,
                item: call("c1"),
            }),
            Ok(finished(Some(Usage {
                input_tokens: 100,
                output_tokens: 10,
                ..Usage::default()
            }))),
        ]),
        &model(),
        0,
        mint(),
    ))
    .await;
    assert!(
        matches!(events[3], Event::Began { index: 1, .. }),
        "the provider's 3 is the second item"
    );
    let response = response(&events);
    assert_eq!(response.model, "p/m");
    assert_eq!(response.output.len(), 2);
    let Item::Message(Message::Assistant {
        content, status, origin, ..
    }) = &response.output[0]
    else {
        panic!("a message")
    };
    assert_eq!(
        content,
        &[OutputContent::OutputText {
            text: "hello".into(),
            annotations: vec![]
        }]
    );
    assert_eq!(*status, Some(ItemStatus::Incomplete), "a message never ended is cut off");
    assert_eq!(origin.as_ref().map(|origin| origin.model.as_str()), Some("p/m"));
    assert_eq!(
        response.usage.and_then(|usage| usage.cost_nanodollars),
        Some(100 * 1000 + 10 * 2000)
    );
}

#[tokio::test]
async fn a_call_with_no_id_or_another_calls_is_given_one_of_its_own_and_keeps_it_as_it_ends() {
    let events = all(TextStream::new(
        source(vec![
            Ok(Event::Began {
                index: 0,
                item: call("c1"),
            }),
            Ok(Event::Began {
                index: 1,
                item: call("c1"),
            }),
            Ok(Event::Ended {
                index: 1,
                item: call("c1"),
            }),
            Ok(Event::Began { index: 2, item: call("") }),
            Ok(finished(None)),
        ]),
        &model(),
        0,
        mint(),
    ))
    .await;
    let ids: Vec<&str> = response(&events)
        .output
        .iter()
        .map(|item| match item {
            Item::FunctionCall(call) => call.call_id.as_str(),
            _ => "",
        })
        .collect();
    assert_eq!(ids, ["c1", "minted-0", "minted-1"]);
}

#[tokio::test]
async fn a_failure_ends_every_item_begun_and_the_response_fails_with_what_had_come() {
    let events = all(TextStream::new(
        source(vec![
            Ok(Event::Began {
                index: 0,
                item: call("c1"),
            }),
            Err(Failure::new(FailureKind::Server, "overloaded")),
        ]),
        &model(),
        0,
        mint(),
    ))
    .await;
    assert!(matches!(&events[1], Event::Ended { index: 0, item: Item::FunctionCall(call) } if call.status == Some(ItemStatus::Incomplete)));
    let response = response(&events);
    assert_eq!(response.status, ResponseStatus::Failed);
    assert_eq!(response.failure.as_ref().map(|failure| failure.kind), Some(FailureKind::Server));
    assert_eq!(response.output.len(), 1);
    // A provider done before its response is a failure of its own.
    let early = all(TextStream::new(source(vec![]), &model(), 0, mint())).await;
    assert_eq!(response_kind(&early), Some(FailureKind::InvalidResponse));
}

fn response_kind(events: &[Event]) -> Option<FailureKind> {
    response(events).failure.as_ref().map(|failure| failure.kind)
}

#[tokio::test(start_paused = true)]
async fn a_provider_that_never_begins_or_falls_silent_times_out() {
    let never: Source = Box::pin(futures::stream::pending());
    assert_eq!(
        response_kind(&all(TextStream::new(never, &model(), 0, mint())).await),
        Some(FailureKind::Timeout)
    );
    let silent: Source = Box::pin(
        futures::stream::iter([Ok(Event::Began {
            index: 0,
            item: assistant(),
        })])
        .chain(futures::stream::pending()),
    );
    let events = all(TextStream::new(silent, &model(), 0, mint())).await;
    assert_eq!(response_kind(&events), Some(FailureKind::Timeout));
    assert_eq!(response(&events).output.len(), 1);
}

#[tokio::test]
async fn a_stream_cancelled_gives_what_had_come_cut_off() {
    let mut stream = TextStream::new(
        Box::pin(
            futures::stream::iter([
                Ok(Event::Began {
                    index: 0,
                    item: assistant(),
                }),
                Ok(Event::Delta {
                    index: 0,
                    part: Part::Text,
                    text: "par".into(),
                }),
            ])
            .chain(futures::stream::pending()),
        ),
        &model(),
        0,
        mint(),
    );
    stream.next().await;
    stream.next().await;
    let response = stream.cancel();
    assert_eq!(
        (response.status, response.incomplete),
        (ResponseStatus::Incomplete, Some(Incomplete::Cancelled))
    );
    assert!(matches!(&response.output[0], Item::Message(Message::Assistant { content, .. }) if content.len() == 1));
}
