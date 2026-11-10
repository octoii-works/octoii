//! What the host gives the models besides the network: the providers' credentials, kept with its secrets, and the
//! bytes of the files a conversation names by reference.

use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// What a provider is reached with: an API key, a sign-in by OAuth, or nothing, for a server that takes none; as the
/// host keeps it, its JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Credential {
    ApiKey { api_key: String },
    OAuth(OAuth),
    None,
}

/// A sign-in by OAuth: the issuer; the client issued to this app for the person at their first sign-in, kept and
/// reused; this host's id for the sign-in, chosen once, which the issuer ties it to; who signed in; and the tokens,
/// none yet where a first sign-in registered its client and went no further.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuth {
    pub issuer: String,
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Tokens>,
}

/// A sign-in's tokens: the access token, its expiry in milliseconds since 1970 and the scopes it was granted; and the
/// refresh token, where the issuer gave one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    pub access: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh: Option<String>,
    pub expires_at_ms: i64,
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// The host could not read or keep a credential.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the credentials are unavailable: {0}")]
pub struct Unavailable(pub String);

/// The host's store of credentials, by provider id.
pub trait Credentials: Send + Sync {
    /// The provider's credential; none where the host keeps none.
    ///
    /// # Errors
    /// Where the store cannot be read.
    fn read(&self, provider: &str) -> Result<Option<Credential>, Unavailable>;

    /// Keeps `credential` as the provider's, written whole.
    ///
    /// # Errors
    /// Where the store cannot be written.
    fn keep(&self, provider: &str, credential: &Credential) -> Result<(), Unavailable>;

    /// Forgets the provider's credential; one the host reads elsewhere, as from its environment, is read as before.
    ///
    /// # Errors
    /// Where the store cannot be written.
    fn forget(&self, provider: &str) -> Result<(), Unavailable>;
}

/// A file a conversation names that the host does not have.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("no file {0}")]
pub struct NoSuchFile(pub String);

/// The files a conversation names by reference, as the host keeps them.
pub trait Files: Send + Sync {
    /// The bytes of the file `reference` names.
    ///
    /// # Errors
    /// Where the host has no such file.
    fn read(&self, reference: &str) -> Result<Bytes, NoSuchFile>;
}
