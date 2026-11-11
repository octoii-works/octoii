//! The node's HTTP, as each way a client reaches it carries it: the protocol's WebSocket at `/ws`, its version agreed
//! by the subprotocol `octoii.1` and its client known by the session it holds — a browser's in its cookie, a program's
//! as a bearer token; the one request a browser pairs by, `/pair`, which redeems a link's credential for a session
//! held in an `HttpOnly` cookie its page cannot read; and the web app's files, every path of no file its page. A request
//! of a page of another origin is refused, as a browser lets any page send one to any address.

use std::path::PathBuf;

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Json, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures_util::{SinkExt as _, StreamExt as _};
use serde::Deserialize;
use siphon::Siphon;
use siphon::connection::{Access as Client, OUTGOING_AHEAD, Outgoing, serve};
use siphon_protocol::{Machine, SUBPROTOCOL};
use tokio::sync::mpsc;
use tower_http::services::{ServeDir, ServeFile};

use crate::access::{self, Access, Holder, Shown};

/// The cookie a browser holds its session in.
pub const COOKIE: &str = "octoii_session";

/// What the node's HTTP serves with: Siphon, the access, the machine's name, the origins its own pages come from, and
/// the web app's folder, where it has one.
#[derive(Debug, Clone)]
pub struct Node {
    pub siphon: Siphon,
    pub access: Access,
    pub machine: Machine,
    pub origins: Vec<String>,
    pub app: Option<PathBuf>,
}

/// The node's routes.
pub fn router(node: Node) -> Router {
    let app = node.app.clone();
    let mut router = Router::new().route("/ws", get(socket)).route("/pair", post(pair)).with_state(node);
    if let Some(app) = app {
        let page = ServeFile::new(app.join("index.html"));
        router = router.fallback_service(ServeDir::new(app).precompressed_br().fallback(page));
    }
    router
}

/// Why a request was refused: its status, and words for whoever reads it.
fn refused(status: StatusCode, why: &str) -> Response {
    (status, why.to_owned()).into_response()
}

/// A request refused before it was answered: its status, and why.
struct Denied(StatusCode, String);

impl IntoResponse for Denied {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}

/// Whether a request of `headers` comes from a page of another origin than the node's own.
fn foreign(node: &Node, headers: &HeaderMap) -> bool {
    headers
        .get(header::ORIGIN)
        .and_then(|origin| origin.to_str().ok())
        .is_some_and(|origin| !node.origins.iter().any(|own| own == origin))
}

/// The credential a request of `headers` presents: a bearer token, else a browser's cookie.
fn credential(headers: &HeaderMap) -> Option<String> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if let Some(bearer) = bearer {
        return Some(bearer.trim().to_owned());
    }
    let cookies = headers.get_all(header::COOKIE).into_iter().filter_map(|value| value.to_str().ok());
    cookies
        .flat_map(|cookies| cookies.split(';'))
        .filter_map(|cookie| cookie.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE)
        .map(|(_, value)| value.to_owned())
}

/// The session a request of `headers` holds.
fn session(node: &Node, headers: &HeaderMap) -> Result<Shown, Denied> {
    if foreign(node, headers) {
        return Err(Denied(StatusCode::FORBIDDEN, "a page of another origin".into()));
    }
    let credential = credential(headers).ok_or_else(|| Denied(StatusCode::UNAUTHORIZED, "no session".into()))?;
    match node.access.held(&credential) {
        Ok(Some(session)) => Ok(session),
        Ok(None) => Err(Denied(StatusCode::UNAUTHORIZED, "no such session".into())),
        Err(error) => Err(Denied(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())),
    }
}

/// The protocol's WebSocket, for the session the request holds, where it asks for the protocol's version.
async fn socket(State(node): State<Node>, headers: HeaderMap, upgrade: WebSocketUpgrade) -> Response {
    let session = match session(&node, &headers) {
        Ok(session) => session,
        Err(denied) => return denied.into_response(),
    };
    let upgrade = upgrade.protocols([SUBPROTOCOL]);
    if upgrade.selected_protocol().is_none() {
        return refused(
            StatusCode::BAD_REQUEST,
            "the WebSocket asks for no protocol of this node's: octoii.1",
        );
    }
    upgrade.on_upgrade(move |socket| carried(node, session, socket))
}

/// Carries the protocol over `socket` for the client of `session`: what it sends taken as calls, what Siphon sends it
/// written as text and binary frames, till either side closes.
async fn carried(node: Node, session: Shown, socket: WebSocket) {
    let (mut writing, mut reading) = socket.split();
    let (calls, incoming) = mpsc::channel(OUTGOING_AHEAD);
    let (outgoing, mut sent) = mpsc::channel(OUTGOING_AHEAD);
    let client = Client {
        session: session.id,
        scopes: session.scopes,
    };
    let serving = tokio::spawn(serve(node.siphon.clone(), client, node.machine.clone(), incoming, outgoing));
    let writer = tokio::spawn(async move {
        while let Some(message) = sent.recv().await {
            let message = match message {
                Outgoing::Text(text) => Message::Text(text.into()),
                Outgoing::Binary(bytes) => Message::Binary(bytes),
            };
            if writing.send(message).await.is_err() {
                return;
            }
        }
    });
    while let Some(Ok(message)) = reading.next().await {
        match message {
            Message::Text(text) => {
                if calls.send(text.as_str().to_owned()).await.is_err() {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    drop(calls);
    let _ = serving.await;
    writer.abort();
}

/// What a browser pairs by: the credential of a link.
#[derive(Debug, Deserialize)]
struct Pairing {
    credential: String,
}

/// A browser paired by a link's credential: its session, held in an `HttpOnly` cookie its page cannot read, sent to this
/// node's own pages alone.
async fn pair(State(node): State<Node>, headers: HeaderMap, Json(pairing): Json<Pairing>) -> Response {
    if foreign(&node, &headers) {
        return refused(StatusCode::FORBIDDEN, "a page of another origin");
    }
    let made = match node.access.redeem(&pairing.credential, Holder::Cookie, None).await {
        Ok(made) => made,
        Err(access::Refused::NoSuchLink) => return refused(StatusCode::FORBIDDEN, "the link was used, or went"),
        Err(error) => return refused(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    };
    let cookie = format!(
        "{COOKIE}={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
        made.credential,
        access::SESSION_LASTS.as_secs()
    );
    let body = serde_json::json!({ "session": made.shown.id });
    ([(header::SET_COOKIE, cookie)], Json(body)).into_response()
}
