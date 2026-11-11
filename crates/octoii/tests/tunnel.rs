//! A device reaching the node through a relay, by the node's key, over HTTP/3 in its tunnel session: refused the
//! protocol till it pairs; paired by a link's credential, once, for a session's credential it sends as a bearer token;
//! then welcomed on the protocol's WebSocket, opened by extended CONNECT with that token.

use ::http::Request;
use bytes::Bytes;
use net::test_utils::run_relay_server;
use octoii::access::{self, Access};
use octoii::home::Home;
use octoii::http::{self, Node};
use octoii_tunnel::{CaTlsConfig, EndpointAddr, Message, PeerError, SecretKey, Tunnel};
use serde_json::{Value, json};
use siphon_protocol::Machine;

async fn place(relay: &net::RelayUrl) -> Tunnel {
    Tunnel::builder(SecretKey::generate())
        .relays(vec![relay.clone().into()])
        .trust(CaTlsConfig::insecure_skip_verify())
        .bind()
        .await
        .expect("bound")
}

/// The protocol's WebSocket at the peer, opened with the protocol's version and `bearer` where it is given: the first
/// message it is sent.
async fn welcomed(peer: &octoii_tunnel::Peer, bearer: Option<&str>) -> Result<Value, PeerError> {
    let mut head = Request::builder().header("sec-websocket-protocol", "octoii.1");
    if let Some(bearer) = bearer {
        head = head.header("authorization", format!("Bearer {bearer}"));
    }
    let head = head.body(()).expect("a head");
    let stream = peer.websocket("/ws", head).await?;
    let mut socket = octoii_tunnel::socket::client(stream).await;
    let Some(Ok(Message::Text(text))) = futures_util::StreamExt::next(&mut socket).await else {
        panic!("a welcome")
    };
    Ok(serde_json::from_str(&text).expect("JSON"))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_pairs_through_the_tunnel_and_speaks_the_protocol_by_its_bearer_token() {
    let (_map, relay, _server) = run_relay_server().await.expect("a relay");
    let folder = tempfile::tempdir().expect("a folder");
    let home = Home::at(folder.path());
    home.prepare().expect("a Home");
    let siphon = octoii::serve::siphon(&home, siphon::providers::SignInSetup::default()).expect("Siphon");
    let access = Access::new(siphon.runtime.clone());
    let node = Node {
        siphon: siphon.clone(),
        access: access.clone(),
        machine: Machine { name: "here".into() },
        origins: Vec::new(),
        app: None,
    };
    let machine = place(&relay).await;
    machine.online().await;
    let serving = machine.clone();
    let router = http::router(node);
    tokio::spawn(async move { serving.serve(router).await });

    let device = place(&relay).await;
    let peer = device
        .connect(EndpointAddr::new(machine.key()).with_relay_url(relay.clone()))
        .await
        .expect("reached");
    assert!(
        matches!(welcomed(&peer, None).await, Err(PeerError::Refused(status)) if status == 401),
        "no session till it pairs"
    );

    let link = access
        .link(Some("phone".into()), access::ALL.to_vec(), access::LINK_LASTS)
        .await
        .expect("a link");
    let pairing = Request::post("/pair")
        .header("content-type", "application/json")
        .body(Bytes::from(json!({ "credential": link.credential }).to_string()))
        .expect("a request");
    let paired = peer.request(pairing.clone()).await.expect("answered");
    assert_eq!(paired.status(), 200);
    assert!(
        paired.headers().get("set-cookie").is_none(),
        "a device holds its session as a bearer token, not a cookie"
    );
    let given: Value = serde_json::from_slice(paired.body()).expect("JSON");
    let credential = given["credential"].as_str().expect("its credential");
    assert_eq!(peer.request(pairing).await.expect("answered").status(), 403, "a link pairs once");
    assert!(
        matches!(welcomed(&peer, None).await, Err(PeerError::Refused(status)) if status == 401),
        "its key alone holds nothing"
    );

    let welcome = welcomed(&peer, Some(credential)).await.expect("welcomed");
    assert_eq!(
        (welcome["event"].as_str(), welcome["scopes"].as_array().map(Vec::len)),
        (Some("welcome"), Some(8))
    );
    let session = access.held(credential).expect("read").expect("its session");
    assert_eq!(session.holder, Some(access::Holder::Bearer));
    assert_eq!(welcome["session"].as_str(), Some(session.id.as_str()));
    device.close().await;
    machine.close().await;
    siphon.runtime.close().await;
}
