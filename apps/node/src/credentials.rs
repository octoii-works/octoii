//! The providers' credentials, as the node keeps them in its Home: a file for each provider in `etc/credstore/`,
//! readable by the person alone and written whole at once, as a new file renamed over the old — a key the person set,
//! a sign-in kept and renewed — and, for a provider with none there, the key the login environment holds under the
//! variable the provider names.

use std::collections::HashMap;
use std::path::PathBuf;

use octoii_ai::driver::Connection;
use octoii_ai::host::{Credential, Credentials, Unavailable};
use octoii_ai::models::Provider;

/// The credentials kept in a folder, and the variables of the environment the keyed providers name.
#[derive(Debug, Clone)]
pub struct Credstore {
    folder: PathBuf,
    variables: HashMap<String, &'static str>,
}

impl Credstore {
    /// The credentials kept in `folder`, for `providers`.
    #[must_use]
    pub fn new(folder: PathBuf, providers: &[Provider]) -> Self {
        let variables = providers
            .iter()
            .filter_map(|provider| match provider.driver.connection() {
                Connection::ApiKey { variable } => Some((provider.id.clone(), variable)),
                _ => None,
            })
            .collect();
        Self { folder, variables }
    }

    /// The file of the provider `provider`'s credential; none for an id no file can be named by.
    fn file(&self, provider: &str) -> Result<PathBuf, Unavailable> {
        let named = !provider.is_empty() && provider.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte));
        if !named || provider.starts_with('.') {
            return Err(Unavailable(format!("no credential is kept for {provider:?}")));
        }
        Ok(self.folder.join(format!("{provider}.json")))
    }
}

fn unavailable(error: impl std::fmt::Display) -> Unavailable {
    Unavailable(error.to_string())
}

impl Credentials for Credstore {
    fn read(&self, provider: &str) -> Result<Option<Credential>, Unavailable> {
        match std::fs::read(self.file(provider)?) {
            Ok(bytes) => return serde_json::from_slice(&bytes).map(Some).map_err(unavailable),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(unavailable(error)),
        }
        let key = self.variables.get(provider).and_then(|variable| std::env::var(variable).ok());
        Ok(key.filter(|key| !key.is_empty()).map(|api_key| Credential::ApiKey { api_key }))
    }

    fn keep(&self, provider: &str, credential: &Credential) -> Result<(), Unavailable> {
        let file = self.file(provider)?;
        let written = file.with_extension("json.new");
        let bytes = serde_json::to_vec(credential).map_err(unavailable)?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut opened = options.open(&written).map_err(unavailable)?;
        std::io::Write::write_all(&mut opened, &bytes).map_err(unavailable)?;
        opened.sync_all().map_err(unavailable)?;
        std::fs::rename(&written, &file).map_err(unavailable)
    }

    fn forget(&self, provider: &str) -> Result<(), Unavailable> {
        match std::fs::remove_file(self.file(provider)?) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(unavailable(error)),
            _ => Ok(()),
        }
    }
}
