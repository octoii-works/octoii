//! The providers models are reached through: the host's, then the accounts a person added — each another instance of
//! one of the host's, a second sign-in, another login, a key of another organization — in the order they were added.
//! Each provider that has more than the host gave it is a directory `siphon.provider:<id>` at the volume's root: an
//! account labelled `siphon.driver` with the host's provider it is another of, and annotated with the name the person
//! gave it; any provider's quota used up — its plan's limit, as it told of a request it refused — a file `limit` in it
//! that goes when the quota is given again, so the limit stands as long as it does.

use beak::{Range, Snapshot};
use bytes::Bytes;
use octoii_ai::driver::Connection;
use octoii_ai::models::Provider;
use serde::{Deserialize, Serialize};
use siphon_runtime::{Changes, Runtime};
use siphon_threads::idempotency::{self, Done};

/// The kind of a provider's directory.
pub const KIND: &str = "siphon.provider";
/// The label of the host's provider an account is another of, by its id; empty for the host's own.
pub const DRIVER_LABEL: &str = "siphon.driver";
/// The annotation of the name the person gave an account.
pub const NAME_ANNOTATION: &str = "siphon.name";
/// The most bytes an account's name holds.
pub const NAME_BYTES_MAX: usize = 64;

/// An account, as a list shows it: its id, the host's provider it is another of, and the name it was given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub provider: String,
    pub name: String,
}

/// A provider's quota used up: when it is given again, in milliseconds since the epoch, and how long the window it is
/// counted over lasts, in minutes, where the provider said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsedUp {
    pub reset_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_minutes: Option<u32>,
}

/// Why an account was not changed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the host gives no provider {0}")]
    UnknownProvider(String),
    #[error("a server of the person's takes no accounts: another would reach the same server")]
    TakesNoAccounts,
    #[error("an account is named by 1 to {NAME_BYTES_MAX} bytes of text")]
    InvalidName,
    #[error("no such account")]
    NoSuchAccount,
    #[error(transparent)]
    Idempotency(#[from] idempotency::Error),
    #[error(transparent)]
    Beak(#[from] beak::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// The directory of the provider `id`; none for an id no directory can be named by.
fn path(id: &str) -> Option<Bytes> {
    let valid = !id.is_empty() && !id.bytes().any(|byte| matches!(byte, b'/' | b':' | 0));
    valid.then(|| beak::layout::child(b"", KIND.as_bytes(), id.as_bytes()))
}

fn named(name: &str) -> Result<&str, Error> {
    let name = name.trim();
    if name.is_empty() || name.len() > NAME_BYTES_MAX {
        return Err(Error::InvalidName);
    }
    Ok(name)
}

/// The accounts, in the order they were added.
///
/// # Errors
/// The volume's failure.
pub fn accounts(snapshot: &Snapshot) -> beak::Result<Vec<Account>> {
    let prefix = format!("{KIND}:");
    let range = Range {
        prefix: prefix.as_bytes(),
        limit: usize::MAX,
        ..Range::default()
    };
    let text = |value: Option<&Bytes>| value.map(|value| String::from_utf8_lossy(value).into_owned()).unwrap_or_default();
    let mut out = Vec::new();
    for entry in snapshot.list(b"", &range)?.unwrap_or_default() {
        let Some(directory) = &entry.directory else { continue };
        let provider = text(directory.label(DRIVER_LABEL.as_bytes()));
        if provider.is_empty() {
            continue;
        }
        out.push(Account {
            id: String::from_utf8_lossy(&entry.name[prefix.len()..]).into_owned(),
            provider,
            name: text(directory.annotation(NAME_ANNOTATION.as_bytes())),
        });
    }
    // Ids are made later each time, so the order of their names is the order they were added.
    Ok(out)
}

/// The host's providers, then an instance of each account's: one of a provider the host gives no more rests till it
/// gives it again.
#[must_use]
pub fn providers(host: &[Provider], accounts: &[Account]) -> Vec<Provider> {
    let mut out = host.to_vec();
    out.extend(accounts.iter().filter_map(|account| {
        let base = host.iter().find(|provider| provider.id == account.provider)?;
        Some(Provider {
            id: account.id.clone(),
            name: account.name.clone(),
            ..base.clone()
        })
    }));
    out
}

/// Adds an account of the host's provider `provider`, named `name`: its id. Added again under the same `key` within a
/// day, it is the account added then.
///
/// # Errors
/// `UnknownProvider`, `TakesNoAccounts`, `InvalidName`; as the key says; the commit's failure.
pub async fn add(runtime: &Runtime, host: &[Provider], provider: &str, name: &str, key: &str) -> Result<String, Error> {
    let base = host
        .iter()
        .find(|each| each.id == provider)
        .ok_or_else(|| Error::UnknownProvider(provider.to_owned()))?;
    if base.driver.connection() == Connection::Server {
        return Err(Error::TakesNoAccounts);
    }
    let name = named(name)?;
    if let Some(done) = idempotency::done_before(&runtime.snapshot()?, key, "accounts.add", None)? {
        return Ok(done.result);
    }
    let id = runtime.new_id()?;
    let account = path(&id).ok_or(Error::NoSuchAccount)?;
    let mut changes = Changes::new();
    changes.mkdir(account.clone()).meta(
        account,
        vec![(
            Bytes::from_static(DRIVER_LABEL.as_bytes()),
            Some(Bytes::copy_from_slice(provider.as_bytes())),
        )],
        vec![(
            Bytes::from_static(NAME_ANNOTATION.as_bytes()),
            Some(Bytes::copy_from_slice(name.as_bytes())),
        )],
    );
    let done = Done {
        method: "accounts.add".into(),
        thread: None,
        result: id.clone(),
    };
    idempotency::record(&mut changes, key, &done, runtime.now_ms())?;
    runtime.commit(changes).await?;
    Ok(id)
}

/// The directory of the account `id`, where there is one.
fn account(snapshot: &Snapshot, id: &str) -> Result<Bytes, Error> {
    let path = path(id).ok_or(Error::NoSuchAccount)?;
    let added = snapshot.stat(&path)?.and_then(|entry| entry.directory).is_some_and(|directory| {
        directory
            .label(DRIVER_LABEL.as_bytes())
            .is_some_and(|provider| !provider.is_empty())
    });
    if !added {
        return Err(Error::NoSuchAccount);
    }
    Ok(path)
}

/// Gives the account `id` the name `name`.
///
/// # Errors
/// `NoSuchAccount`, `InvalidName`; the commit's failure.
pub async fn rename(runtime: &Runtime, id: &str, name: &str) -> Result<(), Error> {
    let name = named(name)?;
    let account = account(&runtime.snapshot()?, id)?;
    let mut changes = Changes::new();
    changes.meta(
        account,
        Vec::new(),
        vec![(
            Bytes::from_static(NAME_ANNOTATION.as_bytes()),
            Some(Bytes::copy_from_slice(name.as_bytes())),
        )],
    );
    runtime.commit(changes).await?;
    Ok(())
}

/// Removes the account `id`.
///
/// # Errors
/// `NoSuchAccount`; the commit's failure.
pub async fn remove(runtime: &Runtime, id: &str) -> Result<(), Error> {
    let account = account(&runtime.snapshot()?, id)?;
    let mut changes = Changes::new();
    changes.delete(account);
    runtime.commit(changes).await?;
    Ok(())
}

fn limit(provider: &str) -> Option<Bytes> {
    path(provider).map(|path| [&path[..], b"/limit"].concat().into())
}

/// Adds to `changes` that the quota of the provider `provider` is used up as `used_up` says, till it is given again —
/// the directory of one of the host's made where it has none, its label empty — a provider no directory can be named
/// by kept nothing of.
///
/// # Errors
/// The volume's failure; what cannot be written as JSON.
pub fn keep_used_up(snapshot: &Snapshot, changes: &mut Changes, provider: &str, used_up: UsedUp) -> Result<(), Error> {
    let (Some(directory), Some(file)) = (path(provider), limit(provider)) else {
        return Ok(());
    };
    if snapshot.stat(&directory)?.is_none() {
        changes.mkdir(directory.clone()).meta(
            directory,
            vec![(Bytes::from_static(DRIVER_LABEL.as_bytes()), Some(Bytes::new()))],
            Vec::new(),
        );
    }
    changes
        .write(file.clone(), serde_json::to_vec(&used_up)?)
        .expire(file, Some(used_up.reset_at_ms));
    Ok(())
}

/// The quota of the provider `provider`, where it is used up still.
///
/// # Errors
/// The volume's failure; a limit that is not what Siphon writes.
pub fn used_up(snapshot: &Snapshot, provider: &str) -> Result<Option<UsedUp>, Error> {
    let Some(file) = limit(provider) else { return Ok(None) };
    Ok(snapshot
        .read(&file, 0, u64::MAX)?
        .map(|bytes| serde_json::from_slice(&bytes))
        .transpose()?)
}

/// The provider of the model a reference names, by its id: what comes before its first `/`.
#[must_use]
pub fn provider_of(model: &str) -> &str {
    model.split_once('/').map_or(model, |(provider, _)| provider)
}
