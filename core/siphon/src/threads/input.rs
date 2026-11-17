//! Ordered initial input, resolved into ordinary model content in the Thread's creation commit.
use beak::Snapshot;
use bytes::Bytes;
use octoii_ai::vocabulary::{ImageDetail, InputContent};
use serde::Serialize;
use sha2::{Digest, Sha256};
use siphon_runtime::Changes;

use super::{NewThread, StartRequest, files};

/// An already described content part or file bytes still to be accepted by the new Thread.
#[derive(Debug, Clone)]
pub enum InputPart {
    Content(InputContent),
    Attachment {
        bytes: Bytes,
        media_type: String,
        filename: Option<String>,
    },
}

impl From<InputContent> for InputPart {
    fn from(content: InputContent) -> Self {
        Self::Content(content)
    }
}

// Content retains its ordinary serialization; adding attachments does not change existing content-only claims.
#[derive(Serialize)]
#[serde(untagged)]
enum Identity<'a> {
    Content(&'a InputContent),
    Attachment {
        digest: [u8; 32],
        media_type: &'a str,
        filename: &'a Option<String>,
    },
}

pub(super) fn fingerprint(new: &NewThread, parts: &[InputPart], request: &StartRequest) -> serde_json::Result<[u8; 32]> {
    let identities: Vec<_> = parts
        .iter()
        .map(|part| match part {
            InputPart::Content(content) => Identity::Content(content),
            InputPart::Attachment {
                bytes,
                media_type,
                filename,
            } => Identity::Attachment {
                digest: Sha256::digest(bytes).into(),
                media_type,
                filename,
            },
        })
        .collect();
    Ok(Sha256::digest(serde_json::to_vec(&(new, identities, request))?).into())
}

pub(super) fn keep(
    snapshot: &Snapshot,
    changes: &mut Changes,
    thread: &str,
    parts: Vec<InputPart>,
) -> Result<Vec<InputContent>, files::Refused> {
    parts
        .into_iter()
        .map(|part| match part {
            InputPart::Content(content) => Ok(content),
            InputPart::Attachment {
                bytes,
                media_type,
                filename,
            } => {
                let file = files::keep_given(snapshot, changes, thread, &bytes, &media_type, filename.as_deref())?;
                Ok(if file.media_type.starts_with("image/") {
                    InputContent::InputImage {
                        file,
                        detail: ImageDetail::Auto,
                    }
                } else {
                    InputContent::InputFile { file }
                })
            }
        })
        .collect()
}
