//! The files a thread's messages give their model, and the pictures its calls show the person, as the node's HTTP
//! carries them — a client's messages are JSON on its WebSocket, a file's bytes are not: a file sent in a POST to
//! `/threads/<thread>/files`, kept in the thread by its content — an image as every model reads one — and answered with
//! what names it, its reference; a kept file's bytes given back at `/threads/<thread>/files/<sha256>` — or, where the
//! query asks with `width_max` and `height_max`, an image fitted to that box, its shape kept and never enlarged, as a
//! picture of it is drawn small — each kept by a browser for good, as what a reference names never changes. Beside
//! them, an image or a PDF of the machine's at `/machine?path=<path>`, as the person browses the machine's files, its
//! bytes read anew each time.

use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use octoii_tunnel::Remote;
use serde::Deserialize;
use siphon::protocol::Scope;
use siphon::threads::files;
use siphon_runtime::Changes;

use crate::http::{Denied, Node, session};

/// What a file sent is named, where it has a name.
#[derive(Debug, Deserialize)]
pub(crate) struct Named {
    name: Option<String>,
}

/// The box a picture of an image is fitted to, where one is asked for.
#[derive(Debug, Deserialize)]
pub(crate) struct Picture {
    width_max: Option<u32>,
    height_max: Option<u32>,
}

/// The session a request holds, where it may do what `scope` opens, and the thread `thread` is there.
fn allowed(node: &Node, headers: &HeaderMap, remote: Option<&Remote>, scope: Scope, thread: &str) -> Result<(), Denied> {
    let session = session(node, headers, remote)?;
    if !session.scopes.contains(&scope) {
        return Err(Denied(StatusCode::FORBIDDEN, "the session may not do this".into()));
    }
    node.siphon
        .threads
        .listed(thread)
        .map_err(|_| Denied(StatusCode::NOT_FOUND, "no such thread".into()))?;
    Ok(())
}

/// A file sent to the thread `thread`, kept as its conversation keeps it: what names it.
pub(crate) async fn sent(
    State(node): State<Node>,
    Path(thread): Path<String>,
    Query(named): Query<Named>,
    headers: HeaderMap,
    remote: Option<Extension<Remote>>,
    body: Bytes,
) -> Result<Response, Denied> {
    allowed(
        &node,
        &headers,
        remote.as_ref().map(|Extension(remote)| remote),
        Scope::ThreadsOperate,
        &thread,
    )?;
    let media_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| Denied(StatusCode::BAD_REQUEST, "a file says what it is by its Content-Type".into()))?;
    let runtime = &node.siphon.runtime;
    let snapshot = runtime
        .snapshot()
        .map_err(|error| Denied(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let mut changes = Changes::new();
    let file = files::keep_given(&snapshot, &mut changes, &thread, &body, media_type, named.name.as_deref())
        .map_err(|error| Denied(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
    if !changes.is_empty() {
        runtime
            .commit(changes)
            .await
            .map_err(|error| Denied(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    }
    Ok(axum::Json(file).into_response())
}

/// The bytes of the file the thread `thread` keeps by the hash `hash` — fitted to the box asked for, where it is an
/// image and one is — kept by a browser for good.
pub(crate) async fn kept(
    State(node): State<Node>,
    Path((thread, hash)): Path<(String, String)>,
    Query(asked): Query<Picture>,
    headers: HeaderMap,
    remote: Option<Extension<Remote>>,
) -> Result<Response, Denied> {
    allowed(
        &node,
        &headers,
        remote.as_ref().map(|Extension(remote)| remote),
        Scope::ThreadsRead,
        &thread,
    )?;
    let snapshot = node
        .siphon
        .runtime
        .snapshot()
        .map_err(|error| Denied(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let bytes = files::read(&snapshot, &thread, &format!("sha256:{hash}"))
        .map_err(|error| Denied(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .ok_or_else(|| Denied(StatusCode::NOT_FOUND, "no such file".into()))?;
    let media_type = siphon::machine::media_type(&bytes).unwrap_or("application/octet-stream");
    let picture = asked.width_max.zip(asked.height_max);
    // What a reference names never changes.
    pictured(media_type, &bytes, picture, "private, max-age=31536000, immutable")
}

/// `bytes` of `media_type` as a response — an image fitted to `picture`, its widest and its tallest, where one is
/// asked for — kept by a browser as `caching` says.
fn pictured(media_type: &'static str, bytes: &Bytes, picture: Option<(u32, u32)>, caching: &'static str) -> Result<Response, Denied> {
    let (mut media_type, mut body) = (media_type, bytes.clone());
    if let (Some((width_max, height_max)), true) = (picture, media_type.starts_with("image/")) {
        let bounds = siphon_image::Bounds {
            width_max,
            height_max,
            pixels_max: u64::from(width_max) * u64::from(height_max),
            bytes_max: files::BYTES_MAX,
            formats: siphon_image::UNIVERSAL_FORMATS,
        };
        let fitted = siphon_image::fit(bytes, &bounds).map_err(|error| Denied(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
        media_type = fitted.format.media_type();
        body = Bytes::from(fitted.bytes.into_owned());
    }
    let headers = [(header::CONTENT_TYPE, media_type), (header::CACHE_CONTROL, caching)];
    Ok((headers, body).into_response())
}

/// Where a file of the machine's is: as the person names it, whole.
#[derive(Debug, Deserialize)]
pub(crate) struct OnMachine {
    path: String,
    width_max: Option<u32>,
    height_max: Option<u32>,
}

/// The bytes of an image or a PDF of the machine's, as the person reads it — fitted to the box asked for, where it is
/// an image and one is — to a session that may read files.
pub(crate) async fn machined(
    State(node): State<Node>,
    Query(asked): Query<OnMachine>,
    headers: HeaderMap,
    remote: Option<Extension<Remote>>,
) -> Result<Response, Denied> {
    let session = session(&node, &headers, remote.as_ref().map(|Extension(remote)| remote))?;
    if !session.scopes.contains(&Scope::FilesRead) {
        return Err(Denied(StatusCode::FORBIDDEN, "the session may not read files".into()));
    }
    let machine = node
        .siphon
        .person_machine()
        .map_err(|error| Denied(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let options = siphon::machine::ReadOptions {
        media_bytes_max: files::BYTES_MAX as u64,
        ..siphon::machine::ReadOptions::default()
    };
    let read = machine.read(&machine.path(&asked.path), &options);
    let (media_type, bytes) = match read {
        Ok(siphon::machine::Read::Media { media_type, bytes, .. }) => (media_type, bytes),
        Ok(siphon::machine::Read::TooLarge { limit, .. }) => {
            return Err(Denied(StatusCode::PAYLOAD_TOO_LARGE, format!("past {limit} bytes")));
        }
        Ok(_) => {
            return Err(Denied(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "no image or PDF; read it as text".into(),
            ));
        }
        Err(siphon::machine::Error::NotFound) => return Err(Denied(StatusCode::NOT_FOUND, "no such file".into())),
        Err(error) => return Err(Denied(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())),
    };
    pictured(media_type, &bytes, asked.width_max.zip(asked.height_max), "private, no-cache")
}
