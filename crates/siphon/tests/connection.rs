//! A client's connection to Siphon on a volume in memory: welcomed first, its calls checked against its session and
//! answered, and its subscriptions telling what their topics hold and how they change — or, where the client does not
//! keep up, that they lagged and where they stood, to be made again from there.

use std::time::Duration;

use beak::Beak;
use serde_json::{Value, json};
use siphon::testing::{self, Client};
use siphon::{Builder, Context, Lagged, Plugin, Siphon, Subscription};
use siphon_protocol::Scope;

fn siphon(beak: &Beak) -> Siphon {
    let setup = testing::setup(siphon_machine::Host::default());
    Siphon::builder(beak, setup).expect("built").plugin(Echo).start().expect("started")
}

/// A plugin of the test's: a method that says back what it is given, and a topic that tells it thrice.
mod echo {
    use serde::{Deserialize, Serialize};
    use siphon_protocol::Scope;
    use ts_rs::TS;

    siphon_protocol::methods! {
        events: EchoEvent, topics: EchoTopic;
        /// What it is given, said back.
        "echo.say" Say(Said) -> Said, Some(Scope::ThreadsRead);
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
    pub struct Said {
        pub words: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum EchoTopic {
        Echo { words: String },
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(tag = "event", rename_all = "snake_case")]
    pub enum EchoEvent {
        Echoed { subscription: String, words: String },
    }
}

struct Echo;

impl Plugin for Echo {
    fn name(&self) -> &'static str {
        "echo"
    }

    fn build(self: Box<Self>, builder: &mut Builder) {
        builder.methods(echo::set(), echo::Method::read, |_: Context, method| async move {
            let echo::Method::Say(said) = method;
            Ok(serde_json::to_value(said).unwrap_or_default())
        });
        builder.topics(
            Scope::TerminalsOperate,
            |subscription: Subscription, topic: echo::EchoTopic| async move {
                let echo::EchoTopic::Echo { words } = topic;
                for _ in 0..3 {
                    let echoed = echo::EchoEvent::Echoed {
                        subscription: subscription.sink.id().to_owned(),
                        words: words.clone(),
                    };
                    subscription
                        .sink
                        .offer(&echoed)
                        .map_err(|_| Lagged(json!({ "type": "echo", "words": words })))?;
                }
                Ok(())
            },
        );
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
    let row = client.event(&listed, |event| event["event"] == "thread").await.0;
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
    let entries = client.event(&followed, |event| event["event"] == "entries").await.0;
    assert_eq!(entries["entries"][0]["type"], "config", "the log begins with how it runs");
    client.event(&followed, |event| event["event"] == "status").await;
    let id = client
        .call("threads.configure", json!({ "thread": thread, "change": { "title": "second" } }))
        .await;
    assert_eq!(client.answer(&id).await["result"], json!({}));
    let changed = client.event(&followed, |event| event["event"] == "entries").await.0;
    assert_eq!(changed["entries"][0]["title"], json!({ "from": "first", "to": "second" }));

    let id = client.call("subscribe", json!({ "topic": { "type": "settings" } })).await;
    let settings = client.answer(&id).await["result"]["subscription"]
        .as_str()
        .expect("an id")
        .to_owned();
    let told = client.event(&settings, |event| event["event"] == "settings").await.0;
    assert_eq!(told["settings"]["environment"]["thread_mode"], "default");
    let id = client
        .call("settings.set", json!({ "environment": { "thread_mode": "auto" } }))
        .await;
    client.answer(&id).await;
    let changed = client.event(&settings, |event| event["event"] == "settings").await.0;
    assert_eq!(changed["settings"]["environment"]["thread_mode"], "auto");

    let id = client.call("threads.delete", json!({ "thread": thread })).await;
    client.answer(&id).await;
    let gone = client.event(&listed, |event| event["event"] == "thread_gone").await.0;
    assert_eq!(gone["thread"].as_str(), Some(thread.as_str()));
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
        let event = client.event(&followed, |_| true).await.0;
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
        let event = client.event(&again, |event| event["event"] == "entries").await.0;
        seen.extend(event["entries"].as_array().expect("entries").iter().cloned());
    }
    let titles: Vec<&str> = seen.iter().filter_map(|entry| entry["title"]["to"].as_str()).collect();
    let wanted: Vec<String> = (0..12).map(|round| format!("t{round}")).collect();
    assert_eq!(titles, wanted, "each entry once, in order, across the lag");
    siphon.runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plugin_s_methods_and_topics_are_the_protocol_s_beside_siphon_s() {
    let beak = Beak::new().expect("Beak");
    let siphon = siphon(&beak);
    let mut client = Client::connect(&siphon, ALL.to_vec(), 64);
    client.message().await;
    let id = client.call("echo.say", json!({ "words": "hi" })).await;
    assert_eq!(client.answer(&id).await["result"], json!({ "words": "hi" }));
    let id = client.call("echo.say", json!({ "words": 3 })).await;
    assert_eq!(client.answer(&id).await["error"]["code"], "bad_params");
    let id = client.call("echo.shout", json!({})).await;
    assert_eq!(client.answer(&id).await["error"]["code"], "no_such_method");
    let id = client
        .call("subscribe", json!({ "topic": { "type": "echo", "words": "hi" } }))
        .await;
    assert_eq!(
        client.answer(&id).await["error"]["code"],
        "forbidden",
        "its topic needs a scope the session lacks"
    );

    let mut operator = Client::connect(&siphon, vec![Scope::ThreadsRead, Scope::TerminalsOperate], 64);
    operator.message().await;
    let id = operator
        .call("subscribe", json!({ "topic": { "type": "echo", "words": "hi" } }))
        .await;
    let subscription = operator.answer(&id).await["result"]["subscription"]
        .as_str()
        .expect("an id")
        .to_owned();
    for _ in 0..3 {
        let echoed = operator.event(&subscription, |event| event["event"] == "echoed").await.0;
        assert_eq!(echoed["words"], "hi");
    }
    let id = operator.call("subscribe", json!({ "topic": { "type": "nowhere" } })).await;
    assert_eq!(operator.answer(&id).await["error"]["code"], "no_such_topic");

    let typescript = siphon_protocol::typescript(&[siphon_protocol::siphon::set(), echo::set()]);
    assert!(typescript.contains("export type Topic = SiphonTopic | EchoTopic;"));
    assert!(typescript.contains("export type Event = ConnectionEvent | SiphonEvent | EchoEvent;"));
    assert!(typescript.contains("'echo.say': { params: Said; result: Said };"));
    siphon.runtime.close().await;
}
