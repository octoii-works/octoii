//! The providers models are reached through, as the person manages them: the host's and the accounts they added of
//! them, each with how it is connected, the kind of credential kept for it — never the credential — and its quota
//! where it is used up; a key set or forgotten; a sign-in in a browser begun, and finished from the address the
//! person's browser was sent back to; an account added, renamed or removed, the models' providers made again from
//! them. What a sign-in begun waits on is held by a task of its own till it is finished, as its one owner.

use std::collections::HashMap;

use crate::protocol::siphon::{Connects, CredentialKind, ProviderShown};
use octoii_ai::driver::{Connection, SignInAttempt, SignInOptions};
use octoii_ai::host::{Credential, Credentials};
use octoii_ai::models::{Models, Provider};
use siphon_runtime::Runtime;

use crate::Siphon;
use tokio::sync::{mpsc, oneshot};

/// How a host's sign-ins in a browser come back: the address the person's browser is sent back to, `{provider}` in
/// it the provider's id; and the host's id for a sign-in.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SignInSetup {
    pub callback: String,
    pub host_id: String,
}

/// Why what was asked of the providers was not done.
#[derive(Debug, thiserror::Error)]
pub enum Refused {
    #[error("no provider {0}")]
    NoSuchProvider(String),
    #[error("no sign-in of {0} waits to be finished")]
    NoSignIn(String),
    #[error("{0}")]
    Provider(String),
    #[error(transparent)]
    Accounts(#[from] crate::accounts::Error),
    #[error(transparent)]
    Beak(#[from] beak::Error),
}

/// What the task that holds the sign-ins begun is asked.
enum Asked {
    Begun {
        provider: String,
        attempt: Box<dyn SignInAttempt>,
    },
    Taken {
        provider: String,
        reply: oneshot::Sender<Option<Box<dyn SignInAttempt>>>,
    },
}

/// The sign-ins begun and not yet finished, held by a task of their own.
#[derive(Debug, Clone)]
pub struct SignIns(mpsc::UnboundedSender<Asked>);

impl std::fmt::Debug for Asked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Asked")
    }
}

impl SignIns {
    /// The sign-ins, their task started on the current runtime.
    #[must_use]
    pub fn start() -> Self {
        let (asks, mut asked) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut waiting: HashMap<String, Box<dyn SignInAttempt>> = HashMap::new();
            while let Some(ask) = asked.recv().await {
                match ask {
                    // A sign-in begun again stands in for the one before it.
                    Asked::Begun { provider, attempt } => {
                        waiting.insert(provider, attempt);
                    }
                    Asked::Taken { provider, reply } => {
                        let _ = reply.send(waiting.remove(&provider));
                    }
                }
            }
        });
        Self(asks)
    }
}

/// The provider `id` of `models`.
fn provider(models: &Models, id: &str) -> Result<Provider, Refused> {
    models
        .providers()
        .into_iter()
        .find(|provider| provider.id == id)
        .ok_or_else(|| Refused::NoSuchProvider(id.to_owned()))
}

/// The providers as the person is shown them.
///
/// # Errors
/// The volume's failure.
pub(crate) fn shown(runtime: &Runtime, models: &Models, credentials: &dyn Credentials) -> Result<Vec<ProviderShown>, Refused> {
    let snapshot = runtime.snapshot()?;
    let accounts = crate::accounts::accounts(&snapshot)?;
    let mut out = Vec::new();
    for provider in models.providers() {
        let (connects, through) = match provider.driver.connection() {
            Connection::ApiKey { .. } => (Connects::ApiKey, None),
            Connection::SignIn => (Connects::SignIn, None),
            Connection::Server => (Connects::Server, None),
            Connection::Agent { through } => (Connects::Agent, Some(through.to_owned())),
        };
        let credential = credentials.read(&provider.id).ok().flatten().map(|credential| match credential {
            Credential::ApiKey { .. } => CredentialKind::ApiKey,
            Credential::OAuth(_) => CredentialKind::OAuth,
            Credential::None => CredentialKind::None,
        });
        out.push(ProviderShown {
            account: accounts.iter().find(|account| account.id == provider.id).cloned(),
            used_up: crate::accounts::used_up(&snapshot, &provider.id)?,
            id: provider.id,
            name: provider.name,
            connects,
            through,
            credential,
        });
    }
    Ok(out)
}

/// Begins a sign-in of the provider `id` in a browser: where the person signs in.
///
/// # Errors
/// `NoSuchProvider`; the provider's refusal.
pub(crate) async fn sign_in(models: &Models, sign_ins: &SignIns, setup: &SignInSetup, id: &str) -> Result<String, Refused> {
    let provider = provider(models, id)?;
    let options = SignInOptions {
        redirect_uri: setup.callback.replace("{provider}", id),
        agent_name_hint: "Octoii".to_owned(),
        host_id: setup.host_id.clone(),
    };
    let begun = provider
        .driver
        .sign_in(&models.call(&provider), &options)
        .await
        .map_err(|failure| Refused::Provider(failure.message))?;
    let _ = sign_ins.0.send(Asked::Begun {
        provider: id.to_owned(),
        attempt: begun.attempt,
    });
    Ok(begun.url)
}

/// Finishes the sign-in of the provider `id` from `redirect`, the address the person's browser was sent back to.
///
/// # Errors
/// `NoSuchProvider`, `NoSignIn`; the provider's refusal.
pub async fn finish_sign_in(models: &Models, sign_ins: &SignIns, id: &str, redirect: &str) -> Result<(), Refused> {
    let provider = provider(models, id)?;
    let (reply, taken) = oneshot::channel();
    let _ = sign_ins.0.send(Asked::Taken {
        provider: id.to_owned(),
        reply,
    });
    let attempt = taken.await.ok().flatten().ok_or_else(|| Refused::NoSignIn(id.to_owned()))?;
    attempt
        .finish(&models.call(&provider), redirect)
        .await
        .map_err(|failure| Refused::Provider(failure.message))
}

/// The models' providers made again: the host's and its extensions' — but those of an extension the person switched
/// off — then an instance of each account the person added of them.
///
/// # Errors
/// The volume's failure.
pub(crate) fn refreshed(siphon: &Siphon) -> Result<(), Refused> {
    let snapshot = siphon.runtime.snapshot()?;
    let accounts = crate::accounts::accounts(&snapshot)?;
    let on: Vec<Provider> = siphon
        .providers
        .iter()
        .filter(|provider| {
            siphon
                .registry
                .providers
                .get(&provider.id)
                .is_none_or(|extension| crate::extension::Registry::on(&snapshot, extension))
        })
        .cloned()
        .collect();
    siphon.models.set_providers(crate::accounts::providers(&on, &accounts));
    Ok(())
}
