//! What tests of Siphon, of its plugins and of a host build on: Siphon's setup on a volume in memory that asks no
//! model, and a client of a connection — its calls made, its answers and events read, a binary frame's event with the
//! bytes it carries.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use octoii_ai::host::{Credential, Credentials, Unavailable};
use serde_json::{Value, json};
use siphon_protocol::{Machine, Scope};
use siphon_runtime::Options;
use tokio::sync::mpsc;

use crate::connection::{Access, Outgoing, serve};
use crate::{Setup, Siphon};

/// No credentials: a test's threads ask no model.
struct Nothing;

impl Credentials for Nothing {
    fn read(&self, _: &str) -> Result<Option<Credential>, Unavailable> {
        Ok(None)
    }

    fn keep(&self, _: &str, _: &Credential) -> Result<(), Unavailable> {
        Ok(())
    }

    fn forget(&self, _: &str) -> Result<(), Unavailable> {
        Ok(())
    }
}

/// Siphon's setup for a test: the machine as `machine` gives it, a volume in memory, no providers and no models.
#[must_use]
pub fn setup(machine: siphon_machine::Host) -> Setup {
    Setup {
        machine,
        config: None,
        sign_in: crate::providers::SignInSetup::default(),
        credentials: Arc::new(Nothing),
        transport: Arc::new(octoii_ai::transport::Network::default()),
        providers: Vec::new(),
        catalogue: Vec::new(),
        options: Options::default(),
    }
}

/// A test's client of a connection: what it sends, what it is sent, and what it was sent and passed over while it
/// waited for an answer, kept for what it waits for next.
#[derive(Debug)]
pub struct Client {
    pub calls: mpsc::Sender<String>,
    sent: mpsc::Receiver<Outgoing>,
    passed: std::collections::VecDeque<(Value, Bytes)>,
    next: u64,
}

impl Client {
    /// A client of `siphon` whose session holds `scopes`, its queue `ahead` long.
    #[must_use]
    pub fn connect(siphon: &Siphon, scopes: Vec<Scope>, ahead: usize) -> Self {
        let (calls, incoming) = mpsc::channel(16);
        let (outgoing, sent) = mpsc::channel(ahead);
        let access = Access {
            session: "session-1".into(),
            scopes,
        };
        let machine = Machine {
            name: "here".into(),
            workspace: siphon.machine.workspace_of(None).to_string_lossy().into_owned(),
        };
        tokio::spawn(serve(siphon.clone(), access, machine, incoming, outgoing));
        Self {
            calls,
            sent,
            passed: std::collections::VecDeque::new(),
            next: 0,
        }
    }

    /// The next message sent, with the bytes a binary frame carries after its event.
    ///
    /// # Panics
    /// Where none comes within five seconds, or it is not the protocol's.
    pub async fn received(&mut self) -> (Value, Bytes) {
        let sent = tokio::time::timeout(Duration::from_secs(5), self.sent.recv())
            .await
            .expect("a message in time")
            .expect("the connection open");
        match sent {
            Outgoing::Text(text) => (serde_json::from_str(&text).expect("JSON"), Bytes::new()),
            Outgoing::Binary(frame) => {
                let length = u32::from_be_bytes(frame[..4].try_into().expect("a length")) as usize;
                let event = serde_json::from_slice(&frame[4..4 + length]).expect("JSON");
                (event, frame.slice(4 + length..))
            }
        }
    }

    /// The next message sent.
    pub async fn message(&mut self) -> Value {
        self.received().await.0
    }

    /// Calls `method` with `params`: its id.
    ///
    /// # Panics
    /// Where the connection is gone.
    pub async fn call(&mut self, method: &str, params: Value) -> String {
        self.next += 1;
        let id = self.next.to_string();
        let call = json!({ "id": id, "method": method, "params": params });
        self.calls.send(call.to_string()).await.expect("sent");
        id
    }

    /// The answer to the call `id`, what came before it kept for later.
    pub async fn answer(&mut self, id: &str) -> Value {
        loop {
            let (message, payload) = self.received().await;
            if message["id"] == id {
                return message;
            }
            self.passed.push_back((message, payload));
        }
    }

    /// Subscribes to `topic`: the subscription's id.
    ///
    /// # Panics
    /// Where it is refused.
    pub async fn subscribe(&mut self, topic: Value) -> String {
        let id = self.call("subscribe", json!({ "topic": topic })).await;
        let answer = self.answer(&id).await;
        answer["result"]["subscription"].as_str().expect("subscribed").to_owned()
    }

    /// The next event of the subscription `subscription` that `wanted` takes, with the bytes it carries.
    pub async fn event(&mut self, subscription: &str, wanted: impl Fn(&Value) -> bool) -> (Value, Bytes) {
        let taken = |message: &Value| message["subscription"] == subscription && wanted(message);
        if let Some(at) = self.passed.iter().position(|(message, _)| taken(message)) {
            let mut kept = self.passed.split_off(at);
            self.passed = kept.split_off(1);
            if let Some(found) = kept.pop_front() {
                return found;
            }
        }
        self.passed.clear();
        loop {
            let (message, payload) = self.received().await;
            if message["subscription"] == subscription && wanted(&message) {
                return (message, payload);
            }
        }
    }
}
