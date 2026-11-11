//! A client's connection, as Siphon serves it whatever carries it: the welcome first; then each call the client makes,
//! read, checked against what its session may do, and answered in a task of its own, so a slow call holds no other
//! back; and its subscriptions, each a task of its own that sends what its topic tells as it changes. What goes out
//! waits in one bounded queue: an answer waits for room, a subscription's event does not — a subscription the client
//! does not keep up with is let go, told it lagged and where it stood, for the client to make again from there.

mod calls;
mod subscriptions;

use std::collections::HashMap;

use siphon_protocol::{Answer, Call, Error, Event, Machine, Method, Scope, Subscribed};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::Siphon;

/// How many messages wait to go out to a client before a subscription's next event lets the subscription go.
pub const OUTGOING_AHEAD: usize = 256;

/// Who a connection's client is: its session, by its id, and what the session lets it do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    pub session: String,
    pub scopes: Vec<Scope>,
}

/// A message to send a client: a text frame of the protocol's JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outgoing {
    Text(String),
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
    let welcome = Event::Welcome {
        session: access.session.clone(),
        scopes: access.scopes.clone(),
        machine,
    };
    if outgoing.send(text(&welcome)).await.is_err() {
        return;
    }
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
        let needed = match &call.method {
            Method::Subscribe(subscribe) => Some(subscriptions::scope_of(&subscribe.topic)),
            method => method.scope(),
        };
        if let Some(scope) = needed.filter(|scope| !access.scopes.contains(scope)) {
            let error = refusal("forbidden", &format!("the session may not {}", scope_name(scope)));
            let _ = outgoing.send(text(&Answer::Error { id: Some(call.id), error })).await;
            continue;
        }
        match call.method {
            Method::Subscribe(subscribe) => {
                let id = subscriptions.made_id();
                let result = serde_json::to_value(Subscribed { subscription: id.clone() }).unwrap_or_default();
                // Its answer goes out before any of its events.
                if outgoing.send(text(&answer(call.id, Ok(result)))).await.is_err() {
                    break;
                }
                let task = subscriptions::Task {
                    siphon: siphon.clone(),
                    session: access.session.clone(),
                    id: id.clone(),
                    outgoing: outgoing.clone(),
                    stop: subscriptions.hold(id),
                };
                tokio::spawn(task.run(subscribe.topic));
            }
            Method::Unsubscribe(unsubscribe) => {
                subscriptions.let_go(&unsubscribe.subscription);
                let _ = outgoing.send(text(&answer(call.id, Ok(serde_json::json!({}))))).await;
            }
            method => {
                let (siphon, session, outgoing) = (siphon.clone(), access.session.clone(), outgoing.clone());
                tokio::spawn(async move {
                    let result = calls::answered(&siphon, &session, method).await;
                    let _ = outgoing.send(text(&answer(call.id, result))).await;
                });
            }
        }
    }
}

/// The subscriptions a connection holds, each by its id, with what ends it; all of them ended as the connection is.
#[derive(Default)]
struct Subscriptions {
    held: HashMap<String, CancellationToken>,
    made: u64,
    ended: CancellationToken,
}

impl Subscriptions {
    /// The id of a subscription to be made, the connection's next.
    fn made_id(&mut self) -> String {
        self.made += 1;
        format!("s{}", self.made)
    }

    /// Holds the subscription `id`: what ends it.
    fn hold(&mut self, id: String) -> CancellationToken {
        let stop = self.ended.child_token();
        self.held.insert(id, stop.clone());
        stop
    }

    /// Ends the subscription `id`, where it is held.
    fn let_go(&mut self, id: &str) {
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
pub(crate) fn refusal(code: &str, message: &str) -> Error {
    Error {
        code: code.to_owned(),
        message: message.to_owned(),
    }
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
