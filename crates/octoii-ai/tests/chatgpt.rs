//! ChatGPT on the person's plan: a sign-in begun, and finished with an ID token the issuer signed; and a request whose
//! sign-in is renewed when due, and once more when the plan refuses it.

mod support;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use futures::future::BoxFuture;
use octoii_ai::driver::{Driver, SignInOptions};
use octoii_ai::drivers::CHATGPT;
use octoii_ai::host::{Credential, Credentials, OAuth, Tokens};
use octoii_ai::transport::{Body, Transport, TransportError};
use octoii_ai::vocabulary::{FailureKind, InputContent, Item, Message, Request};
use serde_json::json;

/// An issuer and a plan that answer by address: the token endpoint and the plan each with the next of their answers,
/// the keys with the test's key; and what each was sent.
struct Issuer {
    tokens: Mutex<VecDeque<String>>,
    plan: Mutex<VecDeque<u16>>,
    sent: Mutex<Vec<(String, String, String)>>,
}

impl Transport for Issuer {
    fn send(&self, request: http::Request<Bytes>) -> BoxFuture<'static, Result<http::Response<Body>, TransportError>> {
        let url = request.uri().to_string();
        let authorization = request
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        self.sent
            .lock()
            .expect("held")
            .push((url.clone(), authorization, String::from_utf8_lossy(request.body()).into_owned()));
        let (status, body) = if url.ends_with("/oauth/token") {
            (200, self.tokens.lock().expect("held").pop_front().expect("a token answer left"))
        } else if url.ends_with("/jwks.json") {
            let modulus = include_str!("fixtures/sign-in.modulus");
            (
                200,
                json!({ "keys": [{ "kty": "RSA", "kid": "k1", "n": modulus, "e": "AQAB", "alg": "RS256" }] }).to_string(),
            )
        } else {
            let status = self.plan.lock().expect("held").pop_front().expect("a plan answer left");
            (
                status,
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r\",\"status\":\"completed\"}}\n\n".to_owned(),
            )
        };
        let body: Body = Box::pin(futures::stream::iter([Ok(Bytes::from(body))]));
        let response = http::Response::builder().status(status).body(body).expect("a response");
        Box::pin(async move { Ok(response) })
    }
}

fn issuer(tokens: Vec<String>, plan: Vec<u16>) -> Arc<Issuer> {
    Arc::new(Issuer {
        tokens: Mutex::new(tokens.into()),
        plan: Mutex::new(plan.into()),
        sent: Mutex::new(Vec::new()),
    })
}

/// An ID token the issuer signed for `client_id`, naming `subject` and carrying `nonce`.
fn id_token(client_id: &str, subject: &str, nonce: &str) -> String {
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some("k1".into());
    let key = jsonwebtoken::EncodingKey::from_rsa_der(include_bytes!("fixtures/sign-in.der"));
    let claims = json!({ "iss": "https://auth.openai.com", "aud": client_id, "exp": 4_000_000_000_u64, "sub": subject, "email": "person@example.com", "nonce": nonce });
    jsonwebtoken::encode(&header, &claims, &key).expect("signed")
}

#[tokio::test]
async fn a_sign_in_begun_registers_the_app_and_finished_keeps_the_person_signed_in() {
    let store = support::Store::of(Credential::None);
    let mut call = support::call(support::Provider::streaming(&[]));
    call.credentials = store.clone();
    let options = SignInOptions {
        redirect_uri: "http://127.0.0.1:7600/auth/callback".into(),
        agent_name_hint: "Octoii".into(),
        host_id: "urn:uuid:0193".into(),
    };
    let begun = CHATGPT.sign_in(&call, &options).await.expect("begun");
    let address = url::Url::parse(&begun.url).expect("an address");
    let asked = |name: &str| {
        address
            .query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };
    assert_eq!(
        asked("client_id").as_deref(),
        Some("dynamic_agent_client"),
        "a first sign-in registers the app"
    );
    assert_eq!(asked("agent_name_hint").as_deref(), Some("Octoii"));
    assert_eq!(asked("code_challenge_method").as_deref(), Some("S256"));
    let (state, nonce) = (asked("state").expect("a state"), asked("nonce").expect("a nonce"));
    let answering = issuer(
        vec![json!({ "access_token": "a1", "refresh_token": "r1", "id_token": id_token("client_9", "user_1", &nonce), "expires_in": 3600, "scope": "openid chatgpt.tokens.use.direct" }).to_string()],
        vec![],
    );
    call.transport = answering.clone();
    let wrong = "http://127.0.0.1:7600/auth/callback?code=c&state=other&client_id=client_9";
    assert_eq!(
        begun.attempt.finish(&call, wrong).await.map_err(|failure| failure.kind),
        Err(FailureKind::Auth)
    );
    let back = format!("http://127.0.0.1:7600/auth/callback?code=c1&state={state}&client_id=client_9");
    begun.attempt.finish(&call, &back).await.expect("finished");
    let Some(Credential::OAuth(oauth)) = store.read("test").expect("read") else {
        panic!("a sign-in kept")
    };
    assert_eq!(
        (oauth.client_id.as_str(), oauth.subject.as_deref(), oauth.email.as_deref()),
        ("client_9", Some("user_1"), Some("person@example.com"))
    );
    assert_eq!(oauth.tokens.map(|tokens| tokens.access), Some("a1".into()));
    let exchanged = answering.sent.lock().expect("held")[0].2.clone();
    assert!(
        exchanged.contains("grant_type=authorization_code") && exchanged.contains("code=c1"),
        "{exchanged}"
    );
    let refused = SignInOptions {
        redirect_uri: "https://example.com/back".into(),
        ..options
    };
    assert_eq!(
        CHATGPT.sign_in(&call, &refused).await.map(|_| ()).map_err(|failure| failure.kind),
        Err(FailureKind::InvalidRequest)
    );
}

fn signed_in(expires_at_ms: i64) -> Credential {
    Credential::OAuth(OAuth {
        issuer: "https://auth.openai.com".into(),
        client_id: "client_9".into(),
        host_id: Some("urn:uuid:0193".into()),
        subject: Some("user_1".into()),
        email: None,
        tokens: Some(Tokens {
            access: "old".into(),
            refresh: Some("r1".into()),
            expires_at_ms,
            scopes: vec!["chatgpt.tokens.use.direct".into()],
        }),
    })
}

fn request() -> Request {
    Request {
        model: "chatgpt/gpt-6-sol".into(),
        instructions: None,
        input: vec![Item::Message(Message::User {
            content: vec![InputContent::text("hi")],
        })],
        tools: vec![],
        tool_choice: None,
        reasoning: None,
        format: None,
        max_output_tokens: None,
        prompt_cache_key: None,
    }
}

#[tokio::test]
async fn a_sign_in_due_is_renewed_before_the_request_and_one_refused_once_after() {
    let fresh = |access: &str| json!({ "access_token": access, "expires_in": 3600 }).to_string();
    let answering = issuer(vec![fresh("new1"), fresh("new2")], vec![200, 401, 200]);
    let store = support::Store::of(signed_in(1000));
    let mut call = support::call(support::Provider::streaming(&[]));
    call.credentials = store.clone();
    call.transport = answering.clone();
    call.now_ms = 1000;
    let model = support::model("chatgpt", "gpt-6-sol");
    let _ = CHATGPT.stream(&call, &model, &request()).await.expect("sent");
    let _ = CHATGPT.stream(&call, &model, &request()).await.expect("sent again");
    let sent = answering.sent.lock().expect("held");
    let to_the_plan: Vec<&str> = sent
        .iter()
        .filter(|(url, _, _)| url.ends_with("/responses"))
        .map(|(_, authorization, _)| authorization.as_str())
        .collect();
    assert_eq!(
        to_the_plan,
        ["Bearer new1", "Bearer new1", "Bearer new2"],
        "renewed when due, and once more when refused"
    );
    let Some(Credential::OAuth(kept)) = store.read("test").expect("read") else {
        panic!("a sign-in kept")
    };
    let tokens = kept.tokens.expect("tokens");
    assert_eq!(
        (tokens.access.as_str(), tokens.refresh.as_deref()),
        ("new2", Some("r1")),
        "the refresh token kept where none was given"
    );
}
