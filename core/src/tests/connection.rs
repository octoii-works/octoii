//! A client's connection to Octoii on a volume in memory: welcomed first, its calls checked against its session and
//! answered, and its subscriptions telling what their topics hold and how they change — or, where the client does not
//! keep up, that they lagged and where they stood, to be made again from there.

use std::time::Duration;

use crate::protocol::Scope;
use crate::testing::{self, Client};
use crate::{Caller, Extension, Lagged, Octoii, Registrar, Subscription};
use serde_json::{Value, json};

fn siphon() -> Octoii {
    let setup = testing::setup(crate::machine::Host::default());
    Octoii::builder(setup)
        .expect("built")
        .extension(TestBase)
        .extension(Echo)
        .start()
        .expect("started")
}

/// An extension of the test's: a method that says back what it is given, and a topic that tells it thrice.
mod echo {
    use crate::protocol::Scope;
    use serde::{Deserialize, Serialize};
    use ts_rs::TS;

    crate::methods! {
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

impl Extension for Echo {
    fn name(&self) -> &'static str {
        "echo"
    }

    fn title(&self) -> &'static str {
        "Echo"
    }

    fn description(&self) -> &'static str {
        "Says back what it is told."
    }

    fn build(self: Box<Self>, registrar: &mut Registrar<'_>) {
        registrar.methods(echo::set(), echo::Method::read, |_: Caller, method| async move {
            let echo::Method::Say(said) = method;
            Ok(serde_json::to_value(said).unwrap_or_default())
        });
        registrar.topics(
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

/// The welcome a client of the session `session-1` with `scopes` is sent: its session, its scopes, the machine, and its
/// extensions.
fn welcome_of(siphon: &crate::Octoii, scopes: &[&str]) -> serde_json::Value {
    json!({
        "event": "welcome",
        "session": "session-1",
        "scopes": scopes,
        "machine": {
            "name": "here",
            "workspace": siphon.machine.workspace_of(None),
            "home": siphon.machine.home,
            "scratchpads": siphon.machine.scratchpads,
            "memory": siphon.machine.memory,
        },
        "extensions": [
            { "name": "base", "title": "Test base", "description": "Connection test fixture" },
            { "name": "echo", "title": "Echo", "description": "Says back what it is told." }
        ],
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_is_welcomed_its_calls_checked_and_answered() {
    let siphon = siphon();
    let mut reader = Client::connect(&siphon, vec![Scope::ThreadsRead], 64);
    let welcome = reader.message().await;
    assert_eq!(welcome, welcome_of(&siphon, &["threads.read"]));
    reader.controls.send("{ not json".into()).await.expect("sent");
    let refused = reader.message().await;
    assert_eq!(
        (refused["id"].clone(), refused["error"]["code"].clone()),
        (Value::Null, json!("bad_call"))
    );
    assert_eq!(
        reader.command("threads.create", json!({ "config": {}, "key": "k1" })).await["error"]["code"],
        "forbidden",
        "a reader makes no thread"
    );
    assert_eq!(reader.command("threads.list", json!({})).await["result"], json!({ "threads": [] }));

    let mut client = Client::connect(&siphon, ALL.to_vec(), 64);
    client.message().await;
    let listed = client.subscribe(json!({ "type": "threads" })).await;
    let all = client.event(&listed, |event| event["event"] == "threads").await.0;
    assert_eq!(all["threads"], json!([]), "the threads as they stand, first: none yet");
    let thread = client
        .command("threads.create", json!({ "config": { "title": "first" }, "key": "k1" }))
        .await["result"]["thread"]
        .as_str()
        .expect("an id")
        .to_owned();
    let row = client.event(&listed, |event| event["event"] == "thread").await.0;
    assert_eq!(
        (row["thread"]["id"].as_str(), row["thread"]["config"]["title"].as_str()),
        (Some(thread.as_str()), Some("first"))
    );

    let followed = client.subscribe(json!({ "type": "thread", "thread": thread, "limit": 20 })).await;
    let first = client.event(&followed, |_| true).await.0;
    assert_eq!(first["event"], "thread_snapshot");
    assert_eq!(first["history"]["entries"][0]["type"], "config");
    assert_eq!(first["status"]["asks"], json!([]));
    assert_eq!(
        client
            .command("threads.configure", json!({ "thread": thread, "change": { "title": "second" } }))
            .await["result"],
        json!({})
    );
    let changed = client
        .event(&followed, |event| {
            event["event"] == "thread_update" && !event["entries"][0].is_null()
        })
        .await
        .0;
    assert_eq!(changed["entries"][0]["title"], json!({ "from": "first", "to": "second" }));

    // A new watch sends a bounded view, with older history available by query.
    let read = client.command("threads.read", json!({ "thread": thread, "limit": 1 })).await["result"].clone();
    assert_eq!(
        (read["entries"].as_array().map(Vec::len), read["complete"].as_bool()),
        (Some(1), Some(false))
    );
    let latest = client.subscribe(json!({ "type": "thread", "thread": thread, "limit": 1 })).await;
    let latest_view = client.event(&latest, |_| true).await.0;
    assert_eq!(latest_view["event"], "thread_snapshot");
    assert_eq!(latest_view["history"], read);
    client
        .command("threads.configure", json!({ "thread": thread, "change": { "title": "third" } }))
        .await;
    let after = client
        .event(&latest, |event| event["event"] == "thread_update" && !event["entries"][0].is_null())
        .await
        .0;
    assert_eq!(after["entries"].as_array().map(Vec::len), Some(1), "what came after the read alone");
    assert_eq!(after["entries"][0]["title"], json!({ "from": "second", "to": "third" }));

    let settings = client.subscribe(json!({ "type": "settings" })).await;
    let told = client.event(&settings, |event| event["event"] == "settings").await.0;
    assert_eq!(told["settings"]["environment"]["thread_mode"], "default");
    client
        .command("settings.set", json!({ "environment": { "thread_mode": "auto" } }))
        .await;
    let changed = client.event(&settings, |event| event["event"] == "settings").await.0;
    assert_eq!(changed["settings"]["environment"]["thread_mode"], "auto");

    client.command("threads.delete", json!({ "thread": thread })).await;
    let gone = client.event(&listed, |event| event["event"] == "thread_gone").await.0;
    assert_eq!(gone["thread"].as_str(), Some(thread.as_str()));
    siphon.runtime.close().expect("closed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lagged_watch_reseeds_from_a_bounded_view() {
    let siphon = siphon();
    let mut client = Client::connect(&siphon, ALL.to_vec(), 4);
    client.message().await;
    let thread = client.command("threads.create", json!({ "config": {}, "key": "k1" })).await["result"]["thread"]
        .as_str()
        .expect("an id")
        .to_owned();
    let followed = client.subscribe(json!({ "type": "thread", "thread": thread, "limit": 4 })).await;
    // Each change a commit of its own, an event of its own, while the client reads nothing and its queue fills.
    for round in 0..12 {
        client
            .command(
                "threads.configure",
                json!({ "thread": thread, "change": { "title": format!("t{round}") } }),
            )
            .await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let lagged = client.event(&followed, |event| event["event"] == "lagged").await.0;
    assert_eq!(lagged["topic"], json!({ "type": "thread", "thread": thread, "limit": 4 }));
    let again = client.subscribe(lagged["topic"].clone()).await;
    let replacement = client.event(&again, |_| true).await.0;
    assert_eq!(replacement["event"], "thread_snapshot");
    assert_eq!(replacement["history"]["complete"], false);
    let entries = replacement["history"]["entries"].as_array().unwrap();
    let titles: Vec<_> = entries.iter().filter_map(|entry| entry["title"]["to"].as_str()).collect();
    assert_eq!(titles, ["t8", "t9", "t10", "t11"]);
    let older = client
        .command("threads.read", json!({ "thread": thread, "before": entries[0]["id"], "limit": 20 }))
        .await;
    assert_eq!(older["result"]["entries"].as_array().map(Vec::len), Some(9));
    assert_eq!(older["result"]["complete"], true);
    siphon.runtime.close().expect("closed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_extension_s_methods_and_topics_are_the_protocol_s_beside_siphon_s() {
    let siphon = siphon();
    let mut client = Client::connect(&siphon, ALL.to_vec(), 64);
    client.message().await;
    assert_eq!(
        client.command("echo.say", json!({ "words": "hi" })).await["result"],
        json!({ "words": "hi" })
    );
    assert_eq!(
        client.command("echo.say", json!({ "words": 3 })).await["error"]["code"],
        "bad_params"
    );
    assert_eq!(client.command("echo.shout", json!({})).await["error"]["code"], "no_such_method");
    let reply = client
        .watch("subscribe", json!({ "topic": { "type": "echo", "words": "hi" } }))
        .await;
    assert_eq!(reply["error"]["code"], "forbidden", "its topic needs a scope the session lacks");

    let mut operator = Client::connect(&siphon, vec![Scope::ThreadsRead, Scope::TerminalsOperate], 64);
    operator.message().await;
    let subscription = operator.subscribe(json!({ "type": "echo", "words": "hi" })).await;
    for _ in 0..3 {
        let echoed = operator.event(&subscription, |event| event["event"] == "echoed").await.0;
        assert_eq!(echoed["words"], "hi");
    }
    assert_eq!(
        operator.watch("subscribe", json!({ "topic": { "type": "nowhere" } })).await["error"]["code"],
        "no_such_topic"
    );

    let typescript = crate::protocol::typescript(&[crate::protocol::octoii::set(), echo::set()]);
    assert!(typescript.contains("export type Topic = OctoiiTopic | EchoTopic;"));
    assert!(typescript.contains("export type Event = ConnectionEvent | OctoiiEvent | EchoEvent;"));
    assert!(typescript.contains("'echo.say': { params: Said; result: Said };"));
    let said = echo::Method::Say(echo::Said { words: "hi".into() });
    assert_eq!(said.name(), "echo.say", "a method is named as its set declares it");
    siphon.runtime.close().expect("closed");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_extension_switched_off_answers_nothing_till_it_is_switched_on() {
    let siphon = siphon();
    let mut client = Client::connect(
        &siphon,
        vec![Scope::ThreadsRead, Scope::ThreadsOperate, Scope::TerminalsOperate],
        64,
    );
    client.message().await;
    let switch = |off: &[&str]| json!({ "environment": { "disabled_extensions": off } });
    assert_eq!(client.command("settings.set", switch(&["echo"])).await["result"], json!({}));
    assert_eq!(
        client.command("echo.say", json!({ "words": "hi" })).await["error"]["code"],
        "extension_off"
    );
    assert_eq!(
        client
            .watch("subscribe", json!({ "topic": { "type": "echo", "words": "hi" } }))
            .await["error"]["code"],
        "extension_off"
    );
    client.command("settings.set", switch(&[])).await;
    assert_eq!(
        client.command("echo.say", json!({ "words": "hi" })).await["result"],
        json!({ "words": "hi" })
    );
    siphon.runtime.close().expect("closed");
}

#[tokio::test(flavor = "multi_thread")]
async fn what_a_project_was_granted_is_listed_and_taken_back_in_part() {
    let siphon = siphon();
    let granted = crate::tools::permissions::Grants {
        commands: vec!["git push *".into(), "cargo test *".into()],
        paths: vec!["/work/app/docs/**".into()],
        tools: Vec::new(),
    };
    let snapshot = siphon.runtime.snapshot().expect("a snapshot");
    let changes = crate::tools::projects::grant(&snapshot, "/work/app", granted).expect("granted");
    siphon.runtime.commit(changes).await.expect("committed");
    let mut client = Client::connect(&siphon, ALL.to_vec(), 64);
    client.message().await;
    let listed = client.command("grants.list", json!({})).await["result"]["projects"].clone();
    assert_eq!(listed[0]["folder"], "/work/app");
    assert_eq!(listed[0]["grants"]["commands"], json!(["git push *", "cargo test *"]));
    let less = json!({ "folder": "/work/app", "grants": { "commands": ["git push *"], "paths": [], "tools": [] } });
    client.command("grants.revoke", less).await;
    let listed = client.command("grants.list", json!({})).await["result"]["projects"].clone();
    assert_eq!(listed[0]["grants"]["commands"], json!(["cargo test *"]), "taken back in part");
    siphon.runtime.close().expect("closed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_goes_on_at_a_time_as_its_status_tells_and_its_window_is_told_part_by_part() {
    let siphon = siphon();
    let mut client = Client::connect(&siphon, ALL.to_vec(), 64);
    client.message().await;
    let thread = client.command("threads.create", json!({ "config": {}, "key": "k1" })).await["result"]["thread"]
        .as_str()
        .expect("an id")
        .to_owned();
    let mut list_client = Client::connect(&siphon, ALL.to_vec(), 64);
    list_client.message().await;
    let list = list_client.subscribe(json!({ "type": "threads" })).await;
    list_client.event(&list, |event| event["event"] == "threads").await;
    let followed = client.subscribe(json!({ "type": "thread", "thread": thread, "limit": 20 })).await;
    let first = client.event(&followed, |event| event["event"] == "thread_snapshot").await.0;
    assert_eq!(
        (first["status"]["compacting"].as_bool(), first["status"].get("resuming_at")),
        (Some(false), None)
    );

    let at = 4_102_444_800_000_i64;
    assert_eq!(
        client.command("threads.resume", json!({ "thread": thread, "at": at })).await["result"],
        json!({})
    );
    client
        .event(&followed, |event| {
            event["event"] == "thread_update" && event["status"]["resuming_at"] == at
        })
        .await;
    list_client
        .event(&list, |event| {
            event["event"] == "thread" && event["thread"]["id"] == thread && event["thread"]["status"]["resuming_at"] == at
        })
        .await;
    let later = at + 60_000;
    assert_eq!(
        client.command("threads.resume", json!({ "thread": thread, "at": later })).await["result"],
        json!({})
    );
    client
        .event(&followed, |event| {
            event["event"] == "thread_update" && event["status"]["resuming_at"] == later
        })
        .await;

    client.command("threads.resume", json!({ "thread": thread })).await;
    client
        .event(&followed, |event| {
            event["event"] == "thread_update" && event["status"].get("resuming_at").is_none()
        })
        .await;

    client
        .command(
            "threads.deliver",
            json!({ "thread": thread, "content": [{ "type": "input_text", "text": "hello" }], "key": "d1" }),
        )
        .await;
    client
        .event(&followed, |event| {
            event["event"] == "thread_update"
                && event["entries"]
                    .as_array()
                    .is_some_and(|entries| entries.iter().any(|entry| entry["type"] == "input"))
        })
        .await;
    let composition = client.command("threads.context", json!({ "thread": thread })).await["result"].clone();
    let parts: Vec<&str> = composition["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|share| share["part"].as_str())
        .collect();
    assert!(parts.contains(&"messages"), "{composition}");
    assert!(composition["tokens"].as_u64().is_some_and(|tokens| tokens > 0), "{composition}");
    siphon.runtime.close().expect("closed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_resume_survives_a_rejected_replacement() {
    let siphon = siphon();
    let mut client = Client::connect(&siphon, ALL.to_vec(), 64);
    client.message().await;
    let thread = client.command("threads.create", json!({ "config": {}, "key": "k1" })).await["result"]["thread"]
        .as_str()
        .expect("an id")
        .to_owned();
    let at = 4_102_444_800_000_i64;
    assert_eq!(
        client.command("threads.resume", json!({ "thread": thread, "at": at })).await["result"],
        json!({})
    );

    let path = octoii_siphon_lib::fiber_path(&crate::threads::thread::main(&thread), crate::agent::ResumeFiber::KEY.as_bytes());
    let resume = || siphon.runtime.snapshot().unwrap().fiber(&path).unwrap().expect("the armed resume");
    let original = resume();
    let mut changes = octoii_siphon_lib::Changes::new().unwrap();
    changes
        .meta(
            crate::threads::thread::path(&thread),
            [],
            [(crate::threads::window::FROM_ANNOTATION.into(), Some("{".into()))],
        )
        .unwrap();
    siphon.runtime.commit(changes).await.unwrap();

    assert_eq!(
        client
            .command("threads.resume", json!({ "thread": thread, "at": at + 60_000 }))
            .await["error"]["code"],
        "failed"
    );
    assert_eq!(resume().id, original.id);
    siphon.runtime.close().expect("closed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_keeps_its_starting_workspace_when_reconfigured() {
    let siphon = siphon();
    let mut client = Client::connect(&siphon, ALL.to_vec(), 64);
    client.message().await;
    let workspace = siphon.machine.workspace_of(None);
    let created = client
        .command(
            "threads.create",
            json!({ "config": { "workspace": workspace }, "key": "fixed-workspace" }),
        )
        .await;
    let thread = created["result"]["thread"].as_str().expect("a thread");
    assert_eq!(
        client
            .command(
                "threads.configure",
                json!({ "thread": thread, "change": { "workspace": "/other" } }),
            )
            .await["error"]["code"],
        "workspace_fixed"
    );
    let listed = siphon.threads.listed(thread).expect("listed");
    assert_eq!(listed.config.workspace.as_deref(), workspace.to_str());
    assert_eq!(listed.status.inbox, []);
    siphon.runtime.close().expect("closed");
}

#[tokio::test(flavor = "multi_thread")]
async fn binding_a_default_workspace_does_not_select_a_project() {
    use crate::agent::{Shared, Teller, Telling};
    use crate::threads::{InputPart, NewThread, StartDirectory, StartWorkspace};
    use crate::tools::Place;

    struct ProjectContext;
    impl Teller for ProjectContext {
        fn tell<'a>(
            &'a self,
            _: &'a crate::volume::Volume,
            _: &'a str,
            place: Option<&'a Place>,
        ) -> futures::future::BoxFuture<'a, anyhow::Result<Telling>> {
            Box::pin(async move {
                let sections = place.into_iter().map(|_| ("project".into(), "configured".into())).collect();
                Ok(Telling {
                    sections,
                    tools: Vec::new(),
                })
            })
        }
    }
    impl Extension for ProjectContext {
        fn name(&self) -> &'static str {
            "project-context"
        }
        fn title(&self) -> &'static str {
            "Project context"
        }
        fn description(&self) -> &'static str {
            "Context only for a selected project"
        }
        fn build(self: Box<Self>, registrar: &mut Registrar<'_>) {
            registrar.teller(Shared::Project, std::sync::Arc::new(*self));
        }
    }

    let host = crate::machine::Host::default();
    let workspace = host.workspace_of(None).to_string_lossy().into_owned();
    let siphon = Octoii::builder(testing::setup(host))
        .unwrap()
        .extension(TestBase)
        .extension(ProjectContext)
        .start()
        .unwrap();
    let new = NewThread {
        actor: "person".into(),
        ..NewThread::default()
    };
    let volume = siphon.volume();
    for (key, project) in [("default-workspace", None), ("with-project", Some(workspace.clone()))] {
        let expected: Vec<_> = project.iter().map(|_| ("project".to_owned(), "configured".to_owned())).collect();
        let binding = StartWorkspace {
            workspace: workspace.clone(),
            project,
        };
        let id = volume
            .start_thread(
                "person",
                key,
                &new,
                vec![InputPart::from(octoii_ai::vocabulary::InputContent::text("hello"))],
                StartDirectory::Current(binding),
            )
            .await
            .unwrap();
        assert_eq!(
            siphon.threads.listed(&id).unwrap().config.workspace.as_deref(),
            Some(workspace.as_str())
        );
        assert_eq!(siphon.told(&id).await.unwrap().sections, expected);
    }
    siphon.close().await;
}

struct TestBase;
impl crate::Extension for TestBase {
    fn name(&self) -> &'static str {
        "base"
    }
    fn title(&self) -> &'static str {
        "Test base"
    }
    fn description(&self) -> &'static str {
        "Connection test fixture"
    }
    fn build(self: Box<Self>, registrar: &mut crate::Registrar<'_>) {
        registrar.prompts(crate::testing::PROMPTS);
    }
}
