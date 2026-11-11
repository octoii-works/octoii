//! ChatGPT: OpenAI's Responses API on the person's ChatGPT plan, reached by signing in with ChatGPT — OAuth with PKCE
//! at OpenAI's issuer. The first sign-in registers the app for the person through the placeholder client, and later
//! ones reuse the client it was issued, under the host's id. The ID token is checked against the issuer's published
//! keys before the credential is kept. A request goes with the sign-in's access token, read at each use: renewed first
//! where it expires within five minutes, and once where the plan refuses it unless renewed meanwhile, the request then
//! sent once more. One renewal runs at a time, the credential read again inside it, so a refresh token is never spent
//! twice.

use base64::Engine;
use futures::StreamExt;
use futures::future::BoxFuture;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::catalogue::Model;
use crate::driver::{Call, Connection, Driver, SignIn, SignInAttempt, SignInOptions};
use crate::host::{Credential, OAuth, Tokens};
use crate::protocols::responses::{self, DeveloperRole, Dialect, Reader};
use crate::stream::Source;
use crate::transport::Body;
use crate::vocabulary::{Failure, FailureKind, Request};
use crate::wire;

/// Who issues the tokens of a sign-in with ChatGPT, as its ID tokens name it.
pub const ISSUER: &str = "https://auth.openai.com";
/// The scope that lets the tokens use the person's plan.
pub const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";
/// Where the API the plan opens is, which the issuer names as the tokens' resource.
pub const BASE_URL: &str = "https://api.openai.com/v1";
const AUTHORIZE_URL: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/api/accounts/oauth/token";
const KEYS_URL: &str = "https://auth.openai.com/.well-known/jwks.json";
/// The placeholder client a first sign-in registers the app through; never the client a credential keeps.
const REGISTRATION_CLIENT: &str = "dynamic_agent_client";
/// What a sign-in asks for: who the person is, a refresh token, and the use of the plan.
const SCOPES: [&str; 6] = ["openid", "profile", "email", "offline_access", "resource.invoke", PLAN_SCOPE];
/// The errors by which the issuer says a code or a refresh token is no longer good.
const SPENT: [&str; 6] = [
    "invalid_grant",
    "invalid_refresh_token",
    "token_expired",
    "refresh_token_expired",
    "refresh_token_invalidated",
    "refresh_token_reused",
];
/// How long before it expires an access token is renewed.
const RENEW_MARGIN_MS: i64 = 5 * 60_000;

/// The plan's own codes, before OpenAI's.
const PLAN_CODES: [(&str, FailureKind); 9] = [
    ("subscription_sharing_usage_limit_exceeded", FailureKind::Quota),
    ("subscription_sharing_usage_unavailable", FailureKind::Server),
    ("subscription_sharing_user_unavailable", FailureKind::Server),
    ("subscription_sharing_user_not_eligible", FailureKind::Forbidden),
    ("subscription_sharing_invalid_user", FailureKind::Auth),
    ("chatpass_v2_scope_not_authorized", FailureKind::Auth),
    ("chatpass_v2_invalid_authorization_context", FailureKind::Auth),
    ("subscription_sharing_unsupported_capability", FailureKind::InvalidRequest),
    ("subscription_sharing_route_not_supported", FailureKind::InvalidRequest),
];

const DIALECT: Dialect = Dialect {
    developer_role: DeveloperRole::Developer,
    cache_key: true,
    encrypted_reasoning: true,
    takes_text_files: true,
    failure_kind: |code, message| {
        let code = code?;
        PLAN_CODES
            .iter()
            .find(|(each, _)| *each == code)
            .map(|(_, kind)| *kind)
            .or_else(|| (super::OPENAI.dialect.failure_kind)(Some(code), message))
    },
};

/// ChatGPT's driver: one renewal of a sign-in at a time.
#[derive(Debug)]
pub struct ChatGpt {
    renewing: Mutex<()>,
}

impl ChatGpt {
    /// ChatGPT, on the person's plan.
    #[must_use]
    pub fn new() -> Self {
        Self { renewing: Mutex::new(()) }
    }
}

impl Default for ChatGpt {
    fn default() -> Self {
        Self::new()
    }
}

/// When a sign-in's tokens are stale: once they are due, or while they are still those the plan refused.
#[derive(Debug, Clone, Copy)]
enum Stale<'a> {
    Due,
    Refused(&'a str),
}

impl Stale<'_> {
    fn of(self, tokens: &Tokens, now_ms: i64) -> bool {
        match self {
            Self::Due => tokens.expires_at_ms - RENEW_MARGIN_MS <= now_ms,
            Self::Refused(access) => tokens.access == access,
        }
    }
}

fn auth(words: impl Into<String>) -> Failure {
    Failure::new(FailureKind::Auth, words)
}

/// The sign-in a credential is, with its tokens.
fn signed_in(credential: Option<Credential>) -> Result<(OAuth, Tokens), Failure> {
    match credential {
        Some(Credential::OAuth(oauth)) => {
            let tokens = oauth
                .tokens
                .clone()
                .ok_or_else(|| auth("ChatGPT is not signed in: sign in with ChatGPT"))?;
            Ok((oauth, tokens))
        }
        _ => Err(auth("ChatGPT is not signed in: sign in with ChatGPT")),
    }
}

impl ChatGpt {
    /// The sign-in's tokens after a renewal: the credential read again inside the one renewal running, and renewed
    /// only where it is still stale, so a renewal made meanwhile is used as it is.
    async fn renewed(&self, call: &Call, stale: Stale<'_>) -> Result<Tokens, Failure> {
        let _alone = self.renewing.lock().await;
        let (oauth, tokens) = signed_in(call.credential()?)?;
        if !stale.of(&tokens, call.now_ms) {
            return Ok(tokens);
        }
        let refresh = tokens
            .refresh
            .clone()
            .ok_or_else(|| auth("The sign-in to ChatGPT has expired and holds no refresh token: sign in again"))?;
        let form = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
            ("client_id", oauth.client_id.as_str()),
            ("resource", BASE_URL),
        ];
        let answer = tokens_for(call, &form, "The sign-in to ChatGPT is no longer good: sign in again").await?;
        let scopes = if tokens.scopes.is_empty() {
            SCOPES.map(str::to_owned).to_vec()
        } else {
            tokens.scopes.clone()
        };
        let renewed = answer.tokens(call.now_ms, Some(refresh), &scopes);
        let kept = Credential::OAuth(OAuth {
            tokens: Some(renewed.clone()),
            ..oauth
        });
        call.credentials
            .keep(&call.provider, &kept)
            .map_err(|unavailable| auth(unavailable.to_string()))?;
        Ok(renewed)
    }

    /// Sends what `made` makes of an access token, with the sign-in's: renewed first where due, and once more where the
    /// plan refuses it.
    async fn send(
        &self,
        call: &Call,
        made: impl Fn(&str) -> Result<http::Request<bytes::Bytes>, Failure>,
    ) -> Result<http::Response<Body>, Failure> {
        let (_, mut tokens) = signed_in(call.credential()?)?;
        if !tokens.scopes.is_empty() && !tokens.scopes.iter().any(|scope| scope == PLAN_SCOPE) {
            return Err(auth(format!(
                "The sign-in to ChatGPT may not use the plan: its tokens lack the scope {PLAN_SCOPE}; sign in again and allow the use of the plan"
            )));
        }
        if Stale::Due.of(&tokens, call.now_ms) {
            tokens = self.renewed(call, Stale::Due).await?;
        }
        let response = wire::send(call, made(&tokens.access)?).await?;
        if response.status() != http::StatusCode::UNAUTHORIZED {
            return Ok(response);
        }
        let tokens = self.renewed(call, Stale::Refused(&tokens.access)).await?;
        wire::send(call, made(&tokens.access)?).await
    }
}

impl Driver for ChatGpt {
    fn id(&self) -> &'static str {
        "chatgpt"
    }

    fn name(&self) -> &'static str {
        "ChatGPT"
    }

    fn connection(&self) -> Connection {
        Connection::SignIn
    }

    fn stream<'a>(&'a self, call: &'a Call, model: &'a Model, request: &'a Request) -> BoxFuture<'a, Result<Source, Failure>> {
        Box::pin(async move {
            let body = bytes::Bytes::from(responses::render(call, model, request, &DIALECT)?.to_string());
            let url = format!("{}/responses", call.base_url(BASE_URL));
            let response = self
                .send(call, |access| {
                    http::Request::post(&url)
                        .header("authorization", format!("Bearer {access}"))
                        .header("accept", "text/event-stream")
                        .header("content-type", "application/json")
                        .body(body.clone())
                        .map_err(|error| Failure::new(FailureKind::InvalidRequest, error.to_string()))
                })
                .await?;
            let mut reader = Reader::new(DIALECT);
            if !response.status().is_success() {
                return Err(wire::failure_of(call, response, &|body: &str| reader.failure_detail(body)).await);
            }
            let events = wire::events(response.into_body()).filter_map(move |event| {
                let made = match event {
                    Ok(event) => reader.event(&event.data),
                    Err(failure) => Some(Err(failure)),
                };
                async move { made }
            });
            let source: Source = Box::pin(events);
            Ok(source)
        })
    }

    fn sign_in<'a>(&'a self, call: &'a Call, options: &'a SignInOptions) -> BoxFuture<'a, Result<SignIn, Failure>> {
        Box::pin(async move { begin(call, options) })
    }
}

/// What one sign-in attempt keeps till it finishes: its secrets, and the client and host it signs in as.
#[derive(Debug)]
struct Attempt {
    state: String,
    nonce: String,
    verifier: String,
    host_id: String,
    /// The client issued at an earlier sign-in, reused; none for a first sign-in, which registers one.
    client_id: Option<String>,
    /// Who signed in before, whom signing in again must be.
    subject: Option<String>,
    redirect_uri: String,
}

fn random_text() -> String {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Whether `uri` is `http://127.0.0.1:<port>/auth/callback`, the one redirect address the issuer takes.
fn loopback_callback(uri: &str) -> bool {
    uri.strip_prefix("http://127.0.0.1:")
        .and_then(|rest| rest.strip_suffix("/auth/callback"))
        .is_some_and(|port| (1..=5).contains(&port.len()) && port.bytes().all(|byte| byte.is_ascii_digit()))
}

fn begin(call: &Call, options: &SignInOptions) -> Result<SignIn, Failure> {
    if !loopback_callback(&options.redirect_uri) {
        return Err(Failure::new(
            FailureKind::InvalidRequest,
            format!(
                "The redirect address {} is not one the issuer takes: it is http://127.0.0.1:<port>/auth/callback",
                options.redirect_uri
            ),
        ));
    }
    let earlier = match call.credential()? {
        Some(Credential::OAuth(oauth)) if oauth.issuer == ISSUER => Some(oauth),
        _ => None,
    };
    let verifier = random_text();
    let attempt = Attempt {
        state: random_text(),
        nonce: random_text(),
        host_id: earlier
            .as_ref()
            .and_then(|given| given.host_id.clone())
            .unwrap_or_else(|| options.host_id.clone()),
        client_id: earlier.as_ref().map(|given| given.client_id.clone()),
        subject: earlier.and_then(|given| given.subject),
        redirect_uri: options.redirect_uri.clone(),
        verifier,
    };
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(attempt.verifier.as_bytes()));
    let scopes = SCOPES.join(" ");
    let mut url = url::Url::parse(AUTHORIZE_URL).expect("the issuer's address is one");
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", attempt.client_id.as_deref().unwrap_or(REGISTRATION_CLIENT))
            .append_pair("redirect_uri", &attempt.redirect_uri)
            .append_pair("scope", &scopes)
            .append_pair("resource", BASE_URL)
            .append_pair("state", &attempt.state)
            .append_pair("nonce", &attempt.nonce)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        if attempt.client_id.is_none() {
            query.append_pair("agent_name_hint", &options.agent_name_hint);
        }
        query.append_pair("ext_agent_host_id", &attempt.host_id);
    }
    Ok(SignIn {
        url: url.into(),
        attempt: Box::new(attempt),
    })
}

/// What the issuer's token endpoint answers with, its expiry in seconds and its scopes apart by spaces.
#[derive(Debug, Deserialize)]
struct TokenAnswer {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: f64,
    scope: Option<String>,
}

impl TokenAnswer {
    /// The tokens to keep: the refresh token the issuer gave, else the one spent; the scopes it granted, else those
    /// asked for.
    fn tokens(&self, now_ms: i64, spent: Option<String>, asked: &[String]) -> Tokens {
        #[expect(clippy::cast_possible_truncation, reason = "an expiry in seconds, far below i64's")]
        let lasts_ms = (self.expires_in * 1000.0) as i64;
        Tokens {
            access: self.access_token.clone(),
            refresh: self.refresh_token.clone().or(spent),
            expires_at_ms: now_ms + lasts_ms,
            scopes: self
                .scope
                .as_ref()
                .map_or_else(|| asked.to_vec(), |granted| granted.split(' ').map(str::to_owned).collect()),
        }
    }
}

/// The tokens the issuer gives for `form`; `auth`, with `spent`, where it says the code or refresh token is no longer
/// good.
async fn tokens_for(call: &Call, form: &[(&str, &str)], spent: &str) -> Result<TokenAnswer, Failure> {
    let body = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(form).finish();
    let request = http::Request::post(TOKEN_URL)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .body(bytes::Bytes::from(body))
        .map_err(|error| Failure::new(FailureKind::InvalidRequest, error.to_string()))?;
    let response = wire::send(call, request).await?;
    wire::json(call, response, &|body: &str| {
        let said: serde_json::Value = serde_json::from_str(body).ok()?;
        let code = said["error"].as_str().or_else(|| said["error"]["code"].as_str())?;
        SPENT.contains(&code).then(|| wire::FailureDetail {
            kind: Some(FailureKind::Auth),
            message: Some(format!("{spent} (the issuer said {code})")),
            code: Some(code.to_owned()),
            ..wire::FailureDetail::default()
        })
    })
    .await
}

/// The claims of an ID token a sign-in checks.
#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    email: Option<String>,
    nonce: Option<String>,
}

/// The claims of `id_token` once it holds: signed with RS256 by one of the issuer's published keys, issued by the
/// issuer for `client_id`, not expired, and carrying the attempt's `nonce`.
async fn verified_claims(call: &Call, id_token: &str, client_id: &str, nonce: &str) -> Result<Claims, Failure> {
    let refused = |why: &str| auth(format!("The issuer's ID token does not hold: {why}"));
    let header = jsonwebtoken::decode_header(id_token).map_err(|_| refused("it is no token signed with RS256"))?;
    if header.alg != jsonwebtoken::Algorithm::RS256 {
        return Err(refused("it is not signed with RS256"));
    }
    let request = http::Request::get(KEYS_URL)
        .body(bytes::Bytes::new())
        .map_err(|error| Failure::new(FailureKind::InvalidRequest, error.to_string()))?;
    let keys: serde_json::Value = wire::json(call, wire::send(call, request).await?, &wire::nothing).await?;
    let key = keys["keys"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|key| key["kty"] == "RSA" && header.kid.as_deref().is_none_or(|kid| key["kid"] == kid))
        .ok_or_else(|| refused("none of the issuer's published keys signed it"))?;
    let decoding =
        jsonwebtoken::DecodingKey::from_rsa_components(key["n"].as_str().unwrap_or_default(), key["e"].as_str().unwrap_or_default())
            .map_err(|_| refused("none of the issuer's published keys signed it"))?;
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[client_id]);
    let claims = jsonwebtoken::decode::<Claims>(id_token, &decoding, &validation)
        .map_err(|error| refused(&error.to_string()))?
        .claims;
    if claims.nonce.as_deref() != Some(nonce) {
        return Err(refused("it does not carry this attempt's nonce"));
    }
    if claims.sub.is_empty() {
        return Err(refused("it names no account"));
    }
    Ok(claims)
}

impl SignInAttempt for Attempt {
    fn finish<'a>(&'a self, call: &'a Call, redirect: &'a str) -> BoxFuture<'a, Result<(), Failure>> {
        Box::pin(async move {
            let address =
                url::Url::parse(redirect.trim()).map_err(|_| auth("What was given is not the address the browser was sent back to"))?;
            let given = |name: &str| {
                address
                    .query_pairs()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.into_owned())
            };
            if let Some(refusal) = given("error") {
                return Err(auth(if refusal == "access_denied" {
                    "The person declined to sign in with ChatGPT".to_owned()
                } else {
                    format!("The sign-in with ChatGPT failed: {}", given("error_description").unwrap_or(refusal))
                }));
            }
            if given("state").as_deref() != Some(self.state.as_str()) {
                return Err(auth("The address is not of this sign-in attempt: its state differs; sign in again"));
            }
            let code = given("code")
                .filter(|code| !code.is_empty())
                .ok_or_else(|| auth("The address holds no code: sign in again"))?;
            let client_id = match (&self.client_id, given("client_id")) {
                (Some(known), Some(other)) if *known != other => {
                    return Err(auth(format!(
                        "The issuer returned the client {other}, not {known} it issued before: sign in again"
                    )));
                }
                (Some(known), _) => known.clone(),
                (None, Some(issued)) if !issued.is_empty() && issued != REGISTRATION_CLIENT => issued,
                (None, _) => return Err(auth("The registration gave no client to sign in as: sign in again")),
            };
            let form = [
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("code_verifier", self.verifier.as_str()),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("client_id", client_id.as_str()),
                ("resource", BASE_URL),
            ];
            let answer = tokens_for(call, &form, "The code is no longer valid: sign in again").await?;
            let id_token = answer
                .id_token
                .as_deref()
                .ok_or_else(|| auth("The issuer gave no ID token, by which Octoii knows who signed in"))?;
            let claims = verified_claims(call, id_token, &client_id, &self.nonce).await?;
            if self.subject.as_deref().is_some_and(|subject| subject != claims.sub) {
                return Err(auth(
                    "The person signed in to another ChatGPT account than the one signing in again: sign out of it first",
                ));
            }
            let asked = SCOPES.map(str::to_owned);
            let credential = Credential::OAuth(OAuth {
                issuer: ISSUER.to_owned(),
                client_id,
                host_id: Some(self.host_id.clone()),
                subject: Some(claims.sub),
                email: claims.email,
                tokens: Some(answer.tokens(call.now_ms, None, &asked)),
            });
            call.credentials
                .keep(&call.provider, &credential)
                .map_err(|unavailable| auth(format!("The host's credentials cannot keep the sign-in: {unavailable}")))
        })
    }
}
