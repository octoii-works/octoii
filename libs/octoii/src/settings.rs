//! The settings a person sets. The environment's — the mode a thread starts in, the models they starred, the model
//! that judges calls in auto, how long a thread's history is kept, the extensions they switched off — are annotations of the directory `siphon.settings`,
//! each as JSON: every client of the environment shares them, and Octoii reads them. Each device's are a document of
//! its client's own words, `siphon.settings/devices/<session>.json`, which Octoii reads none of, changed as a JSON merge
//! patch (RFC 7396) changes a document. Each change is one commit, so whoever follows the volume hears of it.

use crate::agent::{CORE, gather::BASE};
use crate::threads::Mode;
use bytes::Bytes;
use octoii_siphon_lib::{Batch, Changes, Named, RuntimeHandle};
use octoii_siphon_lib::{Directory, Snapshot};
use serde::{Deserialize, Serialize};

/// The directory the settings are kept in.
pub const DIRECTORY: &str = "siphon.settings";

/// The most bytes a device's settings hold.
pub const DEVICE_BYTES_MAX: usize = 64 * 1024;

/// How many days a thread's history is kept where the person set none.
pub const HISTORY_RETENTION_DAYS: u32 = 30;

const THREAD_MODE: &str = "siphon.thread_mode";
const FAVORITE_MODELS: &str = "siphon.favorite_models";
const REVIEW_MODEL: &str = "siphon.review_model";
const HISTORY_RETENTION: &str = "siphon.history_retention_days";
const DISABLED_EXTENSIONS: &str = "siphon.disabled_extensions";

/// The environment's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct Environment {
    /// The mode a thread starts in where it is made with none.
    pub thread_mode: Mode,
    /// The models the person starred, by their references.
    pub favorite_models: Vec<String>,
    /// The model that judges calls in auto; with none, no call goes ahead in auto without the person.
    pub review_model: Option<String>,
    /// How many days a thread keeps what came before its window; forever, where none.
    pub history_retention_days: Option<u32>,
    /// The extensions switched off, by their names: what they tell a thread's model is told no more.
    pub disabled_extensions: Vec<String>,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            thread_mode: Mode::default(),
            favorite_models: Vec::new(),
            review_model: None,
            history_retention_days: Some(HISTORY_RETENTION_DAYS),
            disabled_extensions: Vec::new(),
        }
    }
}

/// A change of the environment's settings: each given in place of the one it had, each left out kept;
/// `Some(None)` lets an optional one go.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct EnvironmentChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_mode: Option<Mode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorite_models: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "::serde_with::rust::double_option")]
    #[ts(as = "_", optional)]
    pub review_model: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "::serde_with::rust::double_option")]
    #[ts(as = "_", optional)]
    pub history_retention_days: Option<Option<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_extensions: Option<Vec<String>>,
}

/// A change of the settings: the environment's, and the device's, as a merge patch of its document.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
pub struct SettingsChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<EnvironmentChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<serde_json::Value>,
}

/// Why reading or writing settings failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the base extension cannot be disabled")]
    RequiredExtension,
    #[error("a device's settings are an object")]
    NotAnObject,
    #[error("a device's settings hold at most {DEVICE_BYTES_MAX} bytes")]
    TooLarge,
    #[error("a session is named by letters, digits, '.', '_' and '-'")]
    InvalidSession,
    #[error("the change changes nothing")]
    Nothing,
    #[error(transparent)]
    Beak(#[from] octoii_siphon_lib::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

fn annotation<T: serde::de::DeserializeOwned>(directory: &Directory, name: &str) -> serde_json::Result<Option<T>> {
    directory
        .annotation(name.as_bytes())
        .map(|value| serde_json::from_slice(value))
        .transpose()
}

/// The environment's settings: the defaults where none was set.
///
/// # Errors
/// The volume's failure, a malformed annotation, or a list that disables the base extension.
pub fn environment(snapshot: &Snapshot) -> Result<Environment, Error> {
    let defaults = Environment::default();
    let Some(directory) = snapshot.stat(DIRECTORY.as_bytes())?.and_then(|entry| entry.directory) else {
        return Ok(defaults);
    };
    let disabled_extensions: Vec<String> = annotation(&directory, DISABLED_EXTENSIONS)?.unwrap_or_default();
    validate_disabled_extensions(&disabled_extensions)?;
    Ok(Environment {
        thread_mode: annotation(&directory, THREAD_MODE)?.unwrap_or(defaults.thread_mode),
        favorite_models: annotation(&directory, FAVORITE_MODELS)?.unwrap_or_default(),
        review_model: annotation(&directory, REVIEW_MODEL)?.flatten(),
        history_retention_days: annotation(&directory, HISTORY_RETENTION)?.unwrap_or(defaults.history_retention_days),
        disabled_extensions,
    })
}

/// Whether what the extension `extension` brought is switched on, as the environment's settings in `snapshot` say:
/// `core` and `base` are required.
#[must_use]
pub(crate) fn switched_on(snapshot: &Snapshot, extension: &str) -> bool {
    matches!(extension, CORE | BASE)
        || environment(snapshot).is_ok_and(|settings| !settings.disabled_extensions.iter().any(|off| off == extension))
}

fn validate_disabled_extensions(extensions: &[String]) -> Result<(), Error> {
    if extensions.iter().any(|name| name == BASE) {
        return Err(Error::RequiredExtension);
    }
    Ok(())
}

/// The file of the settings of the device that holds `session`.
///
/// # Errors
/// `InvalidSession` for a session no file can be named by.
pub fn device_file(session: &str) -> Result<Bytes, Error> {
    let valid = !session.is_empty()
        && session.len() <= 128
        && session
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if !valid {
        return Err(Error::InvalidSession);
    }
    Ok(format!("{DIRECTORY}/devices/{session}.json").into())
}

/// The settings of the device that holds `session`: an object of the names its client set, empty where it set none.
///
/// # Errors
/// `InvalidSession`; the volume's failure; a document that is not JSON.
pub fn device(snapshot: &Snapshot, session: &str) -> Result<serde_json::Value, Error> {
    match snapshot.read_range(&device_file(session)?, 0, u64::MAX)? {
        Some(bytes) => Ok(serde_json::from_slice(&bytes)?),
        None => Ok(serde_json::Value::Object(serde_json::Map::new())),
    }
}

fn json<T: Serialize>(value: &T) -> Result<Bytes, Error> {
    Ok(serde_json::to_vec(value)?.into())
}

/// The annotations that make `change`.
fn annotations(change: &EnvironmentChange) -> Result<Vec<Named>, Error> {
    let named = |name: &'static str, value: Bytes| (Bytes::from_static(name.as_bytes()), Some(value));
    let mut out = Vec::new();
    if let Some(mode) = &change.thread_mode {
        out.push(named(THREAD_MODE, json(mode)?));
    }
    if let Some(models) = &change.favorite_models {
        out.push(named(FAVORITE_MODELS, json(models)?));
    }
    if let Some(model) = &change.review_model {
        out.push((Bytes::from_static(REVIEW_MODEL.as_bytes()), model.as_ref().map(json).transpose()?));
    }
    if let Some(days) = &change.history_retention_days {
        out.push(named(HISTORY_RETENTION, json(days)?));
    }
    if let Some(extensions) = &change.disabled_extensions {
        validate_disabled_extensions(extensions)?;
        out.push(named(DISABLED_EXTENSIONS, json(extensions)?));
    }
    Ok(out)
}

/// Sets what `change` sets — the environment's settings, and the settings of the device that holds `session` — in one
/// commit.
///
/// # Errors
/// A change that disables `base`, invalid device settings, an empty change, or the commit's failure.
pub async fn set(runtime: &RuntimeHandle, session: &str, change: &SettingsChange) -> Result<(), Error> {
    let mut changes = Changes::new();
    changes.mkdir(Bytes::from_static(DIRECTORY.as_bytes()));
    let mut sets_any = false;
    if let Some(environment) = &change.environment {
        let annotations = annotations(environment)?;
        sets_any |= !annotations.is_empty();
        changes.meta(Bytes::from_static(DIRECTORY.as_bytes()), Vec::new(), annotations);
    }
    if let Some(patch) = &change.device {
        let mut document = device(&*runtime.snapshot()?, session)?;
        json_patch::merge(&mut document, patch);
        if !document.is_object() {
            return Err(Error::NotAnObject);
        }
        let bytes = serde_json::to_vec(&document)?;
        if bytes.len() > DEVICE_BYTES_MAX {
            return Err(Error::TooLarge);
        }
        changes
            .mkdir(Bytes::from(format!("{DIRECTORY}/devices")))
            .write(device_file(session)?, bytes);
        sets_any = true;
    }
    if !sets_any {
        return Err(Error::Nothing);
    }
    runtime.commit(changes).await?;
    Ok(())
}

/// Lets go of the settings of the device that held `session`, with `changes`, where it set any.
///
/// # Errors
/// `InvalidSession`; the volume's failure.
pub fn forget(snapshot: &Snapshot, changes: &mut Batch<'_>, session: &str) -> Result<(), Error> {
    let file = device_file(session)?;
    if snapshot.stat(&file)?.is_some() {
        changes.delete(file);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn disabling_base_is_rejected_on_write_and_read() {
        let runtime = octoii_siphon_lib::Runtime::memory().unwrap();
        let disable = |extension: &str| SettingsChange {
            environment: Some(EnvironmentChange {
                disabled_extensions: Some(vec![extension.into()]),
                ..EnvironmentChange::default()
            }),
            device: None,
        };
        assert!(matches!(set(&runtime, "test", &disable(BASE)).await, Err(Error::RequiredExtension)));
        set(&runtime, "test", &disable("git")).await.unwrap();
        let snapshot = runtime.snapshot().unwrap();
        assert_eq!(environment(&snapshot).unwrap().disabled_extensions, ["git"]);
        assert!(switched_on(&snapshot, BASE));
        assert!(!switched_on(&snapshot, "git"));

        let mut changes = Changes::new();
        changes.meta(
            Bytes::from_static(DIRECTORY.as_bytes()),
            Vec::new(),
            vec![(
                Bytes::from_static(DISABLED_EXTENSIONS.as_bytes()),
                Some(Bytes::from_static(br#"["base","git"]"#)),
            )],
        );
        runtime.commit(changes).await.unwrap();
        assert!(matches!(environment(&runtime.snapshot().unwrap()), Err(Error::RequiredExtension)));
        runtime.close().unwrap();
    }
}
