//! The node's HTTP, as each way a client reaches it carries it — HTTP/1.1 and HTTP/2 on the loopback, HTTP/3 in a
//! device's tunnel session — one router for both: the protocol's WebSocket at `/ws`, its version agreed by the
//! subprotocol `octoii.1` — opened by an upgrade on the loopback, by extended CONNECT (RFC 9220) in a tunnel session —
//! its client known by the session it holds: a browser's in its cookie, a program's as a bearer token, a device's by
//! the key its tunnel session shows; the one request a client pairs by, `/pair`, which redeems a link's credential for
//! a session — a browser's held in an `HttpOnly` cookie its page cannot read, a device's by its key; and the web app's
//! files, every path of no file its page. On the loopback a request of a page of another origin is refused, as a
//! browser lets any page send one to any address; in a tunnel session the key that dialled is who asks.

use std::path::PathBuf;

use axum::Router;
use axum::extract::ws::{self, WebSocket, WebSocketUpgrade};
use axum::extract::{Extension, Json, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodFilter, get, on, post};
use futures_util::{Sink, SinkExt as _, Stream, StreamExt as _};
use octoii_tunnel::{OnConnect, Remote};
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
    let mut router = Router::new()
        .route("/ws", get(socket).merge(on(MethodFilter::CONNECT, tunnel_socket)))
        .route("/pair", post(pair))
        .route(
            "/threads/{thread}/files",
            post(crate::files::sent).layer(axum::extract::DefaultBodyLimit::max(siphon_threads::files::BYTES_MAX)),
        )
        .route("/threads/{thread}/files/{hash}", get(crate::files::kept))
        .route("/machine", get(crate::files::machined))
        .route("/sign-in/{provider}/callback", get(signed_in))
        .with_state(node);
    if let Some(app) = app {
        let page = ServeFile::new(app.join("index.html"));
        router = router.fallback_service(ServeDir::new(app).precompressed_br().fallback(page));
    }
    router
}

/// A request refused: its status, and words for whoever reads it.
pub(crate) struct Denied(pub(crate) StatusCode, pub(crate) String);

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

/// The session a request of `headers` holds — of the key `remote`, where it came in a tunnel session: its bearer
/// token or its cookie, else the session the key holds.
pub(crate) fn session(node: &Node, headers: &HeaderMap, remote: Option<&Remote>) -> Result<Shown, Denied> {
    if remote.is_none() && foreign(node, headers) {
        return Err(Denied(StatusCode::FORBIDDEN, "a page of another origin".into()));
    }
    let held = match (credential(headers), remote) {
        (Some(credential), _) => node.access.held(&credential),
        (None, Some(Remote(key))) => node.access.held_by_key(&key.to_string()),
        (None, None) => return Err(Denied(StatusCode::UNAUTHORIZED, "no session".into())),
    };
    match held {
        Ok(Some(session)) => Ok(session),
        Ok(None) => Err(Denied(StatusCode::UNAUTHORIZED, "no such session".into())),
        Err(error) => Err(Denied(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())),
    }
}

/// Whether a request of `headers` asks for the protocol's version among the subprotocols it offers.
fn offers_protocol(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::SEC_WEBSOCKET_PROTOCOL)
        .into_iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|offered| offered.split(','))
        .any(|offered| offered.trim() == SUBPROTOCOL)
}

fn unversioned() -> Response {
    let why = "the WebSocket asks for no protocol of this node's: octoii.1";
    Denied(StatusCode::BAD_REQUEST, why.into()).into_response()
}

/// The protocol's WebSocket on the loopback, upgraded, for the session the request holds.
async fn socket(State(node): State<Node>, headers: HeaderMap, upgrade: WebSocketUpgrade) -> Response {
    let session = match session(&node, &headers, None) {
        Ok(session) => session,
        Err(denied) => return denied.into_response(),
    };
    let upgrade = upgrade.protocols([SUBPROTOCOL]);
    if upgrade.selected_protocol().is_none() {
        return unversioned();
    }
    upgrade.on_upgrade(move |socket: WebSocket| carried(node, session, socket))
}

/// The protocol's WebSocket in a tunnel session, by extended CONNECT, for the session the dialling key holds: its
/// stream taken once the answer is sent.
async fn tunnel_socket(
    State(node): State<Node>,
    headers: HeaderMap,
    Extension(remote): Extension<Remote>,
    Extension(connect): Extension<OnConnect>,
) -> Response {
    let session = match session(&node, &headers, Some(&remote)) {
        Ok(session) => session,
        Err(denied) => return denied.into_response(),
    };
    if !offers_protocol(&headers) {
        return unversioned();
    }
    tokio::spawn(async move {
        let Some(stream) = connect.stream().await else { return };
        carried(node, session, octoii_tunnel::socket::server(stream).await).await;
    });
    ([(header::SEC_WEBSOCKET_PROTOCOL, SUBPROTOCOL)], StatusCode::OK).into_response()
}

/// A WebSocket's message, as the protocol reads and writes it, whichever WebSocket carries it.
trait Frame: Send + 'static {
    /// A message of what Siphon sends.
    fn of(outgoing: Outgoing) -> Self;
    /// What the message says: a call's text; that the socket closes; or nothing the protocol reads.
    fn read(self) -> Read;
}

/// What a message read says.
enum Read {
    Text(String),
    Closed,
    Nothing,
}

impl Frame for ws::Message {
    fn of(outgoing: Outgoing) -> Self {
        match outgoing {
            Outgoing::Text(text) => Self::Text(text.into()),
            Outgoing::Binary(bytes) => Self::Binary(bytes),
        }
    }

    fn read(self) -> Read {
        match self {
            Self::Text(text) => Read::Text(text.as_str().to_owned()),
            Self::Close(_) => Read::Closed,
            _ => Read::Nothing,
        }
    }
}

impl Frame for octoii_tunnel::Message {
    fn of(outgoing: Outgoing) -> Self {
        match outgoing {
            Outgoing::Text(text) => Self::text(text),
            Outgoing::Binary(bytes) => Self::binary(bytes),
        }
    }

    fn read(self) -> Read {
        match self {
            Self::Text(text) => Read::Text(text.as_str().to_owned()),
            Self::Close(_) => Read::Closed,
            _ => Read::Nothing,
        }
    }
}

/// Carries the protocol over `socket` for the client of `session`: what it sends taken as calls, what Siphon sends it
/// written as text and binary frames, till either side closes.
async fn carried<S, F, E>(node: Node, session: Shown, socket: S)
where
    S: Stream<Item = Result<F, E>> + Sink<F> + Send + 'static,
    F: Frame,
{
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
            if writing.send(F::of(message)).await.is_err() {
                return;
            }
        }
    });
    while let Some(Ok(message)) = reading.next().await {
        match message.read() {
            Read::Text(text) => {
                if calls.send(text).await.is_err() {
                    break;
                }
            }
            Read::Closed => break,
            Read::Nothing => {}
        }
    }
    drop(calls);
    let _ = serving.await;
    writer.abort();
}

/// A provider's sign-in come back: finished from the address the person's browser was sent back to — the attempt's own
/// state and proof what it is checked by, as a provider's redirect carries no cookie of the node's.
async fn signed_in(State(node): State<Node>, axum::extract::Path(provider): axum::extract::Path<String>, uri: axum::http::Uri) -> Response {
    let redirect = node.siphon.sign_in.callback.replace("{provider}", &provider);
    let redirect = match uri.query() {
        Some(query) => format!("{redirect}?{query}"),
        None => redirect,
    };
    let finished = siphon::providers::finish_sign_in(&node.siphon.models, &node.siphon.sign_ins, &provider, &redirect).await;
    let said = match finished {
        Ok(()) => "Signed in. You may close this page.".to_owned(),
        Err(error) => format!("Not signed in: {error}"),
    };
    axum::response::Html(format!(
        "<!doctype html><meta charset=\"utf-8\"><title>Octoii</title><p>{}</p>",
        html_escaped(&said)
    ))
    .into_response()
}

/// `text` with what HTML reads as markup escaped.
fn html_escaped(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// What a client pairs by: the credential of a link.
#[derive(Debug, Deserialize)]
struct Pairing {
    credential: String,
}

/// A client paired by a link's credential: a device in a tunnel session, its session held by its key; a browser on the
/// loopback, its session held in an `HttpOnly` cookie its page cannot read, sent to this node's own pages alone.
async fn pair(
    State(node): State<Node>,
    headers: HeaderMap,
    remote: Option<Extension<Remote>>,
    Json(pairing): Json<Pairing>,
) -> Result<Response, Denied> {
    if remote.is_none() && foreign(&node, &headers) {
        return Err(Denied(StatusCode::FORBIDDEN, "a page of another origin".into()));
    }
    let (holder, key) = match &remote {
        Some(Extension(Remote(key))) => (Holder::Key, Some(key.to_string())),
        None => (Holder::Cookie, None),
    };
    let made = match node.access.redeem(&pairing.credential, holder, key).await {
        Ok(made) => made,
        Err(access::Refused::NoSuchLink) => return Err(Denied(StatusCode::FORBIDDEN, "the link was used, or went".into())),
        Err(error) => return Err(Denied(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())),
    };
    let body = serde_json::json!({ "session": made.shown.id });
    if remote.is_some() {
        return Ok(Json(body).into_response());
    }
    let cookie = format!(
        "{COOKIE}={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
        made.credential,
        access::SESSION_LASTS.as_secs()
    );
    Ok(([(header::SET_COOKIE, cookie)], Json(body)).into_response())
}
