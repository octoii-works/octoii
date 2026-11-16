//! The `thread` tool as a turn runs it: a thread it starts is one of the person's, listed with the others the most
//! recently active first — its id, how it stands, when it was last active, its project and its title — the thread that
//! asks marked, and a page past the last told so; what was said in it is found by keywords, every one or any, and its
//! conversation read back, about an entry found too. Arguments that do not fit are told by the field of the action they
//! name.

use serde_json::json;
use siphon::testing::{Beak, Called, Calling};
use siphon::tools::{CallStatus, Tool};
use siphon_thread::{ConversationContent, ThreadResult, ThreadStatus, ThreadTool};

/// Successful calls carry business data fitting the schema that the tool declares.
fn result(call: &Called) -> ThreadResult {
    assert_eq!(call.status, CallStatus::Ok, "{}", call.text);
    let structured = call.structured_content.as_ref().expect("a structured thread result");
    jsonschema::validate(&ThreadTool::output_schema().expect("the result schema"), structured).expect("a result fitting its schema");
    serde_json::from_value(structured.clone()).expect("a typed thread result")
}

/// Calls of the `thread` tool, by a turn of `t1` working in a fresh folder; the folder held.
struct Turn {
    calling: Calling,
    workspace: std::path::PathBuf,
    _folder: tempfile::TempDir,
}

impl Turn {
    fn start(beak: &Beak) -> Self {
        let folder = tempfile::tempdir().expect("a folder");
        Self {
            calling: Calling::start(beak, |tools| tools.with(ThreadTool)),
            workspace: folder.path().canonicalize().expect("its real path"),
            _folder: folder,
        }
    }

    /// What the `thread` calls of `arguments` came to, made under `key`.
    async fn called(&self, key: &str, arguments: Vec<serde_json::Value>) -> Vec<Called> {
        let calls = arguments
            .iter()
            .enumerate()
            .map(|(at, arguments)| (format!("{key}-{at}"), "thread".to_owned(), arguments.to_string()))
            .collect();
        self.calling
            .called(key, self.workspace.clone(), vec!["thread".to_owned()], calls)
            .await
    }

    /// The id of a thread started titled "Greeter".
    async fn greeter(&self) -> String {
        let started = self
            .called("start", vec![json!({ "action": "start", "message": "say hi", "title": "Greeter" })])
            .await;
        let ThreadResult::Start { thread, delivery, report } = result(&started[0]) else {
            panic!("a start result");
        };
        assert_ne!(delivery, "");
        assert!(!report);
        thread
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_started_is_listed_with_the_others_the_most_recently_active_first() {
    let beak = Beak::new().expect("Beak");
    let turn = Turn::start(&beak);
    let greeter = turn.greeter().await;
    let listed = turn.called("k1", vec![json!({ "action": "list" })]).await;
    let text = &listed[0].text;
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    assert!(lines[0].contains("  Greeter"), "the newest first: {text}");
    assert!(lines[1].starts_with("t1  new  ") && lines[1].ends_with("  (this thread)"), "{text}");
    assert_eq!(lines[2], "<system-reminder>2 threads</system-reminder>");
    let ThreadResult::List {
        threads,
        offset,
        total,
        next,
    } = result(&listed[0])
    else {
        panic!("a list result");
    };
    assert_eq!((offset, total, next), (0, 2, None));
    assert_eq!(threads[0].id, greeter);
    assert_eq!(threads[0].title.as_deref(), Some("Greeter"));
    assert_eq!(threads[1].id, "t1");
    assert_eq!(threads[1].status, ThreadStatus::New);
    assert!(threads[1].current);
    assert_eq!(threads[1].project, None);
    assert_eq!(threads[1].title, None);
    let past = turn.called("k2", vec![json!({ "action": "list", "offset": 5 })]).await;
    assert_eq!(
        past[0].text,
        "<system-reminder>no threads past offset 5; there are 2</system-reminder>"
    );
    assert_eq!(
        result(&past[0]),
        ThreadResult::List {
            threads: Vec::new(),
            offset: 5,
            total: 2,
            next: None,
        }
    );
    turn.calling.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn what_was_said_is_found_by_every_keyword_or_any() {
    let beak = Beak::new().expect("Beak");
    let turn = Turn::start(&beak);
    let greeter = turn.greeter().await;
    let found = turn
        .called(
            "k1",
            vec![
                json!({ "action": "search", "keywords": "GREET" }),
                json!({ "action": "search", "keywords": "greeter nonesuch" }),
                json!({ "action": "search", "keywords": "greeter nonesuch", "matching": "any" }),
                json!({ "action": "search", "keywords": ["greeter"] }),
                json!({ "action": "search", "keywords": "greeter", "thread": "nonesuch" }),
            ],
        )
        .await;
    assert!(
        found[0].text.contains(&format!("  {greeter}  Greeter\n  entry ")) && found[0].text.contains("\"Greeter\""),
        "each find by its thread, its entry and the words around it: {}",
        found[0].text
    );
    assert_eq!(
        found[1].text, "<system-reminder>no entries hold those keywords</system-reminder>",
        "every keyword, by default"
    );
    assert!(found[2].text.contains("  Greeter\n"), "any keyword: {}", found[2].text);
    assert_eq!(found[3].status, CallStatus::InvalidArguments);
    assert!(
        found[3].text.starts_with("invalid arguments: /keywords: ") && found[3].text.contains("not of type \"string\""),
        "a misfit told by the field of the action it names: {}",
        found[3].text
    );
    assert_eq!(
        (found[4].status, found[4].text.as_str()),
        (CallStatus::ToolError, "no thread nonesuch")
    );
    let ThreadResult::Search { hits, offset, total, next } = result(&found[0]) else {
        panic!("a search result");
    };
    assert_eq!((offset, total, next), (0, 1, None));
    assert_eq!(hits[0].thread.id, greeter);
    assert_eq!(hits[0].thread.title.as_deref(), Some("Greeter"));
    assert!(hits[0].excerpt.contains("\"Greeter\""));
    assert!(hits[0].at.is_some());
    assert_eq!(
        result(&found[1]),
        ThreadResult::Search {
            hits: Vec::new(),
            offset: 0,
            total: 0,
            next: None,
        }
    );
    assert_eq!(found[3].structured_content, None);
    assert_eq!(found[4].structured_content, None);
    turn.calling.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn found_entries_are_read_with_anchors_and_typed_settings() {
    let beak = Beak::new().expect("Beak");
    let turn = Turn::start(&beak);
    let greeter = turn.greeter().await;
    let found = turn.called("find", vec![json!({ "action": "search", "keywords": "GREET" })]).await;
    let ThreadResult::Search { hits, .. } = result(&found[0]) else {
        panic!("a search result");
    };
    let entry = &hits[0].entry;
    let read = turn
        .called(
            "k2",
            vec![
                json!({ "action": "read", "thread": greeter }),
                json!({ "action": "read", "thread": greeter, "around": entry }),
                json!({ "action": "read", "thread": greeter, "around": "nonesuch" }),
                json!({ "action": "read", "thread": greeter, "before": entry, "after": entry }),
                json!({ "action": "read", "thread": "nonesuch" }),
                json!({ "action": "read", "thread": greeter, "before": entry }),
                json!({ "action": "read", "thread": greeter, "after": entry }),
            ],
        )
        .await;
    assert!(
        read[0].text.contains("settings: ") && read[0].text.contains("\"Greeter\""),
        "{}",
        read[0].text
    );
    assert!(
        read[0]
            .text
            .ends_with("<system-reminder>this reaches the conversation's start</system-reminder>")
    );
    assert_eq!(
        read[1].text, read[0].text,
        "a short conversation is one page about any of its entries"
    );
    assert_eq!(
        (read[2].status, read[2].text.as_str()),
        (CallStatus::ToolError, "no entry nonesuch in this thread")
    );
    assert_eq!(read[3].text, "give one of around, before and after");
    assert_eq!(
        (read[4].status, read[4].text.as_str()),
        (CallStatus::ToolError, "no thread nonesuch")
    );
    assert_eq!(read[5].text, "<system-reminder>no entries on this page</system-reminder>");
    assert_eq!(read[6].text, "<system-reminder>no entries on this page</system-reminder>");
    let ThreadResult::Read {
        thread,
        entries,
        before,
        after,
    } = result(&read[0])
    else {
        panic!("a read result");
    };
    assert_eq!(thread, greeter);
    assert_eq!((before, after), (None, None));
    assert_eq!(&entries[0].id, entry);
    let ConversationContent::Config { changes, .. } = &entries[0].content else {
        panic!("the thread's initial settings");
    };
    assert_eq!(changes["title"].from, json!(null));
    assert_eq!(changes["title"].to, json!("Greeter"));
    assert_eq!(result(&read[1]), result(&read[0]));
    for call in &read[5..] {
        assert_eq!(
            result(call),
            ThreadResult::Read {
                thread: greeter.clone(),
                entries: Vec::new(),
                before: None,
                after: None,
            }
        );
    }
    turn.calling.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn search_merges_threads_newest_first_and_pages_without_duplicates() {
    let beak = Beak::new().unwrap();
    let turn = Turn::start(&beak);
    let started = turn
        .called(
            "many",
            (0..25)
                .map(|at| {
                    json!({
                        "action": "start", "message": "hello", "title": format!("Historymatch {at}")
                    })
                })
                .collect(),
        )
        .await;
    assert!(started.iter().all(|call| matches!(result(call), ThreadResult::Start { .. })));
    let found = turn
        .called(
            "pages",
            vec![
                json!({ "action": "search", "keywords": "Historymatch" }),
                json!({ "action": "search", "keywords": "Historymatch", "offset": 20 }),
                json!({ "action": "search", "keywords": "Historymatch", "offset": 1000 }),
            ],
        )
        .await;
    let ids = |call: &Called| {
        let ThreadResult::Search { hits, .. } = result(call) else {
            panic!("a search page");
        };
        hits.into_iter().map(|hit| hit.entry).collect::<Vec<_>>()
    };
    let mut all = ids(&found[0]);
    assert_eq!(all.len(), 20);
    let rest = ids(&found[1]);
    assert_eq!(rest.len(), 5);
    all.extend(rest);
    assert!(all.windows(2).all(|pair| pair[0] > pair[1]));
    assert!(found[0].text.contains("entries 1-20 of 25; more from offset 20"));
    assert!(found[2].text.contains("no entries past offset 1000; there are 25"));
    assert!(matches!(
        result(&found[0]),
        ThreadResult::Search {
            offset: 0,
            total: 25,
            next: Some(20),
            ..
        }
    ));
    assert!(matches!(
        result(&found[1]),
        ThreadResult::Search {
            offset: 20,
            total: 25,
            next: None,
            ..
        }
    ));
    assert!(matches!(result(&found[2]), ThreadResult::Search { offset: 1000, total: 25, next: None, hits } if hits.is_empty()));
    let listed = turn.called("listed", vec![json!({ "action": "list" })]).await;
    assert!(matches!(result(&listed[0]), ThreadResult::List { total: 26, next: None, threads, .. } if threads.len() == 26));
    turn.calling.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn messages_and_stop_requests_identify_the_thread_and_delivery_without_reading_reminders() {
    let beak = Beak::new().unwrap();
    let turn = Turn::start(&beak);
    let started = turn.called("start", vec![json!({ "action": "start", "message": "begin" })]).await;
    let ThreadResult::Start {
        thread,
        delivery: first,
        report: false,
    } = result(&started[0])
    else {
        panic!("a started thread");
    };
    assert_ne!(first, "");
    let sent = turn
        .called("send", vec![json!({ "action": "send", "thread": thread, "message": "continue" })])
        .await;
    let ThreadResult::Send {
        thread: target,
        delivery,
        report: false,
    } = result(&sent[0])
    else {
        panic!("a sent message");
    };
    assert_eq!(target, thread);
    assert_ne!(delivery, "");
    assert_ne!(first, delivery);
    assert_eq!(
        sent[0].text,
        format!("<system-reminder>thread {thread} has your message</system-reminder>")
    );
    let stopped = turn.called("stop", vec![json!({ "action": "stop", "thread": thread })]).await;
    assert_eq!(result(&stopped[0]), ThreadResult::Stop { thread: thread.clone() });
    assert_eq!(
        stopped[0].text,
        format!("<system-reminder>thread {thread} was asked to stop its turn</system-reminder>")
    );
    turn.calling.close().await;
}

/// Cancels the call before handing it to the real history tool.
#[derive(Debug)]
struct CancelledHistory;
impl siphon::tools::Tool for CancelledHistory {
    const NAME: &'static str = "cancelled_history";
    const DESCRIPTION: &'static str = "A cancelled history read";
    type Args = siphon_thread::ThreadAction;

    fn effect(&self, _: &Self::Args, _: &siphon::machine::Base) -> siphon::tools::Effect {
        siphon::tools::Effect::Reads
    }

    async fn exec(&self, context: &siphon::tools::Context, action: Self::Args) -> Result<siphon::tools::Told, siphon::tools::Refusal> {
        let mut context = context.clone();
        context.cancelled = context.cancelled.child_token();
        context.cancelled.cancel();
        ThreadTool.exec(&context, action).await
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_history_read_stops_before_touching_a_missing_thread() {
    let beak = Beak::new().unwrap();
    let calling = Calling::start(&beak, |tools| tools.with(CancelledHistory));
    let folder = tempfile::tempdir().unwrap();
    let called = calling
        .called(
            "cancelled",
            folder.path().canonicalize().unwrap(),
            vec!["cancelled_history".to_owned()],
            vec![(
                "read".to_owned(),
                "cancelled_history".to_owned(),
                json!({ "action": "read", "thread": "missing" }).to_string(),
            )],
        )
        .await;
    assert_eq!(called[0].status, CallStatus::ToolError);
    assert_eq!(called[0].text, "thread history read cancelled");
    calling.close().await;
}
