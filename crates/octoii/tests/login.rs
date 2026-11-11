//! A machine logs in at an accounts service of the tests' own: it shows its code, waits on its person, and is added at
//! its address once it proves its key; one its person declines is told so; the address kept in its Home and forgotten.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use base64::Engine as _;
use octoii::home::Home;
use octoii::login::{self, Confirm, LoginError, PROOF_CONTEXT};
use octoii_tunnel::{PublicKey, SecretKey};
use serde_json::json;

const ADDRESS: &str = "https://k3x7mq2vtd7a.octoiitunnel.test";

/// What the service decides once the machine has polled once.
#[derive(Clone, Copy)]
enum Decision {
    Add,
    Decline,
}

/// The service's state: its decision, the key a machine began with, and how many times it polled.
struct Accounts {
    decision: Decision,
    key: Option<PublicKey>,
    polls: usize,
}

type Shared = Arc<Mutex<Accounts>>;

fn refusal(error: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response()
}

async fn device_authorization(State(accounts): State<Shared>, Form(form): Form<HashMap<String, String>>) -> Response {
    if form.get("client_id").map(String::as_str) != Some("octoii") || form.get("name").map(String::as_str) != Some("studio mac") {
        return refusal("invalid_request");
    }
    accounts.lock().expect("the state").key = form.get("key").and_then(|key| key.parse().ok());
    Json(json!({
        "device_code": "a-device-code",
        "user_code": "ZQKH-RVTX",
        "verification_uri": "https://accounts.test/device",
        "expires_in": 600,
        "interval": 0,
    }))
    .into_response()
}

async fn token(State(accounts): State<Shared>, Form(form): Form<HashMap<String, String>>) -> Response {
    let mut accounts = accounts.lock().expect("the state");
    accounts.polls += 1;
    if accounts.polls == 1 {
        return refusal("authorization_pending");
    }
    if let Decision::Decline = accounts.decision {
        return refusal("access_denied");
    }
    let proof = form.get("proof").cloned().unwrap_or_default();
    let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(proof)
        .ok()
        .and_then(|bytes| <[u8; 64]>::try_from(bytes).ok())
        .map(|bytes| net::Signature::from_bytes(&bytes));
    let message = [PROOF_CONTEXT, b"a-device-code"].concat();
    let proved = accounts
        .key
        .zip(signature)
        .is_some_and(|(key, signature)| key.verify(&message, &signature).is_ok());
    if !proved {
        return refusal("invalid_grant");
    }
    Json(json!({
        "access_token": "octoii_token",
        "token_type": "Bearer",
        "expires_in": 3600,
        "scope": "machine",
        "address": ADDRESS,
    }))
    .into_response()
}

/// The service on the loopback, deciding as `decision`: its origin and its state.
async fn accounts(decision: Decision) -> (String, Shared) {
    let shared = Arc::new(Mutex::new(Accounts {
        decision,
        key: None,
        polls: 0,
    }));
    let router = Router::new()
        .route("/oauth/device_authorization", post(device_authorization))
        .route("/oauth/token", post(token))
        .with_state(shared.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let origin = format!("http://{}", listener.local_addr().expect("its address"));
    tokio::spawn(async move { axum::serve(listener, router).await });
    (origin, shared)
}

#[tokio::test]
async fn a_machine_shows_its_code_waits_on_its_person_and_is_added_at_its_address_once_it_proves_its_key() {
    let (origin, shared) = accounts(Decision::Add).await;
    let key = SecretKey::generate();
    let mut confirmed = None;
    let address = login::login(&origin, &key, "studio mac", |confirm| confirmed = Some(confirm))
        .await
        .expect("added");
    assert_eq!(address.to_string(), format!("{ADDRESS}/"));
    assert_eq!(
        confirmed,
        Some(Confirm {
            code: "ZQKH-RVTX".into(),
            at: "https://accounts.test/device".into(),
        })
    );
    let accounts = shared.lock().expect("the state");
    assert_eq!(accounts.key, Some(key.public()));
    assert_eq!(accounts.polls, 2);
}

#[tokio::test]
async fn a_machine_its_person_declines_is_told_so() {
    let (origin, _) = accounts(Decision::Decline).await;
    let declined = login::login(&origin, &SecretKey::generate(), "studio mac", |_| {}).await;
    assert!(matches!(declined, Err(LoginError::Denied(None))), "{declined:?}");
}

#[test]
fn the_relay_at_a_machines_address_is_the_address_itself_an_origin() {
    let at = |address: &str| login::relay_at(address).map(|relay| relay.to_string());
    assert_eq!(
        at("https://k3x7mq2vtd7a.octoiitunnel.com").as_deref(),
        Some("https://k3x7mq2vtd7a.octoiitunnel.com/")
    );
    assert_eq!(
        at("http://k3x7mq2vtd7a.localhost:8787").as_deref(),
        Some("http://k3x7mq2vtd7a.localhost:8787/")
    );
    for address in [
        "ftp://octoiitunnel.com",
        "https://octoiitunnel.com/home",
        "https://octoiitunnel.com?x",
        "https://a@octoiitunnel.com",
        "octoiitunnel.com",
        "https:",
    ] {
        assert_eq!(at(address), None, "{address}");
    }
}

#[test]
fn the_address_is_kept_in_the_home_and_forgotten() {
    let folder = tempfile::tempdir().expect("a folder");
    let home = Home::at(folder.path());
    home.prepare().expect("the Home");
    assert_eq!(login::address_of(&home).expect("read"), None);
    let address = login::relay_at(ADDRESS).expect("an address");
    login::keep_address(&home, &address).expect("kept");
    assert_eq!(login::address_of(&home).expect("read"), Some(address.clone()));
    assert_eq!(login::forget_address(&home).expect("forgotten"), Some(address));
    assert_eq!(login::address_of(&home).expect("read"), None);
    assert_eq!(login::forget_address(&home).expect("forgotten"), None);
}
