//! What a message or a call's output holds: input content from the caller — text, and files the host keeps by their
//! reference, their bytes read as the request is sent — and output content from the model.

use serde::{Deserialize, Serialize};

/// A file the host keeps, by the reference its content gives it, `sha256:` and the hash: what it is, and its name
/// where it has one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct FileRef {
    #[serde(rename = "ref")]
    pub reference: String,
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

/// How closely a model looks at an image.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum ImageDetail {
    #[default]
    Auto,
    High,
    Low,
}

/// What user, developer and system messages and calls' outputs hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputContent {
    InputText {
        text: String,
    },
    InputImage {
        file: FileRef,
        #[serde(default)]
        detail: ImageDetail,
    },
    InputFile {
        file: FileRef,
    },
}

impl InputContent {
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::InputText { text: text.into() }
    }
}

/// A web page an answer cites, by the characters of the text that cite it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct UrlCitation {
    pub url: String,
    pub title: String,
    pub start_index: u32,
    pub end_index: u32,
}

/// What an assistant message holds: text the model wrote, with the pages it cites, or its refusal in its words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputContent {
    OutputText {
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        annotations: Vec<UrlCitation>,
    },
    Refusal {
        refusal: String,
    },
}
