//! The terminals as Siphon's extension, through a client's connection: a terminal opened in its thread's workspace, its
//! thread's terminals told, its screen drawn first with the offset it stands at, what it writes after told from there,
//! and a client that stood at an offset told from that offset on.

use std::path::PathBuf;

use beak::Beak;
use serde_json::json;
use siphon::Siphon;
use siphon::testing::{self, Client};
use siphon_protocol::Scope;
use siphon_terminals::TerminalsExtension;

/// The bytes of `pieces` as one text.
fn text(pieces: &[bytes::Bytes]) -> String {
    pieces.iter().map(|piece| String::from_utf8_lossy(piece)).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_is_opened_drawn_and_told_as_it_writes_each_piece_once() {
    let beak = Beak::new().expect("Beak");
    let machine = siphon_machine::Host {
        shell: Some(PathBuf::from("/bin/sh")),
        ..siphon_machine::Host::default()
    };
    let siphon = Siphon::builder(&beak, testing::setup(machine))
        .expect("built")
        .extension(TerminalsExtension)
        .start()
        .expect("started");
    let folder = tempfile::tempdir().expect("a folder");
    let workspace = folder.path().canonicalize().expect("its real path");
    let mut client = Client::connect(&siphon, vec![Scope::ThreadsOperate, Scope::TerminalsOperate], 64);
    client.message().await;
    let id = client
        .call("threads.create", json!({ "config": { "workspace": workspace }, "key": "k1" }))
        .await;
    let thread = client.answer(&id).await["result"]["thread"].as_str().expect("a thread").to_owned();
    let listed = client.subscribe(json!({ "type": "terminals", "thread": thread })).await;
    let (none, _) = client.event(&listed, |event| event["event"] == "terminals").await;
    assert_eq!(none["terminals"], json!([]));

    let id = client
        .call("terminals.open", json!({ "thread": thread, "size": { "rows": 24, "cols": 80 } }))
        .await;
    let terminal = client.answer(&id).await["result"]["terminal"]
        .as_str()
        .expect("a terminal")
        .to_owned();
    let (one, _) = client
        .event(&listed, |event| event["event"] == "terminals" && event["terminals"] != json!([]))
        .await;
    assert_eq!(
        (one["terminals"][0]["id"].as_str(), one["terminals"][0]["number"].as_u64()),
        (Some(terminal.as_str()), Some(1))
    );

    let watched = client.subscribe(json!({ "type": "terminal", "terminal": terminal })).await;
    let (screen, _) = client.event(&watched, |event| event["event"] == "screen").await;
    let mut at = screen["through"].as_u64().expect("an offset");
    let id = client
        .call(
            "terminals.write",
            json!({ "terminal": terminal, "data": "pwd; echo one-$((40+2))\r" }),
        )
        .await;
    client.answer(&id).await;
    let mut pieces = Vec::new();
    while !text(&pieces).contains("one-42") {
        let (piece, bytes) = client.event(&watched, |event| event["event"] == "output").await;
        assert_eq!(piece["offset"].as_u64(), Some(at), "each piece where the last ended");
        at += bytes.len() as u64;
        pieces.push(bytes);
    }
    assert!(
        text(&pieces).contains(&workspace.to_string_lossy().into_owned()),
        "its shell in the thread's workspace"
    );

    let again = client
        .subscribe(json!({ "type": "terminal", "terminal": terminal, "from": screen["through"] }))
        .await;
    let mut replayed = Vec::new();
    while !text(&replayed).contains("one-42") {
        let (_, bytes) = client.event(&again, |event| event["event"] == "output").await;
        replayed.push(bytes);
    }
    assert!(
        text(&replayed).starts_with(&text(&pieces)),
        "a client that stood at an offset told the same from there"
    );
    let id = client.call("threads.delete", json!({ "thread": thread })).await;
    client.answer(&id).await;
    client
        .event(&listed, |event| event["event"] == "terminals" && event["terminals"] == json!([]))
        .await;
    siphon.runtime.close().await;
}
