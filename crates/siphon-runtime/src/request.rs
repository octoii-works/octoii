//! A request: what a process asks the outside — a model's reply, a command run, a person's choice — by a name, with a
//! payload, and keyed so its answer comes back as mail of the same key. Each is answered once: by the handler of its
//! name, in a task of its own, or by a person; a handler that fails is answered for, and one cut off by a stop, or by
//! the host's end, is answered interrupted, for the process that asked to decide.

use bytes::Bytes;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio_util::sync::CancellationToken;

use crate::runtime::Runtime;

/// A request of one name, its payload the value itself, and what answers it.
pub trait Request: Serialize + DeserializeOwned + Send + 'static {
    /// The request's name: `siphon.` and what it asks, as `siphon.model`.
    const NAME: &'static str;
    type Answer: Serialize + DeserializeOwned + Send + 'static;
}

/// The name of the mail that carries an answer given.
pub(crate) const ANSWERED: &str = "answered";
/// The name of the mail that says the request failed, its payload why.
pub(crate) const FAILED: &str = "failed";
/// The name of the mail that says the request was cut off with nothing to give.
pub(crate) const INTERRUPTED: &str = "interrupted";

/// How a request was answered, as its process reads the mail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer<A> {
    Given(A),
    /// It failed, saying why.
    Failed(String),
    /// It was cut off with nothing to give — by a stop, or by the host's end, after which its handler may have acted.
    Interrupted,
}

/// Why a handler gave no answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unanswered {
    #[error("{0}")]
    Failed(String),
    #[error("interrupted")]
    Interrupted,
}

impl From<anyhow::Error> for Unanswered {
    fn from(error: anyhow::Error) -> Self {
        Self::Failed(format!("{error:#}"))
    }
}

/// A request in its handler's hands: what it asks, who asked, and its cancellation — a stop cuts it off, and its
/// handler answers at once with what has come, or `Unanswered::Interrupted`.
#[derive(Debug)]
pub struct Asked<R> {
    pub payload: R,
    /// The process that asked, by its path.
    pub process: Bytes,
    pub key: String,
    pub cancelled: CancellationToken,
    /// The runtime the request is answered on, for what its handler reads and commits.
    pub runtime: Runtime,
}

impl<R> Asked<R> {
    /// Publishes `data` on `topic` to the request's stream: what streams while it is answered, kept till its handler
    /// ends for one who follows the topic later.
    pub fn publish(&self, topic: &str, data: Bytes) {
        self.runtime.streams().publish(topic, &self.key, data);
    }
}

/// The mail that carries `answer`.
pub(crate) fn mail_of<A: Serialize>(answer: &Result<A, Unanswered>) -> (&'static str, Bytes) {
    match answer {
        Ok(given) => match serde_json::to_vec(given) {
            Ok(payload) => (ANSWERED, payload.into()),
            Err(error) => (FAILED, format!("the answer could not be written: {error}").into()),
        },
        Err(Unanswered::Failed(why)) => (FAILED, Bytes::from(why.clone())),
        Err(Unanswered::Interrupted) => (INTERRUPTED, Bytes::new()),
    }
}

/// The answer mail named `name` with `payload` carries.
pub(crate) fn answer_of<A: DeserializeOwned>(name: &[u8], payload: &[u8]) -> Answer<A> {
    if name == ANSWERED.as_bytes() {
        return serde_json::from_slice(payload).map_or_else(
            |error| Answer::Failed(format!("the answer could not be read: {error}")),
            Answer::Given,
        );
    }
    if name == INTERRUPTED.as_bytes() {
        return Answer::Interrupted;
    }
    Answer::Failed(String::from_utf8_lossy(payload).into_owned())
}
