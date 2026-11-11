//! `octoii login`: the machine added to its person's account at an accounts service, as RFC 8628 has a device join
//! one — the service asked for a code, which the person confirms in their browser, and polled meanwhile, each poll
//! carrying the machine's proof that it holds its key: its signature of the device code. Once the person confirmed, the
//! service adds the machine to their account by its key and its name, and gives it its address: the relay devices
//! reach it through, a home of its own there.

use base64::Engine as _;
use oauth2::basic::{BasicErrorResponse, BasicRevocationErrorResponse, BasicTokenIntrospectionResponse, BasicTokenType};
use oauth2::{
    AuthType, ClientId, DeviceAuthorizationUrl, DeviceCodeErrorResponseType, ExtraTokenFields, HttpRequest, HttpResponse,
    RequestTokenError, StandardDeviceAuthorizationResponse, StandardErrorResponse, StandardRevocableToken, StandardTokenResponse, TokenUrl,
};
use octoii_tunnel::{RelayUrl, SecretKey};

use crate::home::Home;
use serde::{Deserialize, Serialize};

/// The accounts a machine joins unless another is named.
pub const ACCOUNTS: &str = "https://accounts.octoii.com";

/// The client a machine signs in as.
const CLIENT: &str = "octoii";

/// What a machine signs, ahead of its device code, to show the key it is added by is its own.
pub const PROOF_CONTEXT: &[u8] = b"octoii machine\0";

/// What the person is told to confirm: the code, and the page where they confirm it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub code: String,
    pub at: String,
}

/// Why the machine was not added.
#[derive(Debug, thiserror::Error)]
pub enum LoginError {
    #[error("{0} is no accounts service's address")]
    Accounts(String),
    #[error("the accounts service: {0}")]
    Unreached(String),
    #[error("the machine was not added: {}", .0.as_deref().unwrap_or("its person declined"))]
    Denied(Option<String>),
    #[error("the accounts service refused the machine: {}", .0.as_deref().unwrap_or("it did not say why"))]
    Refused(Option<String>),
    #[error("the code was not confirmed in time; log in again for another")]
    Expired,
    #[error("the accounts service gave no address a relay is at: {0}")]
    Address(String),
}

/// What the token's answer carries beside the token: the machine's address.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Addressed {
    address: String,
}

impl ExtraTokenFields for Addressed {}

/// Adds the machine of `key`, named `name`, to an account at the accounts service at `accounts`: the code its person
/// confirms given to `told`, then the service polled till they decide. Its address, once added.
///
/// # Errors
/// Where the service is not reached, or the machine is not added.
pub async fn login(accounts: &str, key: &SecretKey, name: &str, told: impl FnOnce(Confirm)) -> Result<RelayUrl, LoginError> {
    let wrong = |_| LoginError::Accounts(accounts.to_owned());
    let client = oauth2::Client::<
        BasicErrorResponse,
        StandardTokenResponse<Addressed, BasicTokenType>,
        BasicTokenIntrospectionResponse,
        StandardRevocableToken,
        BasicRevocationErrorResponse,
    >::new(ClientId::new(CLIENT.to_owned()))
    .set_auth_type(AuthType::RequestBody)
    .set_device_authorization_url(DeviceAuthorizationUrl::new(format!("{accounts}/oauth/device_authorization")).map_err(wrong)?)
    .set_token_uri(TokenUrl::new(format!("{accounts}/oauth/token")).map_err(wrong)?);
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| LoginError::Unreached(with_causes(&error)))?;
    let send = |request: HttpRequest| sent(&http, request);
    let begun: StandardDeviceAuthorizationResponse = client
        .exchange_device_code()
        .add_extra_param("key", key.public().to_string())
        .add_extra_param("name", name)
        .request_async(&send)
        .await
        .map_err(|error| refused(&error))?;
    told(Confirm {
        code: begun.user_code().secret().clone(),
        at: begun
            .verification_uri_complete()
            .map_or_else(|| begun.verification_uri().to_string(), |complete| complete.secret().clone()),
    });
    let added = client
        .exchange_device_access_token(&begun)
        .add_extra_param("proof", proof(key, begun.device_code().secret()))
        .request_async(&send, tokio::time::sleep, None)
        .await
        .map_err(|error| match error {
            RequestTokenError::ServerResponse(answer) => match answer.error() {
                DeviceCodeErrorResponseType::AccessDenied => LoginError::Denied(answer.error_description().cloned()),
                DeviceCodeErrorResponseType::ExpiredToken => LoginError::Expired,
                _ => LoginError::Refused(answer.error_description().cloned()),
            },
            error => refused(&error),
        })?;
    let address = &added.extra_fields().address;
    relay_at(address).ok_or_else(|| LoginError::Address(address.clone()))
}

/// What a failed request of the service's became, where it did not decide on the machine.
fn refused<T>(error: &RequestTokenError<reqwest::Error, StandardErrorResponse<T>>) -> LoginError
where
    T: oauth2::ErrorResponseType + std::fmt::Display + 'static,
{
    match error {
        RequestTokenError::ServerResponse(answer) => LoginError::Refused(Some(
            answer.error_description().cloned().unwrap_or_else(|| answer.error().to_string()),
        )),
        RequestTokenError::Request(error) => LoginError::Unreached(with_causes(error)),
        RequestTokenError::Parse(error, _) => LoginError::Unreached(format!("an answer it could not read: {error}")),
        RequestTokenError::Other(error) => LoginError::Unreached(error.clone()),
    }
}

/// `error` told with what caused it, each cause after the one it caused.
fn with_causes(error: &dyn std::error::Error) -> String {
    let mut told = error.to_string();
    let mut cause = error.source();
    while let Some(each) = cause {
        told = format!("{told}: {each}");
        cause = each.source();
    }
    told
}

/// `request` sent, and its answer read whole.
async fn sent(http: &reqwest::Client, request: HttpRequest) -> Result<HttpResponse, reqwest::Error> {
    let answered = http.execute(request.try_into()?).await?;
    let (status, headers) = (answered.status(), answered.headers().clone());
    let mut response = HttpResponse::new(answered.bytes().await?.to_vec());
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    Ok(response)
}

/// The machine's proof for `device_code`: its signature of it, ahead of the proof's context, in base64url.
#[must_use]
pub fn proof(key: &SecretKey, device_code: &str) -> String {
    let signature = key.sign(&[PROOF_CONTEXT, device_code.as_bytes()].concat());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes())
}

/// The relay at a machine's address: the address itself, an `https` or `http` origin — nothing past its host and port.
#[must_use]
pub fn relay_at(address: &str) -> Option<RelayUrl> {
    let url = reqwest::Url::parse(address).ok()?;
    let origin = matches!(url.scheme(), "https" | "http")
        && url.has_host()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none();
    origin.then(|| RelayUrl::from(url))
}

/// The address the machine's account gave it, as its Home keeps it; none where it has none.
///
/// # Errors
/// Where the file cannot be read, or keeps what is no address.
pub fn address_of(home: &Home) -> std::io::Result<Option<RelayUrl>> {
    let text = match std::fs::read_to_string(home.address()) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let address = text.trim();
    let relay = relay_at(address).ok_or_else(|| std::io::Error::other(format!("{} keeps no address", home.address().display())))?;
    Ok(Some(relay))
}

/// Keeps `address` as the machine's.
///
/// # Errors
/// Where the file cannot be written.
pub fn keep_address(home: &Home, address: &RelayUrl) -> std::io::Result<()> {
    std::fs::write(home.address(), format!("{address}\n"))
}

/// Forgets the machine's address: the one it had, where it had one.
///
/// # Errors
/// Where the file cannot be read or removed.
pub fn forget_address(home: &Home) -> std::io::Result<Option<RelayUrl>> {
    let address = address_of(home)?;
    if address.is_some() {
        std::fs::remove_file(home.address())?;
    }
    Ok(address)
}
