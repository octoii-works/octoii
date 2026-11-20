//! A device reaching the node through a relay, by the node's key, over HTTP/3 in its tunnel session: refused the
//! protocol till it pairs; paired by a link's credential, once, for a session's credential it sends as a bearer token;
//! then welcomed on the protocol's WebSocket, opened by extended CONNECT with that token.

use ::http::Request;
use bytes::Bytes;
use futures_util::StreamExt as _;
use net::test_utils::run_relay_server;
use octoii::access;
use octoii::protocol::{CALL_PATH, Machine, SUBPROTOCOL, WATCH_PATH};
use octoii_node::home::Home;
use octoii_node::http::{self, Node};
use octoii_tunnel::{CaTlsConfig, EndpointAddr, Message, PeerError, SecretKey, Tunnel};
use serde_json::{Value, json};

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
    let mut head = Request::builder().header("sec-websocket-protocol", SUBPROTOCOL);
    if let Some(bearer) = bearer {
        head = head.header("authorization", format!("Bearer {bearer}"));
    }
    let head = head.body(()).expect("a head");
    let stream = peer.websocket(WATCH_PATH, head).await?;
    let mut socket = octoii_tunnel::socket::client(stream).await;
    let Some(Ok(Message::Text(text))) = socket.next().await else {
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
    let siphon = octoii_node::serve::siphon(&home, octoii::providers::SignInSetup::default()).expect("Octoii");
    let access = siphon.access();
    let node = Node {
        siphon: siphon.clone(),
        access: access.clone(),
        machine: Machine::of("here".into(), &siphon.machine),
        origins: Vec::new(),
        cookie: http::cookie_of(7600),
        home,
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
    assert_eq!(welcome["event"], "welcome");
    assert_eq!(welcome["scopes"], json!(octoii::access::ALL));
    let session = access.held(credential).expect("read").expect("its session");
    assert_eq!(session.holder, Some(access::Holder::Bearer));
    assert_eq!(welcome["session"].as_str(), Some(session.id.as_str()));

    let call = Request::post(CALL_PATH)
        .header("authorization", format!("Bearer {credential}"))
        .header("content-type", "application/json")
        .body(Bytes::from(json!({ "method": "threads.list", "params": {} }).to_string()))
        .unwrap();
    let answer = peer.request(call.clone()).await.unwrap();
    assert_eq!(answer.status(), 200);
    assert_eq!(
        serde_json::from_slice::<Value>(answer.body()).unwrap(),
        json!({ "result": { "threads": [] } })
    );
    let head = Request::builder()
        .header("authorization", format!("Bearer {credential}"))
        .header("sec-websocket-protocol", SUBPROTOCOL)
        .body(())
        .unwrap();
    let stream = peer.websocket(WATCH_PATH, head).await.expect("an active watch");
    let mut watch = octoii_tunnel::socket::client(stream).await;
    assert!(matches!(watch.next().await, Some(Ok(Message::Text(_)))));
    access.revoke(&session.id).await.expect("revoked");
    let revoked = tokio::time::timeout(std::time::Duration::from_secs(3), watch.next()).await.unwrap();
    let Some(Ok(Message::Text(revoked))) = revoked else {
        panic!("the revocation event")
    };
    assert_eq!(serde_json::from_str::<Value>(&revoked).unwrap(), json!({ "event": "revoked" }));
    assert_eq!(peer.request(call).await.unwrap().status(), 401);
    device.close().await;
    machine.close().await;
    siphon.close().await;
}
