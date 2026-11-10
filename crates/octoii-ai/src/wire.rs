//! What every provider's protocol shares over HTTP: a request sent through the call's transport with its provider's
//! headers; a failure classified by its status, its headers and what its body says; server-sent events read as they
//! come; JSON read whole.

use bytes::{Bytes, BytesMut};
use eventsource_stream::Eventsource;
use futures::{Stream, StreamExt};
use http::HeaderMap;

use crate::driver::Call;
use crate::transport::Body;
use crate::vocabulary::{Failure, FailureKind};

/// The most of a body read whole, as a failure's or a listing's.
const BODY_BYTES_MAX: usize = 16 << 20;

/// Sends `request` through the call's transport, the provider's own headers in place of the driver's of their names,
/// and gives its response whatever its status.
///
/// # Errors
/// A `transport` failure where it does not get through.
pub async fn send(call: &Call, mut request: http::Request<Bytes>) -> Result<http::Response<Body>, Failure> {
    for (name, value) in &call.settings.headers {
        let (Ok(name), Ok(value)) = (
            http::HeaderName::try_from(name.as_str()),
            http::HeaderValue::try_from(value.as_str()),
        ) else {
            continue;
        };
        request.headers_mut().insert(name, value);
    }
    let host = request.uri().host().unwrap_or("the provider").to_owned();
    call.transport.send(request).await.map_err(|error| {
        Failure::new(
            FailureKind::Transport,
            format!("The request to {host} did not get through: {error}"),
        )
    })
}

/// The whole of a body, to `BODY_BYTES_MAX`.
///
/// # Errors
/// A `transport` failure where it breaks off.
pub async fn read_whole(mut body: Body) -> Result<Bytes, Failure> {
    let mut whole = BytesMut::new();
    while let Some(piece) = body.next().await {
        let piece = piece.map_err(|error| Failure::new(FailureKind::Transport, format!("The response broke off: {error}")))?;
        whole.extend_from_slice(&piece[..piece.len().min(BODY_BYTES_MAX.saturating_sub(whole.len()))]);
        if whole.len() >= BODY_BYTES_MAX {
            break;
        }
    }
    Ok(whole.freeze())
}

/// The JSON of a response that is ok, as `T`; the failure it stands for where it is not.
///
/// # Errors
/// The failure of a response that is not ok, or an `invalid_response` one where its JSON is no `T`.
pub async fn json<T: serde::de::DeserializeOwned>(call: &Call, response: http::Response<Body>, details: Details<'_>) -> Result<T, Failure> {
    if !response.status().is_success() {
        return Err(failure_of(call, response, details).await);
    }
    let body = read_whole(response.into_body()).await?;
    serde_json::from_slice(&body).map_err(|error| {
        Failure::new(
            FailureKind::InvalidResponse,
            format!("The provider's answer is no JSON it should be: {error}"),
        )
    })
}

/// What a protocol reads of a failure from its body: the provider's own kind, words, code and waits, each where it
/// says; and, for a quota used up, when it is given again and the window it is counted over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Said {
    pub kind: Option<FailureKind>,
    pub message: Option<String>,
    pub code: Option<String>,
    pub retry_after_ms: Option<u64>,
    pub reset_at_ms: Option<i64>,
    pub window_minutes: Option<u32>,
}

/// How a protocol reads what a failure's body says.
pub type Details<'a> = &'a (dyn Fn(&str) -> Option<Said> + Sync);

/// What a body says of nothing.
#[must_use]
pub fn nothing(_: &str) -> Option<Said> {
    None
}

/// The failure a response that is not ok stands for, its body read: its kind by its status unless the body says;
/// the wait its headers ask for, else the body's, else for a rate limit the reset of its spent limits; a quota used up
/// given again when the body says, else once that wait is over.
pub async fn failure_of(call: &Call, response: http::Response<Body>, details: Details<'_>) -> Failure {
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = read_whole(response.into_body()).await.unwrap_or_default();
    let body = String::from_utf8_lossy(&body);
    let said = details(&body).unwrap_or_default();
    let kind = said.kind.unwrap_or_else(|| kind_of_status(status));
    let wait = retry_after_ms(&headers, call.now_ms).or(said.retry_after_ms);
    let reset_at_ms = said.reset_at_ms.or_else(|| {
        (kind == FailureKind::Quota)
            .then_some(wait)
            .flatten()
            .map(|wait| call.now_ms + i64::try_from(wait).unwrap_or(i64::MAX))
    });
    Failure {
        kind,
        message: said.message.unwrap_or_else(|| {
            if body.is_empty() {
                format!("The provider answered {status}")
            } else {
                format!("The provider answered {status}: {body}")
            }
        }),
        code: said.code,
        status: Some(status),
        retry_after_ms: wait.or_else(|| (kind == FailureKind::RateLimit).then(|| rate_limit_reset_ms(&headers)).flatten()),
        reset_at_ms,
        window_minutes: said.window_minutes,
    }
}

/// The kind of failure an HTTP status stands for, before a protocol reads the provider's own codes.
#[must_use]
pub fn kind_of_status(status: u16) -> FailureKind {
    match status {
        401 => FailureKind::Auth,
        402 => FailureKind::Quota,
        403 => FailureKind::Forbidden,
        408 => FailureKind::Timeout,
        413 => FailureKind::PayloadTooLarge,
        429 => FailureKind::RateLimit,
        500.. => FailureKind::Server,
        _ => FailureKind::InvalidRequest,
    }
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok()).map(str::trim)
}

/// How long a response asks to wait before trying again: its `retry-after-ms`, else its `retry-after` in seconds or as
/// an HTTP date, counted from `now_ms` and never below 0.
#[must_use]
pub fn retry_after_ms(headers: &HeaderMap, now_ms: i64) -> Option<u64> {
    if let Some(ms) = header(headers, "retry-after-ms").and_then(delay) {
        return Some(rounded(ms));
    }
    let text = header(headers, "retry-after")?;
    if let Some(seconds) = delay(text) {
        return Some(rounded(seconds * 1000.0));
    }
    let date = httpdate::parse_http_date(text).ok()?;
    let date_ms = i64::try_from(date.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis()).ok()?;
    Some(u64::try_from((date_ms - now_ms).max(0)).unwrap_or(0))
}

/// A delay of whole or decimal units; none for another text.
fn delay(text: &str) -> Option<f64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit() || byte == b'.') {
        return None;
    }
    text.parse().ok()
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a wait of milliseconds, never below 0"
)]
fn rounded(ms: f64) -> u64 {
    ms.max(0.0).ceil() as u64
}

/// How long a rate limit lasts by its `x-ratelimit-reset-*` headers: the latest reset of the limits whose
/// `x-ratelimit-remaining-*` say they are spent, or of all of them where none says so; none where it gives none.
#[must_use]
pub fn rate_limit_reset_ms(headers: &HeaderMap) -> Option<u64> {
    let (mut latest_spent, mut latest): (Option<f64>, Option<f64>) = (None, None);
    for (name, value) in headers {
        let Some(limit) = name.as_str().strip_prefix("x-ratelimit-reset-") else {
            continue;
        };
        let Some(ms) = value.to_str().ok().and_then(|value| go_duration_ms(value.trim())) else {
            continue;
        };
        latest = Some(latest.map_or(ms, |most| most.max(ms)));
        let remaining = header(headers, &format!("x-ratelimit-remaining-{limit}")).and_then(|left| left.parse::<f64>().ok());
        if remaining.is_some_and(|left| left <= 0.0) {
            latest_spent = Some(latest_spent.map_or(ms, |most| most.max(ms)));
        }
    }
    latest_spent.or(latest).map(rounded)
}

/// A duration as Go writes it — `6m0s`, `1.5s`, `20ms` — in milliseconds; none for another text.
#[must_use]
pub fn go_duration_ms(text: &str) -> Option<f64> {
    const UNITS: [(&str, f64); 7] = [
        ("ms", 1.0),
        ("us", 0.001),
        ("µs", 0.001),
        ("ns", 0.000_001),
        ("h", 3_600_000.0),
        ("m", 60_000.0),
        ("s", 1000.0),
    ];
    if text.is_empty() {
        return None;
    }
    let mut rest = text;
    let mut total = 0.0;
    while !rest.is_empty() {
        let number_end = rest
            .find(|character: char| !character.is_ascii_digit() && character != '.')
            .unwrap_or(rest.len());
        let amount: f64 = rest[..number_end].parse().ok()?;
        rest = &rest[number_end..];
        let (unit, ms) = UNITS.iter().find(|(unit, _)| rest.starts_with(unit))?;
        total += amount * ms;
        rest = &rest[unit.len()..];
    }
    Some(total)
}

/// The server-sent events of a body, as they come.
pub fn events(body: Body) -> impl Stream<Item = Result<eventsource_stream::Event, Failure>> + Send {
    body.eventsource()
        .map(|event| event.map_err(|error| Failure::new(FailureKind::Transport, format!("The response's events broke off: {error}"))))
}
