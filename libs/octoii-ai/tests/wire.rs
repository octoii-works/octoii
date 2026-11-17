//! What every provider's protocol shares over HTTP: a failure classified by status, headers and body, the waits a
//! response asks for, and server-sent events read as they come.

use std::sync::Arc;

use bytes::Bytes;
use futures::StreamExt;
use futures::future::BoxFuture;
use http::HeaderMap;
use octoii_ai::driver::{Call, Settings};
use octoii_ai::host::{Credential, Credentials, Unavailable};
use octoii_ai::transport::{Body, Transport, TransportError};
use octoii_ai::vocabulary::FailureKind;
use octoii_ai::wire::{self, FailureDetail};

/// A transport that answers every request with one response, and says what it was sent.
struct Answering {
    status: u16,
    headers: Vec<(&'static str, &'static str)>,
    body: &'static str,
    sent: std::sync::Mutex<Vec<http::Request<Bytes>>>,
}

impl Transport for Answering {
    fn send(&self, request: http::Request<Bytes>) -> BoxFuture<'static, Result<http::Response<Body>, TransportError>> {
        self.sent.lock().expect("held").push(request);
        let mut response = http::Response::builder().status(self.status);
        for (name, value) in &self.headers {
            response = response.header(*name, *value);
        }
        let body: Body = Box::pin(futures::stream::iter([Ok(Bytes::from_static(self.body.as_bytes()))]));
        let response = response.body(body).expect("a response");
        Box::pin(async move { Ok(response) })
    }
}

struct NoCredentials;

impl Credentials for NoCredentials {
    fn read(&self, _: &str) -> Result<Option<Credential>, Unavailable> {
        Ok(None)
    }

    fn keep(&self, _: &str, _: &Credential) -> Result<(), Unavailable> {
        Ok(())
    }

    fn forget(&self, _: &str) -> Result<(), Unavailable> {
        Ok(())
    }
}

fn call(transport: Arc<dyn Transport>) -> Call {
    Call {
        provider: "p".into(),
        settings: Settings {
            base_url: None,
            headers: vec![("x-proxy-key".into(), "k".into())],
        },
        credentials: Arc::new(NoCredentials),
        transport,
        files: None,
        now_ms: 1_000_000,
    }
}

fn answering(status: u16, headers: Vec<(&'static str, &'static str)>, body: &'static str) -> Arc<Answering> {
    Arc::new(Answering {
        status,
        headers,
        body,
        sent: std::sync::Mutex::new(Vec::new()),
    })
}

#[tokio::test]
async fn a_failure_is_its_statuss_kind_with_the_wait_its_headers_ask_for_unless_its_body_says_otherwise() {
    let limited = answering(429, vec![("retry-after", "2")], "slow down");
    let made = call(limited.clone());
    let response = wire::send(
        &made,
        http::Request::post("https://api.example.com/v1")
            .body(Bytes::new())
            .expect("a request"),
    )
    .await
    .expect("sent");
    let failure = wire::failure_of(&made, response, &wire::nothing).await;
    assert_eq!(
        (failure.kind, failure.status, failure.retry_after_ms),
        (FailureKind::RateLimit, Some(429), Some(2000))
    );
    assert_eq!(failure.message, "The provider answered 429: slow down");
    assert_eq!(
        limited.sent.lock().expect("held")[0].headers()["x-proxy-key"],
        "k",
        "the provider's headers go with each request"
    );
    // A body that says its own kind is believed; a quota used up is given again once its wait is over.
    let said = |_: &str| {
        Some(FailureDetail {
            kind: Some(FailureKind::Quota),
            message: Some("used up".into()),
            ..FailureDetail::default()
        })
    };
    let quota = answering(429, vec![("retry-after-ms", "1500")], "{}");
    let made = call(quota);
    let response = wire::send(
        &made,
        http::Request::post("https://api.example.com/v1")
            .body(Bytes::new())
            .expect("a request"),
    )
    .await
    .expect("sent");
    let failure = wire::failure_of(&made, response, &said).await;
    assert_eq!((failure.kind, failure.message.as_str()), (FailureKind::Quota, "used up"));
    assert_eq!(failure.reset_at_ms, Some(1_000_000 + 1500));
}

#[test]
fn statuses_are_kinds_and_waits_are_read_as_each_header_writes_them() {
    assert_eq!(wire::kind_of_status(401), FailureKind::Auth);
    assert_eq!(wire::kind_of_status(529), FailureKind::Server);
    assert_eq!(wire::kind_of_status(404), FailureKind::InvalidRequest);
    let headers = |pairs: &[(&'static str, &'static str)]| {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, value.parse().expect("a value"));
        }
        map
    };
    assert_eq!(wire::retry_after_ms(&headers(&[("retry-after", "1.2")]), 0), Some(1200));
    // An HTTP date, counted from now: 1994-11-06 08:49:37 UTC.
    let date = headers(&[("retry-after", "Sun, 06 Nov 1994 08:49:37 GMT")]);
    assert_eq!(wire::retry_after_ms(&date, 784_111_777_000 - 5000), Some(5000));
    assert_eq!(wire::retry_after_ms(&date, 784_111_777_000 + 5000), Some(0));
    let limits = headers(&[
        ("x-ratelimit-reset-requests", "1s"),
        ("x-ratelimit-remaining-requests", "5"),
        ("x-ratelimit-reset-tokens", "6m0s"),
        ("x-ratelimit-remaining-tokens", "0"),
    ]);
    assert_eq!(wire::rate_limit_reset_ms(&limits), Some(360_000), "the spent limit's reset");
    assert_eq!(wire::go_duration_ms("1.5s"), Some(1500.0));
    assert_eq!(wire::go_duration_ms("1h2m"), Some(3_720_000.0));
    assert_eq!(wire::go_duration_ms("soon"), None);
}

#[tokio::test]
async fn server_sent_events_are_read_as_they_come_across_pieces() {
    let body: Body = Box::pin(futures::stream::iter(
        ["event: a\ndata: {\"x\"", ":1}\n\nevent: b\ndata: 2\n\n"].map(|piece| Ok(Bytes::from(piece))),
    ));
    let events: Vec<_> = wire::events(body).map(|event| event.expect("an event")).collect().await;
    assert_eq!((events[0].event.as_str(), events[0].data.as_str()), ("a", "{\"x\":1}"));
    assert_eq!((events[1].event.as_str(), events[1].data.as_str()), ("b", "2"));
}
