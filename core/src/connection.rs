//! Dispatches authenticated commands and queries, and serves subscriptions over the watch connection. Each
//! subscription can fall behind independently and tell the client how to subscribe again.

mod calls;
mod journal;
mod machine;
mod providers;
mod subscriptions;

use std::collections::HashMap;

use crate::protocol::{Answer, Command, ConnectionEvent, Error, Machine, Scope, Subscribed, Unsubscribe};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub(crate) use calls::answered;
pub(crate) use subscriptions::told;

use crate::Octoii;
use crate::extension::{Caller, Lagged, Sink, Subscription};

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

/// Dispatches an authenticated HTTP command or query through the product's method registry.
///
/// # Errors
/// Refuses unknown methods, disabled extensions, insufficient scopes, and errors from the handler.
pub async fn call(octoii: Octoii, access: Access, command: Command) -> Result<serde_json::Value, Error> {
    let caller = Caller {
        siphon: octoii,
        session: access.session,
        scopes: access.scopes,
    };
    let Command { method, params } = command;
    let owned = caller
        .siphon
        .registry
        .methods
        .get(method.as_str())
        .ok_or_else(|| refusal("no_such_method", &format!("there is no method {method}")))?;
    if let Some(scope) = owned.item.0 {
        caller.require_scope(scope)?;
    }
    if let Some(off) = switched_off(&caller.siphon, &owned.extension) {
        return Err(off);
    }
    (owned.item.1)(caller.clone(), &method, params).await
}

/// Serves a client of `access`, which reached `machine`: each message of `incoming` taken as a call, what is sent it
/// put on `outgoing` — the queue its carrier writes from, `OUTGOING_AHEAD` long — till `incoming` ends, when every
/// subscription of its ends with it.
pub async fn serve(
    siphon: Octoii,
    access: Access,
    machine: Machine,
    mut incoming: mpsc::Receiver<String>,
    outgoing: mpsc::Sender<Outgoing>,
) {
    let welcome = ConnectionEvent::Welcome {
        session: access.session.clone(),
        scopes: access.scopes.clone(),
        machine,
        extensions: siphon.registry.extensions.clone(),
    };
    if outgoing.send(text(&welcome)).await.is_err() {
        return;
    }
    let caller = Caller {
        siphon,
        session: access.session,
        scopes: access.scopes,
    };
    let mut subscriptions = Subscriptions::default();
    while let Some(message) = incoming.recv().await {
        let call = match serde_json::from_str::<WatchCall>(&message) {
            Ok(call) => call,
            Err(error) => {
                let refused = Answer::Error {
                    id: id_of(&message),
                    error: refusal("bad_call", &error.to_string()),
                };
                if outgoing.send(text(&refused)).await.is_err() {
                    break;
                }
                continue;
            }
        };
        let answered = match &call.method {
            WatchMethod::Subscribe { topic } => subscriptions.subscribe(&caller, topic, &outgoing),
            WatchMethod::Unsubscribe(unsubscribe) => {
                subscriptions.unsubscribe(&unsubscribe.subscription);
                Ok((serde_json::json!({}), None))
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
type Subscribing = Result<(serde_json::Value, Option<crate::extension::Boxed<()>>), Error>;

#[derive(Deserialize)]
struct WatchCall {
    id: String,
    #[serde(flatten)]
    method: WatchMethod,
}

#[derive(Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
enum WatchMethod {
    Subscribe { topic: serde_json::Value },
    Unsubscribe(Unsubscribe),
}

/// The subscriptions a connection holds, each by its id, with what ends it; all of them ended as the connection is.
#[derive(Default)]
struct Subscriptions {
    held: HashMap<String, CancellationToken>,
    made: u64,
    ended: CancellationToken,
}

impl Subscriptions {
    fn subscribe(&mut self, caller: &Caller, topic: &serde_json::Value, outgoing: &mpsc::Sender<Outgoing>) -> Subscribing {
        let Some((scope, telling, extension)) = caller
            .siphon
            .registry
            .topics
            .iter()
            .find_map(|owned| (owned.item.1)(topic).map(|telling| (owned.item.0, telling, &owned.extension)))
        else {
            return Err(refusal("no_such_topic", &format!("there is no topic {topic}")));
        };
        caller.require_scope(scope)?;
        if let Some(off) = switched_off(&caller.siphon, extension) {
            return Err(off);
        }
        self.held.retain(|_, stop| !stop.is_cancelled());
        self.made += 1;
        let id = format!("s{}", self.made);
        let stop = self.ended.child_token();
        self.held.insert(id.clone(), stop.clone());
        let sink = Sink {
            id: id.clone(),
            outgoing: outgoing.clone(),
        };
        let subscription = Subscription {
            caller: caller.clone(),
            sink: sink.clone(),
            stop: stop.clone(),
        };
        let started: crate::extension::Boxed<()> = Box::pin(async move {
            if let Err(Lagged(topic)) = telling(subscription).await {
                let lagged = ConnectionEvent::Lagged {
                    subscription: sink.id.clone(),
                    topic,
                };
                tokio::select! {
                    () = stop.cancelled() => {},
                    _ = sink.outgoing.send(text(&lagged)) => {},
                }
            }
            stop.cancel();
        });
        let result = serde_json::to_value(Subscribed { subscription: id }).unwrap_or_default();
        Ok((result, Some(started)))
    }

    fn unsubscribe(&mut self, id: &str) {
        if let Some(stop) = self.held.remove(id) {
            stop.cancel();
        }
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
/// The refusal of what the extension `extension` brought, where the person switched it off; none where it is on.
fn switched_off(siphon: &Octoii, extension: &str) -> Option<Error> {
    let on = siphon
        .runtime
        .snapshot()
        .is_ok_and(|snapshot| crate::settings::switched_on(&snapshot, extension));
    (!on).then(|| refusal("extension_off", &format!("the extension {extension} is switched off")))
}

#[must_use]
pub fn refusal(code: &str, message: &str) -> Error {
    Error {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

/// The refusal of what a session needs `scope` for and lacks.
pub(crate) fn forbidden(scope: Scope) -> Error {
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
