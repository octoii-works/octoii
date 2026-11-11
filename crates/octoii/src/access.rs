//! The node's access: the sessions its clients hold and the links that pair new clients, each a directory of the
//! volume's — `octoii.session:<id>`, `octoii.link:<id>` — that holds the digest of its secret, never the secret, as an
//! SSH server keeps its authorized keys, and goes as it expires. A client presents its credential, `<id>.<secret>`: a
//! browser in its cookie, a program as a bearer token; a device in a tunnel session is known by the key its session
//! showed. A link's credential pairs one client, once, while it lasts: redeemed, it is gone in the commit that issues the
//! client a session granting what the link granted — so of two clients that redeem it at once, one alone is paired.

use std::time::Duration;

use base64::Engine as _;
use beak::{Range, Snapshot};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use siphon_protocol::Scope;
use siphon_runtime::{Changes, Runtime};

/// The kinds of the directories of sessions and of links.
const SESSION_KIND: &str = "octoii.session";
const LINK_KIND: &str = "octoii.link";
/// The annotation that holds what a session or a link is.
const KEPT: &str = "octoii.access";

/// How long a session lasts.
pub const SESSION_LASTS: Duration = Duration::from_hours(365 * 24);
/// How long a link lasts.
pub const LINK_LASTS: Duration = Duration::from_mins(5);

/// Every scope there is, as a session the person makes for themselves holds them.
pub const ALL: [Scope; 8] = [
    Scope::ThreadsRead,
    Scope::ThreadsOperate,
    Scope::FilesRead,
    Scope::TerminalsOperate,
    Scope::BrowserOperate,
    Scope::ProvidersManage,
    Scope::AccessRead,
    Scope::AccessWrite,
];

/// How a client holds its session: a browser's cookie, a program's bearer token, or the key its tunnel session shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Holder {
    Cookie,
    Bearer,
    Key,
}

/// What a session's or a link's directory holds: the digest of its secret, what it grants, its label, how its client
/// holds it and the key it shows where it is a device's, when it was made and when it goes, in ms since the epoch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Kept {
    digest: String,
    scopes: Vec<Scope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    holder: Option<Holder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key: Option<String>,
    made_at: i64,
    expires_at: i64,
}

/// A session or a link, as the access shows it: its id, what it grants, its label, how its client holds it — none for a
/// link — and when it was made and goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shown {
    pub id: String,
    pub scopes: Vec<Scope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holder: Option<Holder>,
    pub made_at: i64,
    pub expires_at: i64,
}

/// What was made, with the credential that holds it — shown this once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Made {
    pub shown: Shown,
    pub credential: String,
}

/// Why access was not given.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("the credential is no link's, or its link was used or went")]
    NoSuchLink,
    #[error("no such session or link")]
    NoSuch,
    #[error("the system gave no random bytes: {0}")]
    Random(String),
    #[error(transparent)]
    Beak(#[from] beak::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// The access, on Siphon's runtime.
#[derive(Debug, Clone)]
pub struct Access {
    runtime: Runtime,
}

/// The SHA-256 of `secret`, in hex.
fn digest(secret: &str) -> String {
    Sha256::digest(secret.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// Whether `a` and `b` are the same, compared in time that tells nothing of where they differ.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |differ, (a, b)| differ | (a ^ b)) == 0
}

/// A secret: 32 random bytes, in unpadded base64url.
fn secret() -> Result<String, Refused> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|error| Refused::Random(error.to_string()))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn path(kind: &str, id: &str) -> Bytes {
    beak::layout::child(b"", kind.as_bytes(), id.as_bytes())
}

/// What the directory at `path` keeps, where `snapshot` has it.
fn kept(snapshot: &Snapshot, path: &[u8]) -> Result<Option<Kept>, Refused> {
    let Some(directory) = snapshot.stat(path)?.and_then(|entry| entry.directory) else {
        return Ok(None);
    };
    let Some(value) = directory.annotation(KEPT.as_bytes()) else {
        return Ok(None);
    };
    Ok(Some(serde_json::from_slice(value)?))
}

fn shown(id: &str, kept: &Kept) -> Shown {
    Shown {
        id: id.to_owned(),
        scopes: kept.scopes.clone(),
        label: kept.label.clone(),
        holder: kept.holder,
        made_at: kept.made_at,
        expires_at: kept.expires_at,
    }
}

fn millis(lasts: Duration) -> i64 {
    i64::try_from(lasts.as_millis()).unwrap_or(i64::MAX)
}

impl Access {
    #[must_use]
    pub fn new(runtime: Runtime) -> Self {
        Self { runtime }
    }

    /// The directory of `kind` made with `changes`, keeping `kept`: its id, and its credential.
    fn made(&self, changes: &mut Changes, kind: &str, kept: &mut Kept) -> Result<(String, String), Refused> {
        let id = self.runtime.new_id()?;
        let secret = secret()?;
        kept.digest = digest(&secret);
        let path = path(kind, &id);
        let annotation = (Bytes::from_static(KEPT.as_bytes()), Some(Bytes::from(serde_json::to_vec(kept)?)));
        changes
            .mkdir(path.clone())
            .meta(path.clone(), Vec::new(), vec![annotation])
            .expire(path, Some(kept.expires_at));
        Ok((id.clone(), format!("{id}.{secret}")))
    }

    /// A link that pairs one client with `scopes`, labelled `label`, lasting `lasts`.
    ///
    /// # Errors
    /// The volume's failure.
    pub async fn link(&self, label: Option<String>, scopes: Vec<Scope>, lasts: Duration) -> Result<Made, Refused> {
        let now = self.runtime.now_ms();
        let mut kept = Kept {
            digest: String::new(),
            scopes,
            label,
            holder: None,
            key: None,
            made_at: now,
            expires_at: now + millis(lasts),
        };
        let mut changes = Changes::new();
        let (id, credential) = self.made(&mut changes, LINK_KIND, &mut kept)?;
        self.runtime.commit(changes).await?;
        Ok(Made {
            shown: shown(&id, &kept),
            credential,
        })
    }

    /// A session with `scopes`, labelled `label`, its client holding it as `holder`, by the key `key` where it is a
    /// device's — made with `changes`.
    fn session(
        &self,
        changes: &mut Changes,
        scopes: Vec<Scope>,
        label: Option<String>,
        holder: Holder,
        key: Option<String>,
    ) -> Result<Made, Refused> {
        let now = self.runtime.now_ms();
        let mut kept = Kept {
            digest: String::new(),
            scopes,
            label,
            holder: Some(holder),
            key,
            made_at: now,
            expires_at: now + millis(SESSION_LASTS),
        };
        let (id, credential) = self.made(changes, SESSION_KIND, &mut kept)?;
        Ok(Made {
            shown: shown(&id, &kept),
            credential,
        })
    }

    /// A session issued at once, as the person asks one for a program: its bearer token.
    ///
    /// # Errors
    /// The volume's failure.
    pub async fn issue(&self, label: Option<String>, scopes: Vec<Scope>) -> Result<Made, Refused> {
        let mut changes = Changes::new();
        let made = self.session(&mut changes, scopes, label, Holder::Bearer, None)?;
        self.runtime.commit(changes).await?;
        Ok(made)
    }

    /// The session the link of `credential` grants, its client holding it as `holder`, by the key `key` where it is a
    /// device's — the link gone in the same commit.
    ///
    /// # Errors
    /// `NoSuchLink` where the credential is no link's, or its link was used or went; the volume's failure.
    pub async fn redeem(&self, credential: &str, holder: Holder, key: Option<String>) -> Result<Made, Refused> {
        let (id, secret) = credential.split_once('.').ok_or(Refused::NoSuchLink)?;
        let link = path(LINK_KIND, id);
        let kept = kept(&self.runtime.snapshot()?, &link)?.ok_or(Refused::NoSuchLink)?;
        if !same(&kept.digest, &digest(secret)) || kept.expires_at <= self.runtime.now_ms() {
            return Err(Refused::NoSuchLink);
        }
        let mut changes = Changes::new();
        // The link's deletion fails where another took it first, and the session with it.
        changes.delete(link);
        let made = self.session(&mut changes, kept.scopes, kept.label, holder, key)?;
        self.runtime.commit(changes).await.map_err(|_| Refused::NoSuchLink)?;
        Ok(made)
    }

    /// The session `credential` holds, where it holds one that has not gone.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn held(&self, credential: &str) -> Result<Option<Shown>, Refused> {
        let Some((id, secret)) = credential.split_once('.') else {
            return Ok(None);
        };
        let Some(kept) = kept(&self.runtime.snapshot()?, &path(SESSION_KIND, id))? else {
            return Ok(None);
        };
        let live = same(&kept.digest, &digest(secret)) && kept.expires_at > self.runtime.now_ms();
        Ok(live.then(|| shown(id, &kept)))
    }

    /// The session a device holds by its key `key`, where it holds one that has not gone.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn held_by_key(&self, key: &str) -> Result<Option<Shown>, Refused> {
        let now = self.runtime.now_ms();
        let found = self
            .all(SESSION_KIND)?
            .into_iter()
            .find(|(_, kept)| kept.key.as_deref() == Some(key) && kept.expires_at > now);
        Ok(found.map(|(id, kept)| shown(&id, &kept)))
    }

    /// The sessions and the links, each in the order made.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn list(&self) -> Result<(Vec<Shown>, Vec<Shown>), Refused> {
        let now = self.runtime.now_ms();
        let live = |all: Vec<(String, Kept)>| {
            all.into_iter()
                .filter(|(_, kept)| kept.expires_at > now)
                .map(|(id, kept)| shown(&id, &kept))
                .collect()
        };
        Ok((live(self.all(SESSION_KIND)?), live(self.all(LINK_KIND)?)))
    }

    /// Every directory of `kind`, with what it keeps.
    fn all(&self, kind: &str) -> Result<Vec<(String, Kept)>, Refused> {
        let prefix = format!("{kind}:");
        let range = Range {
            prefix: prefix.as_bytes(),
            limit: usize::MAX,
            ..Range::default()
        };
        let snapshot = self.runtime.snapshot()?;
        let mut out = Vec::new();
        for entry in snapshot.list(b"", &range)?.unwrap_or_default() {
            let id = String::from_utf8_lossy(&entry.name[prefix.len()..]).into_owned();
            if let Some(kept) = kept(&snapshot, &path(kind, &id))? {
                out.push((id, kept));
            }
        }
        Ok(out)
    }

    /// Revokes the session or the link `id`: which it was.
    ///
    /// # Errors
    /// `NoSuch` where there is neither; the volume's failure.
    pub async fn revoke(&self, id: &str) -> Result<&'static str, Refused> {
        let snapshot = self.runtime.snapshot()?;
        for (kind, what) in [(SESSION_KIND, "session"), (LINK_KIND, "link")] {
            let path = path(kind, id);
            if kept(&snapshot, &path)?.is_some() {
                let mut changes = Changes::new();
                changes.delete(path);
                self.runtime.commit(changes).await?;
                return Ok(what);
            }
        }
        Err(Refused::NoSuch)
    }
}
