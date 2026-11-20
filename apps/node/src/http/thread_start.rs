//! Accept a thread and its initial input from a bounded multipart request.
use std::collections::{HashMap, HashSet};
use std::path::Path;

use axum::extract::{Extension, FromRequest, Multipart, Request, State};
use axum::http::{HeaderMap, StatusCode};
use bytes::{Bytes, BytesMut};
use octoii::access::AccessEntry;
use octoii::machine::{Base, Host};
use octoii::protocol::Scope;
use octoii::threads::{Config, InputPart, NewThread, StartDirectory, StartWorkspace};
use octoii::tools::Place;
use octoii_ai::vocabulary::InputContent;
use octoii_tunnel::Remote;
use serde::{Deserialize, Serialize};
use siphon_git::start::WorktreeIntent;

use super::{Denied, Node, session};

pub(crate) const BODY_MAX: usize = 64 << 20;
const MANIFEST_MAX: usize = 1 << 20;
const PARTS_MAX: usize = 128;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    key: String,
    config: Config,
    workspace: Workspace,
    parts: Vec<Part>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Workspace {
    Current,
    Isolated { path: String, base: Option<String> },
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

#[derive(Serialize)]
pub(crate) struct Accepted {
    thread: String,
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

async fn read_manifest(multipart: &mut Multipart) -> Result<Manifest, Denied> {
    let field = multipart
        .next_field()
        .await
        .map_err(|error| multipart_error(&error))?
        .ok_or_else(|| invalid("missing manifest"))?;
    if field.name() != Some("manifest") {
        return Err(invalid("manifest must be the first field"));
    }
    serde_json::from_slice(&bytes(field, MANIFEST_MAX).await?).map_err(invalid)
}

async fn read_parts(mut multipart: Multipart, parts: Vec<Part>) -> Result<Vec<InputPart>, Denied> {
    if parts.is_empty() || parts.len() > PARTS_MAX {
        return Err(invalid("a message requires between 1 and 128 parts"));
    }
    let mut expected = HashSet::new();
    for part in &parts {
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
        files.insert(index, bytes(field, octoii::threads::files::BYTES_MAX).await?);
    }
    if !expected.is_empty() {
        return Err(invalid("missing attachment"));
    }
    parts
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
        .collect()
}

pub(crate) async fn accepted(
    State(node): State<Node>,
    headers: HeaderMap,
    remote: Option<Extension<Remote>>,
    request: Request,
) -> Result<axum::Json<Accepted>, Denied> {
    let session = session(&node, &headers, remote.as_ref().map(|Extension(remote)| remote))?;
    if !session.scopes.contains(&Scope::ThreadsOperate) {
        return Err(Denied(StatusCode::FORBIDDEN, "Thread operation permission is required".into()));
    }
    let mut multipart = Multipart::from_request(request, &node).await.map_err(invalid)?;
    let Manifest {
        key,
        config,
        workspace,
        parts,
    } = read_manifest(&mut multipart).await?;
    if matches!(&workspace, Workspace::Isolated { .. }) {
        authorize_isolated(&node, &session)?;
    }
    let content = read_parts(multipart, parts).await?;
    let volume = node.siphon.volume();
    let thread = match workspace {
        Workspace::Current => {
            let directory = current(&node.siphon.machine, config.workspace.as_deref())?;
            let new = NewThread {
                actor: "person".into(),
                config,
                ..NewThread::default()
            };
            volume
                .start_thread(&session.id, &key, &new, content, StartDirectory::Current(directory))
                .await
        }
        Workspace::Isolated { path, base } => {
            siphon_git::start::accept(
                &node.siphon,
                || node.home.worktrees(),
                &session.id,
                &key,
                config,
                content,
                WorktreeIntent { path, base },
            )
            .await
        }
    }
    .map_err(|error| Denied(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
    Ok(axum::Json(Accepted { thread }))
}

fn current(host: &Host, given: Option<&str>) -> Result<StartWorkspace, Denied> {
    let base = Base {
        workspace: host.workspace_of(None),
        home: host.home.clone(),
    };
    let workspace = std::path::absolute(base.resolve(given.unwrap_or(".")))
        .map_err(invalid)?
        .into_os_string()
        .into_string()
        .map_err(|_| invalid("workspace must be UTF-8"))?;
    let project = given.map(|_| Place::of(Path::new(&workspace)).project.to_string_lossy().into_owned());
    Ok(StartWorkspace { workspace, project })
}

fn authorize_isolated(node: &Node, session: &AccessEntry) -> Result<(), Denied> {
    if !session.scopes.contains(&Scope::GitOperate) {
        return Err(Denied(
            StatusCode::FORBIDDEN,
            "Git operation permission is required for an isolated workspace".into(),
        ));
    }
    let available = node
        .siphon
        .can_prepare_workspace("git")
        .map_err(|error| Denied(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    if !available {
        return Err(Denied(StatusCode::FORBIDDEN, "Git extension is unavailable".into()));
    }
    Ok(())
}
