//! A client's connection to Siphon on a volume in memory: welcomed first, its calls checked against its session and
//! answered, and its subscriptions telling what their topics hold and how they change — or, where the client does not
//! keep up, that they lagged and where they stood, to be made again from there.

use std::sync::Arc;
use std::time::Duration;

use beak::Beak;
use bytes::Bytes;
use octoii_ai::host::{Credential, Credentials, Files, NoSuchFile, Unavailable};
use octoii_ai::models::{Host, Models};
use serde_json::{Value, json};
use siphon::connection::{Access, Outgoing, serve};
use siphon::{BrowserSetup, Setup, Siphon};
use siphon_protocol::{Machine, Scope};
use siphon_runtime::Options;
use tokio::sync::mpsc;

/// No credentials and no files: the threads here ask no model.
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

impl Files for Nothing {
    fn read(&self, reference: &str) -> Result<Bytes, NoSuchFile> {
        Err(NoSuchFile(reference.into()))
    }
}

fn siphon(beak: &Beak) -> Siphon {
    let host = Host {
        credentials: Arc::new(Nothing),
        files: Arc::new(Nothing),
        transport: Arc::new(octoii_ai::transport::Network::default()),
        new_id: Arc::new(|| "id".to_owned()),
        clock: Arc::new(|| 0),
    };
    let setup = Setup {
        machine: siphon_machine::Host::default(),
        models: Arc::new(Models::new(host, Vec::new(), Vec::new())),
        browser: BrowserSetup::default(),
        options: Options::default(),
    };
    Siphon::start(beak, setup).expect("started")
}

/// A client: what it sends, and what it is sent.
struct Client {
    calls: mpsc::Sender<String>,
    sent: mpsc::Receiver<Outgoing>,
    next: u64,
}

impl Client {
    fn connect(siphon: &Siphon, scopes: Vec<Scope>, ahead: usize) -> Self {
        let (calls, incoming) = mpsc::channel(16);
        let (outgoing, sent) = mpsc::channel(ahead);
        let access = Access {
            session: "session-1".into(),
            scopes,
        };
        let machine = Machine { name: "here".into() };
        tokio::spawn(serve(siphon.clone(), access, machine, incoming, outgoing));
        Self { calls, sent, next: 0 }
    }

    async fn message(&mut self) -> Value {
        let sent = tokio::time::timeout(Duration::from_secs(5), self.sent.recv())
            .await
            .expect("in time")
            .expect("open");
        let Outgoing::Text(text) = sent;
        serde_json::from_str(&text).expect("JSON")
    }

    /// Calls `method` with `params`: its id.
    async fn call(&mut self, method: &str, params: Value) -> String {
        self.next += 1;
        let id = self.next.to_string();
        let call = json!({ "id": id, "method": method, "params": params });
        self.calls.send(call.to_string()).await.expect("sent");
        id
    }

    /// The answer to the call `id`, the events before it passed over.
    async fn answer(&mut self, id: &str) -> Value {
        loop {
            let message = self.message().await;
            if message["id"] == id {
                return message;
            }
        }
    }

    /// The next event of the subscription `subscription` that `wanted` takes.
    async fn event(&mut self, subscription: &str, wanted: impl Fn(&Value) -> bool) -> Value {
        loop {
            let message = self.message().await;
            if message["subscription"] == subscription && wanted(&message) {
                return message;
            }
        }
    }
}

const ALL: [Scope; 2] = [Scope::ThreadsRead, Scope::ThreadsOperate];

#[tokio::test(flavor = "multi_thread")]
async fn a_client_is_welcomed_its_calls_checked_and_answered() {
    let beak = Beak::new().expect("Beak");
    let siphon = siphon(&beak);
    let mut reader = Client::connect(&siphon, vec![Scope::ThreadsRead], 64);
    let welcome = reader.message().await;
    assert_eq!(
        welcome,
        json!({ "event": "welcome", "session": "session-1", "scopes": ["threads.read"], "machine": { "name": "here" } })
    );
    reader.calls.send("{ not json".into()).await.expect("sent");
    let refused = reader.message().await;
    assert_eq!(
        (refused["id"].clone(), refused["error"]["code"].clone()),
        (Value::Null, json!("bad_call"))
    );
    let id = reader.call("threads.create", json!({ "config": {}, "key": "k1" })).await;
    assert_eq!(reader.answer(&id).await["error"]["code"], "forbidden", "a reader makes no thread");
    let id = reader.call("threads.list", json!({})).await;
    assert_eq!(reader.answer(&id).await["result"], json!({ "threads": [] }));

    let mut client = Client::connect(&siphon, ALL.to_vec(), 64);
    client.message().await;
    let id = client.call("subscribe", json!({ "topic": { "type": "threads" } })).await;
    let listed = client.answer(&id).await["result"]["subscription"]
        .as_str()
        .expect("an id")
        .to_owned();
    let id = client
        .call("threads.create", json!({ "config": { "title": "first" }, "key": "k1" }))
        .await;
    let thread = client.answer(&id).await["result"]["thread"].as_str().expect("an id").to_owned();
    let row = client.event(&listed, |event| event["event"] == "thread").await;
    assert_eq!(
        (row["thread"]["id"].as_str(), row["thread"]["config"]["title"].as_str()),
        (Some(thread.as_str()), Some("first"))
    );

    let id = client
        .call("subscribe", json!({ "topic": { "type": "thread", "thread": thread } }))
        .await;
    let followed = client.answer(&id).await["result"]["subscription"]
        .as_str()
        .expect("an id")
        .to_owned();
    let entries = client.event(&followed, |event| event["event"] == "entries").await;
    assert_eq!(entries["entries"][0]["type"], "config", "the log begins with how it runs");
    client.event(&followed, |event| event["event"] == "status").await;
    let id = client
        .call("threads.configure", json!({ "thread": thread, "change": { "title": "second" } }))
        .await;
    assert_eq!(client.answer(&id).await["result"], json!({}));
    let changed = client.event(&followed, |event| event["event"] == "entries").await;
    assert_eq!(changed["entries"][0]["title"], json!({ "from": "first", "to": "second" }));

    let id = client.call("subscribe", json!({ "topic": { "type": "settings" } })).await;
    let settings = client.answer(&id).await["result"]["subscription"]
        .as_str()
        .expect("an id")
        .to_owned();
    let told = client.event(&settings, |event| event["event"] == "settings").await;
    assert_eq!(told["settings"]["environment"]["thread_mode"], "default");
    siphon.runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_subscription_its_client_does_not_keep_up_with_lags_and_is_made_again_from_where_it_stood() {
    let beak = Beak::new().expect("Beak");
    let siphon = siphon(&beak);
    let mut client = Client::connect(&siphon, ALL.to_vec(), 4);
    client.message().await;
    let id = client.call("threads.create", json!({ "config": {}, "key": "k1" })).await;
    let thread = client.answer(&id).await["result"]["thread"].as_str().expect("an id").to_owned();
    let id = client
        .call("subscribe", json!({ "topic": { "type": "thread", "thread": thread } }))
        .await;
    let followed = client.answer(&id).await["result"]["subscription"]
        .as_str()
        .expect("an id")
        .to_owned();
    // Each change a commit of its own, an event of its own, while the client reads nothing and its queue fills.
    for round in 0..12 {
        client
            .call(
                "threads.configure",
                json!({ "thread": thread, "change": { "title": format!("t{round}") } }),
            )
            .await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut seen: Vec<Value> = Vec::new();
    let lagged = loop {
        let event = client.event(&followed, |_| true).await;
        match event["event"].as_str() {
            Some("lagged") => break event,
            Some("entries") => seen.extend(event["entries"].as_array().expect("entries").iter().cloned()),
            _ => {}
        }
    };
    assert!(seen.len() < 13, "it lagged before telling all");
    let id = client
        .call(
            "subscribe",
            json!({ "topic": { "type": "thread", "thread": thread, "from": lagged["cursor"] } }),
        )
        .await;
    let again = client.answer(&id).await["result"]["subscription"]
        .as_str()
        .expect("an id")
        .to_owned();
    while seen.len() < 13 {
        let event = client.event(&again, |event| event["event"] == "entries").await;
        seen.extend(event["entries"].as_array().expect("entries").iter().cloned());
    }
    let titles: Vec<&str> = seen.iter().filter_map(|entry| entry["title"]["to"].as_str()).collect();
    let wanted: Vec<String> = (0..12).map(|round| format!("t{round}")).collect();
    assert_eq!(titles, wanted, "each entry once, in order, across the lag");
    siphon.runtime.close().await;
}
