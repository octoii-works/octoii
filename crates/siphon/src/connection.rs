//! A client's connection, as Siphon serves it whatever carries it: the welcome first; then each call the client makes,
//! read, checked against what its session may do, and answered in a task of its own, so a slow call holds no other
//! back; and its subscriptions, each a task of its own that sends what its topic tells as it changes. What goes out
//! waits in one bounded queue: an answer waits for room, a subscription's event does not — a subscription the client
//! does not keep up with is let go, told it lagged and where it stood, for the client to make again from there.

mod calls;
mod git;
mod journal;
mod machine;
mod providers;
mod subscriptions;

use std::collections::HashMap;

use serde::Deserialize;
use siphon_protocol::{Answer, Call, ConnectionEvent, Error, Machine, Scope, Subscribed, Unsubscribe};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub(crate) use calls::answered;
pub(crate) use subscriptions::told;

use crate::Siphon;
use crate::plugin::{Context, Lagged, Sink, Subscription};

/// How many messages wait to go out to a client before a subscription's next event lets the subscription go.
pub const OUTGOING_AHEAD: usize = 256;

/// Who a connection's client is: its session, by its id, and what the session lets it do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    pub session: String,
    pub scopes: Vec<Scope>,
}

/// A message to send a client: a text frame of the protocol's JSON; or a binary frame of an event that carries bytes —
/// a painted page, a terminal's output — its event's JSON first, after its length as four bytes, big-endian, then the
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outgoing {
    Text(String),
    Binary(bytes::Bytes),
}

/// Serves a client of `access`, which reached `machine`: each message of `incoming` taken as a call, what is sent it
/// put on `outgoing` — the queue its carrier writes from, `OUTGOING_AHEAD` long — till `incoming` ends, when every
/// subscription of its ends with it.
pub async fn serve(
    siphon: Siphon,
    access: Access,
    machine: Machine,
    mut incoming: mpsc::Receiver<String>,
    outgoing: mpsc::Sender<Outgoing>,
) {
    let welcome = ConnectionEvent::Welcome {
        session: access.session.clone(),
        scopes: access.scopes.clone(),
        machine,
    };
    if outgoing.send(text(&welcome)).await.is_err() {
        return;
    }
    let context = Context {
        siphon,
        session: access.session.clone(),
    };
    let mut subscriptions = Subscriptions::default();
    while let Some(message) = incoming.recv().await {
        let call = match serde_json::from_str::<Call>(&message) {
            Ok(call) => call,
            Err(error) => {
                let refused = Answer::Error {
                    id: id_of(&message),
                    error: refusal("bad_call", &error.to_string()),
                };
                let _ = outgoing.send(text(&refused)).await;
                continue;
            }
        };
        let answered = match call.method.as_str() {
            "subscribe" => subscriptions.subscribe(&context, &access, call.params, &outgoing),
            "unsubscribe" => subscriptions.unsubscribe(call.params),
            name => {
                let Some((scope, handle)) = context.siphon.registry.methods.get(name) else {
                    let refused = refusal("no_such_method", &format!("there is no method {name}"));
                    let _ = outgoing.send(text(&answer(call.id, Err(refused)))).await;
                    continue;
                };
                if let Some(scope) = scope.filter(|scope| !access.scopes.contains(scope)) {
                    let _ = outgoing.send(text(&answer(call.id, Err(forbidden(scope))))).await;
                    continue;
                }
                let (doing, outgoing) = (handle(context.clone(), name, call.params), outgoing.clone());
                tokio::spawn(async move {
                    let _ = outgoing.send(text(&answer(call.id, doing.await))).await;
                });
                continue;
            }
        };
        // A subscription's answer goes out before any of its events, which wait for it.
        let (result, started) = match answered {
            Ok((result, started)) => (Ok(result), started),
            Err(error) => (Err(error), None),
        };
        if outgoing.send(text(&answer(call.id, result))).await.is_err() {
            break;
        }
        if let Some(started) = started {
            tokio::spawn(started);
        }
    }
}

/// What subscribing or letting go answers: its result, and the subscription to start once the answer is sent.
type Subscribing = Result<(serde_json::Value, Option<crate::plugin::Boxed<()>>), Error>;

/// The params of `subscribe`.
#[derive(Deserialize)]
struct Subscribe {
    topic: serde_json::Value,
}

/// The subscriptions a connection holds, each by its id, with what ends it; all of them ended as the connection is.
#[derive(Default)]
struct Subscriptions {
    held: HashMap<String, CancellationToken>,
    made: u64,
    ended: CancellationToken,
}

impl Subscriptions {
    /// A subscription to the topic `params` names, for the client of `access`: its id, and the task that tells it,
    /// held till it is let go.
    fn subscribe(
        &mut self,
        context: &Context,
        access: &Access,
        params: serde_json::Value,
        outgoing: &mpsc::Sender<Outgoing>,
    ) -> Subscribing {
        let subscribe: Subscribe = serde_json::from_value(params).map_err(|error| refusal("bad_params", &error.to_string()))?;
        let Some((scope, telling)) = context
            .siphon
            .registry
            .topics
            .iter()
            .find_map(|(scope, teller)| teller(&subscribe.topic).map(|telling| (*scope, telling)))
        else {
            return Err(refusal("no_such_topic", &format!("there is no topic {}", subscribe.topic)));
        };
        if !access.scopes.contains(&scope) {
            return Err(forbidden(scope));
        }
        self.made += 1;
        let id = format!("s{}", self.made);
        let stop = self.ended.child_token();
        self.held.insert(id.clone(), stop.clone());
        let sink = Sink {
            id: id.clone(),
            outgoing: outgoing.clone(),
        };
        let subscription = Subscription {
            context: context.clone(),
            sink: sink.clone(),
            stop,
        };
        let started: crate::plugin::Boxed<()> = Box::pin(async move {
            if let Err(Lagged(topic)) = telling(subscription).await {
                let lagged = ConnectionEvent::Lagged {
                    subscription: sink.id.clone(),
                    topic,
                };
                let _ = sink.outgoing.send(text(&lagged)).await;
            }
        });
        let result = serde_json::to_value(Subscribed { subscription: id }).unwrap_or_default();
        Ok((result, Some(started)))
    }

    /// Lets go of the subscription `params` names, where it is held.
    fn unsubscribe(&mut self, params: serde_json::Value) -> Subscribing {
        let unsubscribe: Unsubscribe = serde_json::from_value(params).map_err(|error| refusal("bad_params", &error.to_string()))?;
        if let Some(stop) = self.held.remove(&unsubscribe.subscription) {
            stop.cancel();
        }
        Ok((serde_json::json!({}), None))
    }
}

impl Drop for Subscriptions {
    fn drop(&mut self) {
        self.ended.cancel();
    }
}

/// The answer to the call `id`: its result, or why it was refused.
fn answer(id: String, result: Result<serde_json::Value, Error>) -> Answer {
    match result {
        Ok(result) => Answer::Result { id, result },
        Err(error) => Answer::Error { id: Some(id), error },
    }
}

/// An error of `code`, saying `message`.
pub(crate) fn refusal(code: &str, message: &str) -> Error {
    Error {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

/// The refusal of what a session needs `scope` for and lacks.
fn forbidden(scope: Scope) -> Error {
    refusal("forbidden", &format!("the session may not {}", scope_name(scope)))
}

/// The name a scope has on the wire.
fn scope_name(scope: Scope) -> String {
    serde_json::to_value(scope)
        .ok()
        .and_then(|name| name.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// The id of a message that is no call, where it has one to answer by.
fn id_of(message: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(message).ok()?;
    value.get("id")?.as_str().map(str::to_owned)
}

/// A message as the text frame that carries it.
pub(crate) fn text(message: &impl serde::Serialize) -> Outgoing {
    Outgoing::Text(serde_json::to_string(message).unwrap_or_default())
}
