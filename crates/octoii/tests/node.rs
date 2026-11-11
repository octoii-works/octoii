//! The node on a Home of its own: its commands reach it through its control socket; a browser pairs by a link's
//! credential, once, for a session in its cookie; a program holds a bearer token; each speaks the protocol on the
//! WebSocket the version names, welcomed with what its session may do; a page of another origin is refused; and a
//! session revoked holds nothing.

use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt as _, Full};
use hyper::Method;
use octoii::control;
use octoii::home::Home;
use octoii::serve::{Served, serve_until};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::tungstenite::{Error as SocketError, Message};

/// A request of HTTP/1.1 to the loopback at `address`: its status, its headers and its body.
async fn request(address: &str, method: Method, path: &str, headers: &[(&str, &str)], body: Value) -> (u16, hyper::HeaderMap, Bytes) {
    let stream = tokio::net::TcpStream::connect(address).await.expect("connected");
    let (mut sender, connection) = hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream))
        .await
        .expect("HTTP");
    tokio::spawn(connection);
    let mut built = hyper::Request::builder().method(method).uri(path).header("host", address);
    for (name, value) in headers {
        built = built.header(*name, *value);
    }
    let request = built.body(Full::new(Bytes::from(body.to_string()))).expect("a request");
    let response = sender.send_request(request).await.expect("answered");
    let (parts, body) = response.into_parts();
    (parts.status.as_u16(), parts.headers, body.collect().await.expect("read").to_bytes())
}

/// The protocol's WebSocket at `address`, with `headers`: the first message it is sent.
async fn welcomed(address: &str, headers: &[(&str, &str)]) -> Result<Value, SocketError> {
    let mut request = format!("ws://{address}/ws").into_client_request().expect("a request");
    for (name, value) in headers {
        let name = tokio_tungstenite::tungstenite::http::HeaderName::try_from(*name).expect("a name");
        request.headers_mut().insert(name, value.parse().expect("a header"));
    }
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await?;
    let Some(Ok(Message::Text(text))) = futures_util::StreamExt::next(&mut socket).await else {
        panic!("a welcome")
    };
    Ok(serde_json::from_str(&text).expect("JSON"))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_browser_and_a_program_reach_the_node_by_the_sessions_its_commands_make() {
    let folder = tempfile::tempdir().expect("a folder");
    let home = Home::at(folder.path());
    let stop = tokio_util::sync::CancellationToken::new();
    let stopping = stop.clone();
    let served = Served {
        home: home.clone(),
        port: 0,
        app: None,
        relays: Vec::new(),
    };
    let node = tokio::spawn(serve_until(served, async move { stopping.cancelled().await }));
    let socket = home.socket();
    let status = loop {
        if let Ok(status) = control::ask(&socket, Method::GET, "/status", None).await {
            break status;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let loopback = status["loopback"].as_str().expect("its loopback").to_owned();
    let address = loopback.trim_start_matches("http://").to_owned();

    let made = control::ask(&socket, Method::POST, "/links", Some(json!({ "label": "Chrome" })))
        .await
        .expect("a link");
    let credential = made["url"]
        .as_str()
        .expect("its url")
        .split_once('#')
        .expect("a fragment")
        .1
        .to_owned();
    let own = [("origin", loopback.as_str()), ("content-type", "application/json")];
    let foreign = [("origin", "https://elsewhere.example"), ("content-type", "application/json")];
    let (refused, _, _) = request(&address, Method::POST, "/pair", &foreign, json!({ "credential": credential })).await;
    assert_eq!(refused, 403, "a page of another origin pairs nothing");
    let (paired, headers, _) = request(&address, Method::POST, "/pair", &own, json!({ "credential": credential })).await;
    assert_eq!(paired, 200);
    let cookie = headers["set-cookie"].to_str().expect("a cookie").to_owned();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"), "{cookie}");
    let (again, _, _) = request(&address, Method::POST, "/pair", &own, json!({ "credential": credential })).await;
    assert_eq!(again, 403, "a link pairs once");

    let session = cookie.split(';').next().expect("its value").to_owned();
    let browser = [
        ("cookie", session.as_str()),
        ("origin", loopback.as_str()),
        ("sec-websocket-protocol", "octoii.1"),
    ];
    let welcome = welcomed(&address, &browser).await.expect("welcomed");
    assert_eq!(welcome["event"], "welcome");
    assert_eq!(
        welcome["scopes"].as_array().map(Vec::len),
        Some(8),
        "a link made by the person grants everything"
    );
    let unversioned = [("cookie", session.as_str()), ("origin", loopback.as_str())];
    assert!(
        welcomed(&address, &unversioned).await.is_err(),
        "a socket of no version of the protocol's is refused"
    );
    let elsewhere = [
        ("cookie", session.as_str()),
        ("origin", "https://elsewhere.example"),
        ("sec-websocket-protocol", "octoii.1"),
    ];
    assert!(
        welcomed(&address, &elsewhere).await.is_err(),
        "a page of another origin is refused its socket"
    );

    let asked = json!({ "label": "script", "scopes": ["threads.read"] });
    let token = control::ask(&socket, Method::POST, "/tokens", Some(asked)).await.expect("a token");
    let bearer = format!("Bearer {}", token["token"].as_str().expect("a token"));
    let program = [("authorization", bearer.as_str()), ("sec-websocket-protocol", "octoii.1")];
    let welcome = welcomed(&address, &program).await.expect("welcomed");
    assert_eq!(welcome["scopes"], json!(["threads.read"]));

    let listed = control::ask(&socket, Method::GET, "/access", None).await.expect("listed");
    let sessions = listed["sessions"].as_array().expect("sessions");
    assert_eq!(sessions.len(), 2);
    let chrome = sessions.iter().find(|session| session["label"] == "Chrome").expect("the browser's");
    assert_eq!(chrome["holder"], "cookie");
    let id = chrome["id"].as_str().expect("an id");
    control::ask(&socket, Method::DELETE, &format!("/access/{id}"), None)
        .await
        .expect("revoked");
    assert!(welcomed(&address, &browser).await.is_err(), "a revoked session holds nothing");

    stop.cancel();
    node.await.expect("joined").expect("served and stopped");
    assert!(!socket.exists(), "its control socket gone with it");
}
