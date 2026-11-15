//! The `thread` tool as a turn runs it: a thread it starts is one of the person's, listed with the others the most
//! recently active first — its id, how it stands, when it was last active, its project and its title — the thread that
//! asks marked, and a page past the last told so; what was said in it is found by keywords, every one or any, and its
//! conversation read back, about an entry found too. Arguments that do not fit are told by the field of the action they
//! name.

use serde_json::json;
use siphon::testing::{Beak, Called, Calling};
use siphon::tools::CallStatus;
use siphon_thread::ThreadTool;

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
        assert_eq!(started[0].status, CallStatus::Ok, "{}", started[0].text);
        let listed = self.called("list", vec![json!({ "action": "list" })]).await;
        listed[0].text.split_whitespace().next().expect("its id").to_owned()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_started_is_listed_with_the_others_the_most_recently_active_first() {
    let beak = Beak::new().expect("Beak");
    let turn = Turn::start(&beak);
    turn.greeter().await;
    let listed = turn.called("k1", vec![json!({ "action": "list" })]).await;
    let text = &listed[0].text;
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3, "{text}");
    assert!(lines[0].contains("  Greeter"), "the newest first: {text}");
    assert!(lines[1].starts_with("t1  new  ") && lines[1].ends_with("  (this thread)"), "{text}");
    assert_eq!(lines[2], "<system-reminder>2 threads</system-reminder>");
    let past = turn.called("k2", vec![json!({ "action": "list", "offset": 5 })]).await;
    assert_eq!(
        past[0].text,
        "<system-reminder>no threads past offset 5; there are 2</system-reminder>"
    );
    turn.calling.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn what_was_said_is_found_by_every_keyword_or_any_and_read_where_it_was_said() {
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
    let entry = found[0]
        .text
        .split("  entry ")
        .nth(1)
        .and_then(|rest| rest.split(':').next())
        .expect("an entry");
    let read = turn
        .called(
            "k2",
            vec![
                json!({ "action": "read", "thread": greeter }),
                json!({ "action": "read", "thread": greeter, "around": entry }),
                json!({ "action": "read", "thread": greeter, "around": "nonesuch" }),
                json!({ "action": "read", "thread": greeter, "before": entry, "after": entry }),
                json!({ "action": "read", "thread": "nonesuch" }),
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
    turn.calling.close().await;
}
