//! The node's access as its own methods of the protocol: the sessions and links listed, a link made that pairs a client
//! once, a session revoked — and a session that may only read the access makes no link.

use octoii::access::{Access, Holder};
use octoii::home::Home;
use octoii::pairing::{AccessPlugin, Links};
use serde_json::{Value, json};
use siphon::testing::Client;
use siphon_protocol::Scope;

async fn asked(client: &mut Client, method: &str, params: Value) -> Value {
    let id = client.call(method, params).await;
    client.answer(&id).await
}

#[tokio::test(flavor = "multi_thread")]
async fn the_access_is_listed_paired_and_revoked_through_the_protocol() {
    let folder = tempfile::tempdir().expect("a folder");
    let home = Home::at(folder.path());
    home.prepare().expect("a Home");
    let tunnel = octoii_tunnel::Tunnel::builder(octoii_tunnel::SecretKey::generate())
        .bind()
        .await
        .expect("a tunnel");
    let links = Links {
        loopback: "http://127.0.0.1:7600".into(),
        tunnel,
    };
    let building = octoii::serve::building(&home, siphon::providers::SignInSetup::default()).expect("Siphon");
    let siphon = building.plugin(AccessPlugin { links }).start().expect("started");
    let access = Access::new(siphon.runtime.clone());
    let program = access
        .issue(Some("script".into()), vec![Scope::ThreadsRead])
        .await
        .expect("a session");
    let mut client = Client::connect(&siphon, vec![Scope::AccessRead, Scope::AccessWrite], 64);
    client.message().await;

    let made = asked(&mut client, "access.pair", json!({ "label": "phone", "scopes": ["threads.read"] })).await["result"].clone();
    assert!(
        made["url"]
            .as_str()
            .is_some_and(|url| url.starts_with("http://127.0.0.1:7600/pair#"))
    );
    assert!(made.get("device").is_none(), "no device link without a home relay");
    let credential = made["credential"].as_str().expect("a credential");
    let paired = access.redeem(credential, Holder::Cookie).await.expect("it pairs");
    assert_eq!(paired.shown.scopes, vec![Scope::ThreadsRead], "what the link granted");

    let listed = asked(&mut client, "access.list", json!({})).await["result"].clone();
    let labels: Vec<&str> = listed["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .filter_map(|session| session["label"].as_str())
        .collect();
    assert_eq!(labels, ["script", "phone"]);
    assert_eq!(listed["links"], json!([]), "a link used is gone");
    let revoked = asked(&mut client, "access.revoke", json!({ "id": program.shown.id })).await["result"].clone();
    assert_eq!(revoked, json!({ "revoked": "session" }));
    assert!(access.held(&program.credential).expect("read").is_none());

    let mut reader = Client::connect(&siphon, vec![Scope::AccessRead], 64);
    reader.message().await;
    assert_eq!(asked(&mut reader, "access.pair", json!({})).await["error"]["code"], "forbidden");
    siphon.runtime.close().await;
}
