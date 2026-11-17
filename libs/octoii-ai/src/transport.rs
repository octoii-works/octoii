//! How a driver's requests reach their provider: a transport the host gives, which takes an HTTP request whole and
//! gives back its response as its body streams — over the network by reqwest, or to a program of the host's own, as
//! an agent gateway is reached.

use std::pin::Pin;

use bytes::Bytes;
use futures::future::BoxFuture;
use futures::{Stream, StreamExt, TryStreamExt};

/// A response's body, as it comes.
pub type Body = Pin<Box<dyn Stream<Item = Result<Bytes, TransportError>> + Send>>;

/// A request that did not get through, or a body that broke off, in the transport's words.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct TransportError(pub String);

/// Where a driver's requests go.
pub trait Transport: Send + Sync {
    /// Sends `request`, giving its response whatever its status, its body as it comes.
    fn send(&self, request: http::Request<Bytes>) -> BoxFuture<'static, Result<http::Response<Body>, TransportError>>;
}

/// Requests over the network, by reqwest: HTTP/2 where the server speaks it, TLS by rustls.
#[derive(Debug, Clone, Default)]
pub struct Network {
    client: reqwest::Client,
}

impl Transport for Network {
    fn send(&self, request: http::Request<Bytes>) -> BoxFuture<'static, Result<http::Response<Body>, TransportError>> {
        let client = self.client.clone();
        Box::pin(async move {
            let request = reqwest::Request::try_from(request).map_err(|error| TransportError(error.to_string()))?;
            let response = client.execute(request).await.map_err(|error| TransportError(error.to_string()))?;
            let mut made = http::Response::builder().status(response.status());
            if let Some(headers) = made.headers_mut() {
                headers.clone_from(response.headers());
            }
            let body: Body = Box::pin(response.bytes_stream().map_err(|error| TransportError(error.to_string())).boxed());
            made.body(body).map_err(|error| TransportError(error.to_string()))
        })
    }
}
