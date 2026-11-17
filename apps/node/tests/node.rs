//! The node on a Home of its own: its commands reach it through its control socket; a browser pairs by a link's
//! credential, once, for a session in its cookie; a program holds a bearer token; each speaks the protocol on the
//! WebSocket the version names, welcomed with what its session may do; a page of another origin is refused; and a
//! session revoked holds nothing.

use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt as _, StreamExt as _};
use http_body_util::{BodyExt as _, Full};
use hyper::Method;
use octoii_node::control;
use octoii_node::home::Home;
use octoii_node::serve::{Served, serve_until};
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
    let home = Home::at(folder.path().join("home"));
    let app = folder.path().join("app");
    std::fs::create_dir(&app).expect("the app's folder");
    std::fs::write(app.join("index.html"), "the app").expect("its page");
    let stop = tokio_util::sync::CancellationToken::new();
    let stopping = stop.clone();
    let served = Served {
        home: home.clone(),
        port: 0,
        app: Some(app),
        relays: Vec::new(),
    };
    let node = tokio::spawn(serve_until(served, async move { stopping.cancelled().await }));
    let socket = home.socket().expect("its socket");
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
    let (opened, _, page) = request(&address, Method::GET, "/pair", &[], Value::Null).await;
    assert_eq!((opened, &page[..]), (200, &b"the app"[..]), "a link opens the app");
    let own = [("origin", loopback.as_str()), ("content-type", "application/json")];
    let foreign = [("origin", "https://elsewhere.example"), ("content-type", "application/json")];
    let (refused, _, _) = request(&address, Method::POST, "/pair", &foreign, json!({ "credential": credential })).await;
    assert_eq!(refused, 403, "a page of another origin pairs nothing");
    let (paired, headers, _) = request(&address, Method::POST, "/pair", &own, json!({ "credential": credential })).await;
    assert_eq!(paired, 200);
    let cookie = headers["set-cookie"].to_str().expect("a cookie").to_owned();
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"), "{cookie}");
    let port: u16 = address.rsplit(':').next().and_then(|port| port.parse().ok()).expect("its port");
    assert!(
        cookie.starts_with(&format!("{}=", octoii_node::http::cookie_of(port))),
        "named by its own port, as another node's on the machine is by its: {cookie}"
    );
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
        welcome["scopes"],
        json!(octoii_node::access::ALL),
        "a link made by the person grants everything"
    );

    verify_sign_in(&address, &loopback, &browser).await;

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

/// The node owns the callback URL and accepts each matching sign-in state only once.
async fn verify_sign_in(address: &str, loopback: &str, browser: &[(&str, &str)]) {
    let mut client_request = format!("ws://{address}/ws").into_client_request().expect("a request");
    for &(name, value) in browser {
        let name = tokio_tungstenite::tungstenite::http::HeaderName::try_from(name).expect("a name");
        client_request.headers_mut().insert(name, value.parse().expect("a header"));
    }
    let (mut client, _) = tokio_tungstenite::connect_async(client_request).await.expect("a client");
    client.next().await.expect("welcomed").expect("a message");
    client
        .send(Message::Text(
            json!({ "id": "sign-in", "method": "providers.sign-in", "params": { "provider": "chatgpt" } })
                .to_string()
                .into(),
        ))
        .await
        .expect("a sign-in asked for");
    let begun = loop {
        let message = client.next().await.expect("an answer").expect("a message");
        let answer: Value = serde_json::from_str(message.to_text().expect("text")).expect("an answer");
        if answer["id"] == "sign-in" {
            break answer;
        }
    };
    let authorize = reqwest::Url::parse(begun["result"]["url"].as_str().expect("a sign-in begun")).expect("an address");
    let asked = |name: &str| {
        authorize
            .query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
            .expect("an authorization parameter")
    };
    assert_eq!(asked("redirect_uri"), format!("{loopback}/auth/callback"));
    let host_id = asked("ext_agent_host_id");
    let host_uuid = uuid::Uuid::parse_str(&host_id).expect("a host UUID URI");
    assert_eq!(host_uuid.get_version(), Some(uuid::Version::Sha1));
    assert_eq!(host_id, host_uuid.urn().to_string());
    assert_eq!(asked("code_challenge_method"), "S256");
    let state = asked("state");
    assert!(!state.is_empty() && !asked("code_challenge").is_empty());
    let (_, _, refused) = request(
        address,
        Method::GET,
        "/auth/callback?error=access_denied&state=other",
        &[],
        Value::Null,
    )
    .await;
    assert!(String::from_utf8_lossy(&refused).contains("no sign-in waits"));
    let returned = format!("/auth/callback?error=access_denied&state={state}");
    let (_, _, declined) = request(address, Method::GET, &returned, &[], Value::Null).await;
    assert!(
        String::from_utf8_lossy(&declined).contains("The person declined to sign in with ChatGPT"),
        "a valid state reaches the driver's attempt after an unrelated return"
    );
    let (_, _, repeated) = request(address, Method::GET, &returned, &[], Value::Null).await;
    assert!(
        String::from_utf8_lossy(&repeated).contains("no sign-in waits"),
        "an attempt is consumed once"
    );
    client.close(None).await.expect("closed");
}
