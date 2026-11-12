//! The providers' methods, done on Siphon's providers for the person: what each is, a sign-in begun and finished, a key
//! set or forgotten, an account added, renamed or removed — the models' providers made again after — and the models
//! listed or fetched anew.

use crate::protocol::siphon::{AccountAdded, Method, ModelsListed, ProviderStatus, ProvidersListed, SignInBegun};
use crate::protocol::{Error, Nothing};
use octoii_ai::host::Credential;
use serde::Serialize;

use super::refusal;
use crate::Siphon;
use crate::providers::{self, Refused};

fn json(value: &impl Serialize) -> Result<serde_json::Value, Error> {
    serde_json::to_value(value).map_err(|error| refusal("failed", &error.to_string()))
}

fn told(error: &Refused) -> Error {
    let code = match error {
        Refused::NoSuchProvider(_) => "no_such_provider",
        Refused::NoSignIn(_) => "no_sign_in",
        Refused::Provider(_) => "refused_by_provider",
        Refused::Accounts(_) => "bad_account",
        Refused::Beak(_) => "failed",
    };
    refusal(code, &error.to_string())
}

fn unavailable(error: &octoii_ai::host::Unavailable) -> Error {
    refusal("failed", &error.0)
}

/// Does the providers' method `method` on `siphon`.
pub(super) async fn answered(siphon: &Siphon, method: Method) -> Result<serde_json::Value, Error> {
    let models = &siphon.models;
    let refresh = || providers::refreshed(siphon).map_err(|error| told(&error));
    match method {
        Method::ListProviders(Nothing {}) => json(&ProvidersListed {
            providers: providers::shown(&siphon.runtime, models, siphon.credentials.as_ref()).map_err(|error| told(&error))?,
        }),
        Method::StatusOfProvider(of) => {
            let provider = models
                .providers()
                .into_iter()
                .find(|provider| provider.id == of.provider)
                .ok_or_else(|| told(&Refused::NoSuchProvider(of.provider.clone())))?;
            let standing = provider
                .driver
                .status(&models.call(&provider))
                .await
                .map_err(|failure| refusal("refused_by_provider", &failure.message))?;
            json(&ProviderStatus { standing })
        }
        Method::SignIn(of) => json(&SignInBegun {
            url: providers::sign_in(models, &siphon.sign_ins, &siphon.sign_in, &of.provider)
                .await
                .map_err(|error| told(&error))?,
        }),
        Method::FinishSignIn(finish) => {
            providers::finish_sign_in(models, &siphon.sign_ins, &finish.provider, &finish.redirect)
                .await
                .map_err(|error| told(&error))?;
            json(&Nothing {})
        }
        Method::SetCredential(set) => {
            let credential = Credential::ApiKey { api_key: set.api_key };
            siphon
                .credentials
                .keep(&set.provider, &credential)
                .map_err(|error| unavailable(&error))?;
            json(&Nothing {})
        }
        Method::RemoveCredential(of) => {
            siphon.credentials.forget(&of.provider).map_err(|error| unavailable(&error))?;
            json(&Nothing {})
        }
        Method::AddAccount(add) => {
            let account = crate::accounts::add(&siphon.runtime, &siphon.providers, &add.provider, &add.name, &add.key)
                .await
                .map_err(|error| told(&Refused::Accounts(error)))?;
            refresh()?;
            json(&AccountAdded { account })
        }
        Method::RenameAccount(rename) => {
            crate::accounts::rename(&siphon.runtime, &rename.account, &rename.name)
                .await
                .map_err(|error| told(&Refused::Accounts(error)))?;
            refresh()?;
            json(&Nothing {})
        }
        Method::RemoveAccount(of) => {
            if let Some(provider) = models.providers().into_iter().find(|provider| provider.id == of.account) {
                provider
                    .driver
                    .forget(&models.call(&provider))
                    .await
                    .map_err(|failure| refusal("refused_by_provider", &failure.message))?;
            }
            crate::accounts::remove(&siphon.runtime, &of.account)
                .await
                .map_err(|error| told(&Refused::Accounts(error)))?;
            siphon.credentials.forget(&of.account).map_err(|error| unavailable(&error))?;
            refresh()?;
            json(&Nothing {})
        }
        Method::ListModels(Nothing {}) => json(&ModelsListed { models: models.models() }),
        Method::RefreshModels(Nothing {}) => {
            models.refresh().await;
            json(&Nothing {})
        }
        _ => Err(refusal("bad_call", "no method of the providers'")),
    }
}
