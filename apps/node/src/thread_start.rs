//! Bounded multipart input. Authentication precedes body parsing; Git validates tasks and Siphon commits them.
use std::collections::HashMap;

use axum::extract::{Extension, FromRequest, Multipart, Request, State};
use axum::http::{HeaderMap, StatusCode};
use bytes::{Bytes, BytesMut};
use octoii_ai::vocabulary::InputContent;
use octoii_tunnel::Remote;
use serde::Deserialize;
use siphon::protocol::Scope;
use siphon::threads::{Config, InputPart};
use siphon_git::protocol::CreateWorktree;

use crate::http::{Denied, Node, session};

pub(crate) const BODY_MAX: usize = 64 << 20;
const MANIFEST_MAX: usize = 1 << 20;
const PARTS_MAX: usize = 128;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    key: String,
    config: Config,
    worktree: CreateWorktree,
    parts: Vec<Part>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Part {
    Content {
        content: InputContent,
    },
    Attachment {
        index: usize,
        media_type: String,
        filename: Option<String>,
    },
}

fn invalid(error: impl std::fmt::Display) -> Denied {
    Denied(StatusCode::BAD_REQUEST, error.to_string())
}

fn multipart_error(error: &axum::extract::multipart::MultipartError) -> Denied {
    Denied(error.status(), error.to_string())
}

async fn bytes(mut field: axum::extract::multipart::Field<'_>, limit: usize) -> Result<Bytes, Denied> {
    let mut bytes = BytesMut::new();
    while let Some(chunk) = field.chunk().await.map_err(|error| multipart_error(&error))? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err(Denied(StatusCode::PAYLOAD_TOO_LARGE, "multipart field is too large".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes.freeze())
}

async fn read(mut multipart: Multipart) -> Result<(Manifest, HashMap<usize, Bytes>), Denied> {
    let field = multipart
        .next_field()
        .await
        .map_err(|error| multipart_error(&error))?
        .ok_or_else(|| invalid("missing manifest"))?;
    if field.name() != Some("manifest") {
        return Err(invalid("manifest must be the first field"));
    }
    let manifest: Manifest = serde_json::from_slice(&bytes(field, MANIFEST_MAX).await?).map_err(invalid)?;
    if manifest.parts.is_empty() || manifest.parts.len() > PARTS_MAX {
        return Err(invalid("a message requires between 1 and 128 parts"));
    }
    let mut expected = std::collections::HashSet::new();
    for part in &manifest.parts {
        if let Part::Attachment { index, .. } = part
            && !expected.insert(*index)
        {
            return Err(invalid("an attachment index may be referenced only once"));
        }
    }
    let mut files = HashMap::new();
    while let Some(field) = multipart.next_field().await.map_err(|error| multipart_error(&error))? {
        let name = field.name().ok_or_else(|| invalid("unnamed attachment"))?;
        let index: usize = name
            .strip_prefix("file-")
            .ok_or_else(|| invalid("unexpected multipart field"))?
            .parse()
            .map_err(invalid)?;
        if name != format!("file-{index}") || !expected.remove(&index) {
            return Err(invalid("unexpected or duplicate attachment"));
        }
        files.insert(index, bytes(field, siphon::threads::files::BYTES_MAX).await?);
    }
    if !expected.is_empty() {
        return Err(invalid("missing attachment"));
    }
    Ok((manifest, files))
}

pub(crate) async fn accepted(
    State(node): State<Node>,
    headers: HeaderMap,
    remote: Option<Extension<Remote>>,
    request: Request,
) -> Result<axum::Json<siphon_git::start::ThreadStart>, Denied> {
    let session = session(&node, &headers, remote.as_ref().map(|Extension(remote)| remote))?;
    if !session.scopes.contains(&Scope::GitOperate) || !session.scopes.contains(&Scope::ThreadsOperate) {
        return Err(Denied(
            StatusCode::FORBIDDEN,
            "Git and Thread operation permissions are required".into(),
        ));
    }
    let volume = node.siphon.volume();
    let internal = |error: String| Denied(StatusCode::INTERNAL_SERVER_ERROR, error);
    let settings = siphon::settings::environment(&volume.snapshot().map_err(|error| internal(error.to_string()))?)
        .map_err(|error| internal(error.to_string()))?;
    if settings.disabled_extensions.iter().any(|name| name == "git")
        || !node
            .siphon
            .sets()
            .iter()
            .flat_map(|set| &set.methods)
            .any(|(name, _)| *name == "git.start")
    {
        return Err(Denied(StatusCode::FORBIDDEN, "Git extension is unavailable".into()));
    }
    let multipart = Multipart::from_request(request, &node).await.map_err(invalid)?;
    let (manifest, mut files) = read(multipart).await?;
    let parts = manifest
        .parts
        .into_iter()
        .map(|part| match part {
            Part::Content { content } => Ok(InputPart::Content(content)),
            Part::Attachment {
                index,
                media_type,
                filename,
            } => Ok(InputPart::Attachment {
                bytes: files.remove(&index).ok_or_else(|| invalid("missing attachment"))?,
                media_type,
                filename,
            }),
        })
        .collect::<Result<Vec<_>, Denied>>()?;
    let accepted = siphon_git::start::accept(&volume, &manifest.key, manifest.config, parts, manifest.worktree)
        .await
        .map_err(|error| Denied(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
    Ok(axum::Json(accepted))
}
