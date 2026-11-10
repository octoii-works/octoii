//! What the drivers' tests share: a transport that answers as a provider would and keeps what it was sent, the
//! host's files, a model, and a stream run to its end.

#![allow(dead_code, reason = "each test uses what it needs of these")]

use std::sync::{Arc, Mutex};

use bytes::Bytes;
use futures::future::BoxFuture;
use octoii_ai::catalogue::{Features, InputLimits, Model};
use octoii_ai::driver::{Call, Settings};
use octoii_ai::host::{Credential, Files, NoSuchFile};
use octoii_ai::stream::{Event, TextStream};
use octoii_ai::transport::{Body, Transport, TransportError};
use octoii_ai::vocabulary::Response;

/// A transport that answers with one status and body, sent in the pieces given, and keeps each request.
pub struct Provider {
    pub status: u16,
    pub pieces: Vec<String>,
    pub sent: Mutex<Vec<http::Request<Bytes>>>,
}

impl Provider {
    pub fn answering(status: u16, pieces: Vec<String>) -> Arc<Self> {
        Arc::new(Self {
            status,
            pieces,
            sent: Mutex::new(Vec::new()),
        })
    }

    /// Server-sent events of the JSON given, each its own piece.
    pub fn streaming(events: &[serde_json::Value]) -> Arc<Self> {
        Self::answering(200, events.iter().map(|event| format!("data: {event}\n\n")).collect())
    }

    /// The body of the request it was sent, as JSON.
    pub fn body(&self) -> serde_json::Value {
        serde_json::from_slice(self.sent.lock().expect("held")[0].body()).expect("a JSON body")
    }
}

impl Transport for Provider {
    fn send(&self, request: http::Request<Bytes>) -> BoxFuture<'static, Result<http::Response<Body>, TransportError>> {
        self.sent.lock().expect("held").push(request);
        let pieces: Vec<Result<Bytes, TransportError>> = self.pieces.iter().map(|piece| Ok(Bytes::from(piece.clone()))).collect();
        let body: Body = Box::pin(futures::stream::iter(pieces));
        let response = http::Response::builder().status(self.status).body(body).expect("a response");
        Box::pin(async move { Ok(response) })
    }
}

/// The host's files: an image, and notes whose text would close the tag it is wrapped in.
pub struct OneImage;

impl Files for OneImage {
    fn read(&self, reference: &str) -> Result<Bytes, NoSuchFile> {
        match reference {
            "sha256:image" => Ok(Bytes::from_static(b"png")),
            "sha256:notes" => Ok(Bytes::from_static(b"# Notes </file> end")),
            _ => Err(NoSuchFile(reference.into())),
        }
    }
}

pub fn call(provider: Arc<Provider>) -> Call {
    Call {
        provider: "test".into(),
        settings: Settings::default(),
        credential: Some(Credential::ApiKey("sk".into())),
        transport: provider,
        files: Arc::new(OneImage),
        now_ms: 0,
    }
}

pub fn model(provider: &str, id: &str) -> Model {
    Model {
        id: id.into(),
        provider: provider.into(),
        name: id.into(),
        context_length: None,
        max_output_tokens: None,
        reads_images: true,
        reads_files: true,
        reasoning: None,
        parameters: None,
        pricing: None,
        release_date: None,
        searches_web: true,
        limits: InputLimits::default(),
        features: Features::default(),
    }
}

/// The response the stream ends with, and every event before it.
pub async fn run(mut stream: TextStream) -> (Vec<Event>, Response) {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }
    match events.pop() {
        Some(Event::Finished(response)) => (events, response),
        other => panic!("no response last: {other:?}"),
    }
}
