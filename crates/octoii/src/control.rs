//! The node's control socket, `run/octoii.sock` in its Home, which the person alone may reach, as a system's daemons
//! are reached by theirs: what the node's commands ask of the running node — a link that pairs a client, a session for
//! a program, how it stands, the access its clients hold and its revoking — each a request of HTTP/1.1 over the
//! socket, answered with JSON.

use std::path::Path;

use axum::Router;
use axum::extract::{Json, Path as Named, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use bytes::Bytes;
use http_body_util::{BodyExt as _, Full};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use siphon_protocol::Scope;

use crate::access::{self, Refused};
use crate::http::Node;

/// What the control socket serves with: the node, the address of its loopback, its Home's folder, and its tunnel.
#[derive(Debug, Clone)]
pub struct Control {
    pub node: Node,
    pub loopback: String,
    pub home: std::path::PathBuf,
    pub tunnel: octoii_tunnel::Tunnel,
}

/// What a link or a session is asked with: its label, and what it grants — everything, where none is given.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Asked {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<Scope>>,
}

/// The control socket's routes.
pub fn router(control: Control) -> Router {
    Router::new()
        .route("/links", post(link))
        .route("/tokens", post(token))
        .route("/status", get(status))
        .route("/access", get(listed))
        .route("/access/{id}", delete(revoke))
        .with_state(control)
}

fn failed(error: &Refused) -> Response {
    let status = match error {
        Refused::NoSuch | Refused::NoSuchLink => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(json!({ "error": error.to_string() }))).into_response()
}

/// Where the web app is served to a device that reaches the node through the tunnel.
pub const APP: &str = "https://app.octoii.com";

/// A link that pairs one client, and the addresses that open it, its credential in the fragment a page reads and no
/// request sends: on this computer, at the loopback; on another device, at the web app, where the node has a home relay
/// — with the node's key and its relay, by which the page dials it.
async fn link(State(control): State<Control>, Json(asked): Json<Asked>) -> Response {
    let scopes = asked.scopes.unwrap_or_else(|| access::ALL.to_vec());
    match control.node.access.link(asked.label, scopes, access::LINK_LASTS).await {
        Ok(made) => {
            let url = format!("{}/pair#{}", control.loopback, made.credential);
            let addr = control.tunnel.addr();
            let device = addr
                .relay_urls()
                .next()
                .map(|relay| format!("{APP}/pair#key={}&relay={relay}&link={}", control.tunnel.key(), made.credential));
            Json(json!({ "link": made.shown, "url": url, "device": device })).into_response()
        }
        Err(error) => failed(&error),
    }
}

/// A session for a program: its bearer token.
async fn token(State(control): State<Control>, Json(asked): Json<Asked>) -> Response {
    let scopes = asked.scopes.unwrap_or_else(|| access::ALL.to_vec());
    match control.node.access.issue(asked.label, scopes).await {
        Ok(made) => Json(json!({ "session": made.shown, "token": made.credential })).into_response(),
        Err(error) => failed(&error),
    }
}

/// How the node stands: its version, its Home, its machine and its loopback's address; the key devices reach it by,
/// its home relay, where it is seen from outside.
async fn status(State(control): State<Control>) -> Response {
    let addr = control.tunnel.addr();
    let relay = addr.relay_urls().next().map(ToString::to_string);
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "home": control.home,
        "machine": control.node.machine.name,
        "loopback": control.loopback,
        "key": control.tunnel.key().to_string(),
        "relay": relay,
        "seen_from_outside": control.tunnel.seen_from_outside(),
    }))
    .into_response()
}

/// The sessions and the links.
async fn listed(State(control): State<Control>) -> Response {
    match control.node.access.list() {
        Ok((sessions, links)) => Json(json!({ "sessions": sessions, "links": links })).into_response(),
        Err(error) => failed(&error),
    }
}

/// A session or a link revoked.
async fn revoke(State(control): State<Control>, Named(id): Named<String>) -> Response {
    match control.node.access.revoke(&id).await {
        Ok(what) => Json(json!({ "revoked": what })).into_response(),
        Err(error) => failed(&error),
    }
}

/// Why the running node could not be asked.
#[derive(Debug, thiserror::Error)]
pub enum Unreached {
    #[error("no node runs for this Home ({0}); start it with `octoii serve`, or as a service with `octoii service start`")]
    NotRunning(String),
    #[error("the node answered {status}: {body}")]
    Refused { status: u16, body: String },
    #[error(transparent)]
    Http(#[from] hyper::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Asks the node whose control socket is `socket`: `method` at `path`, with `body` where there is one — its answer.
///
/// # Errors
/// `NotRunning` where no node listens; `Refused` with what it answered otherwise.
pub async fn ask(socket: &Path, method: hyper::Method, path: &str, body: Option<Value>) -> Result<Value, Unreached> {
    let stream = tokio::net::UnixStream::connect(socket)
        .await
        .map_err(|_| Unreached::NotRunning(socket.display().to_string()))?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream)).await?;
    tokio::spawn(connection);
    let bytes = body.map(|body| serde_json::to_vec(&body)).transpose()?.unwrap_or_default();
    let request = hyper::Request::builder()
        .method(method)
        .uri(path)
        .header("host", "octoii")
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(bytes)))
        .map_err(|error| Unreached::Refused {
            status: 0,
            body: error.to_string(),
        })?;
    let response = sender.send_request(request).await?;
    let status = response.status();
    let read = response.into_body().collect().await?.to_bytes();
    if !status.is_success() {
        return Err(Unreached::Refused {
            status: status.as_u16(),
            body: String::from_utf8_lossy(&read).into_owned(),
        });
    }
    Ok(serde_json::from_slice(&read)?)
}
