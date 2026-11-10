//! What the host gives the models besides the network: the providers' credentials, kept with its secrets, and the
//! bytes of the files a conversation names by reference.

use bytes::Bytes;

/// What a provider is reached with: an API key, a sign-in by OAuth, or nothing, for a server that takes none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    ApiKey(String),
    OAuth(OAuth),
    None,
}

/// A sign-in by OAuth, as the host keeps it: the issuer and the client issued to this app, who signed in, and the
/// tokens, the access token's expiry in milliseconds since 1970.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuth {
    pub issuer: String,
    pub client_id: String,
    pub account_id: Option<String>,
    pub email: Option<String>,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_ms: i64,
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
