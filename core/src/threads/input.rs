//! Ordered initial input, resolved into ordinary model content in the Thread's creation commit.
use bytes::Bytes;
use octoii_ai::vocabulary::{ImageDetail, InputContent};
use octoii_siphon_lib::Batch;
use octoii_siphon_lib::Snapshot;
use serde::Serialize;
use sha2::{Digest, Sha256};

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

/// What the startup claim hashes for each input part, including its variant.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
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
    changes: &mut Batch<'_>,
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
