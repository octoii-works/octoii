//! The seam between the models and a driver: a driver speaks one provider's API — renders a request for it, reads
//! its answer back into the vocabulary, lists its models where its provider lists them — through the call it is given,
//! which holds what the provider and the host give it.

use std::sync::Arc;

use futures::future::BoxFuture;

use crate::catalogue::Model;
use crate::host::{Credential, Credentials, Files};
use crate::stream::Source;
use crate::transport::Transport;
use crate::vocabulary::{Failure, FailureKind, Request};

/// How a provider is reached: by an API key, its environment's variable named; by a sign-in in a browser; as a server
/// of the person's that takes no credential; or through an agent the person signs in to on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    ApiKey { variable: &'static str },
    SignIn,
    Server,
    Agent { through: &'static str },
}

/// What a provider sets of its driver: where it reaches its API — the address its paths follow, without a trailing
/// `/` — none, the driver's own; and headers sent with each of its requests over the driver's own of the same name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    pub base_url: Option<String>,
    pub headers: Vec<(String, String)>,
}

/// A call of a driver's: the provider it is made for, by its id, and its settings; the host's credentials, read for
/// the provider's at each use and kept as a sign-in renews; where its requests go; the files a conversation names; and
/// the time it is made, in milliseconds since 1970.
#[derive(Clone)]
pub struct Call {
    pub provider: String,
    pub settings: Settings,
    pub credentials: Arc<dyn Credentials>,
    pub transport: Arc<dyn Transport>,
    pub files: Arc<dyn Files>,
    pub now_ms: i64,
}

impl std::fmt::Debug for Call {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Call")
            .field("provider", &self.provider)
            .field("settings", &self.settings)
            .field("now_ms", &self.now_ms)
            .finish_non_exhaustive()
    }
}

impl Call {
    /// The address the provider's API is reached at: its own, else `default`.
    #[must_use]
    pub fn base_url<'a>(&'a self, default: &'a str) -> &'a str {
        self.settings.base_url.as_deref().unwrap_or(default)
    }

    /// The provider's credential, as the host keeps it now; none where it keeps none.
    ///
    /// # Errors
    /// An `auth` failure where the host's credentials cannot be read.
    pub fn credential(&self) -> Result<Option<Credential>, Failure> {
        self.credentials
            .read(&self.provider)
            .map_err(|unavailable| Failure::new(FailureKind::Auth, unavailable.to_string()))
    }

    /// The provider's API key; none where the host keeps another credential for it, or none.
    ///
    /// # Errors
    /// An `auth` failure where the host's credentials cannot be read.
    pub fn api_key(&self) -> Result<Option<String>, Failure> {
        Ok(match self.credential()? {
            Some(Credential::ApiKey { api_key }) => Some(api_key),
            _ => None,
        })
    }
}

/// The code that speaks one provider's API.
pub trait Driver: Send + Sync {
    /// Such as `anthropic`: the key of the metadata it attaches to what its models write.
    fn id(&self) -> &'static str;

    /// For people, such as `Anthropic`.
    fn name(&self) -> &'static str;

    fn connection(&self) -> Connection;

    /// Streams the answer of `model` to `request`, the request rendered for the provider.
    fn stream<'a>(&'a self, call: &'a Call, model: &'a Model, request: &'a Request) -> BoxFuture<'a, Result<Source, Failure>>;

    /// Begins a sign-in in a browser, for a driver whose provider is reached by one.
    ///
    /// # Errors
    /// `invalid_request` for a provider reached otherwise.
    fn sign_in<'a>(&'a self, call: &'a Call, options: &'a SignInOptions) -> BoxFuture<'a, Result<SignIn, Failure>> {
        let _ = (call, options);
        Box::pin(async move {
            Err(Failure::new(
                FailureKind::InvalidRequest,
                format!("{} is not reached by a sign-in", self.name()),
            ))
        })
    }

    /// The models the provider serves now, where it lists them — none for a provider whose models are the
    /// catalogue's alone — each with what the listing says of it.
    fn list<'a>(&'a self, call: &'a Call) -> BoxFuture<'a, Result<Option<Vec<Model>>, Failure>> {
        let _ = call;
        Box::pin(async { Ok(None) })
    }
}

/// What a sign-in in a browser is begun with: the address the person's browser is sent back to, the name the app is
/// offered under on a first sign-in, and the host's id for a sign-in, kept where the provider's earlier one holds none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInOptions {
    pub redirect_uri: String,
    pub agent_name_hint: String,
    pub host_id: String,
}

/// A sign-in begun: where the person signs in, and the attempt it finishes by.
pub struct SignIn {
    pub url: String,
    pub attempt: Box<dyn SignInAttempt>,
}

impl std::fmt::Debug for SignIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignIn").field("url", &self.url).finish_non_exhaustive()
    }
}

/// A sign-in attempt, finished from the address the person's browser was sent back to.
pub trait SignInAttempt: Send + Sync {
    /// Finishes the sign-in, its credential kept by the host.
    fn finish<'a>(&'a self, call: &'a Call, redirect: &'a str) -> BoxFuture<'a, Result<(), Failure>>;
}
