//! Plugins: what Siphon is built with besides its own threads and agent — each a value that, given the builder, adds
//! what it brings: the providers models are reached through, the tools a thread's model calls, the programs and
//! requests of the runtime, and the methods and subscriptions of the protocol. Siphon knows none of them; a host picks
//! them, gives each what it needs of the host as it makes it, and builds Siphon with them, as Bevy's plugins build its
//! app.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use beak::Kernel;
use octoii_ai::models::Provider;
use serde::Serialize;
use serde::de::DeserializeOwned;
use siphon_machine::Host;
use siphon_protocol::{Error, Scope, Set};
use siphon_tools::Tools;
use tokio::sync::mpsc::{self, error::TrySendError};
use tokio_util::sync::CancellationToken;

use crate::Siphon;
use crate::connection::{Outgoing, text};

/// A plugin: its name, and what it adds as Siphon is built.
pub trait Plugin {
    fn name(&self) -> &'static str;
    fn build(self: Box<Self>, builder: &mut Builder);
}

/// Who a call or a subscription is for: Siphon, and the session of the client that made it.
#[derive(Debug, Clone)]
pub struct Context {
    pub siphon: Siphon,
    pub session: String,
}

/// A future of the plugins', boxed.
pub(crate) type Boxed<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// How a method is done: from its name and params, as its set reads them.
type Handler = Arc<dyn Fn(Context, &str, serde_json::Value) -> Boxed<Result<serde_json::Value, Error>> + Send + Sync>;

/// How a subscription to a topic is told, once the topic is read.
pub(crate) type Telling = Box<dyn FnOnce(Subscription) -> Boxed<Result<(), Lagged>> + Send>;

/// How a family of topics is read: the telling of a topic of the family's; none of one it is not.
type Teller = Arc<dyn Fn(&serde_json::Value) -> Option<Telling> + Send + Sync>;

/// The methods and topics Siphon and its plugins brought, as the connection finds them.
#[derive(Default)]
pub(crate) struct Registry {
    pub(crate) sets: Vec<Set>,
    pub(crate) methods: std::collections::HashMap<&'static str, (Option<Scope>, Handler)>,
    pub(crate) topics: Vec<(Scope, Teller)>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("methods", &self.methods.keys())
            .finish_non_exhaustive()
    }
}

/// A subscription as its teller holds it: who it is for, its id, where its events go, and what ends it.
#[derive(Debug, Clone)]
pub struct Subscription {
    pub context: Context,
    pub sink: Sink,
    pub stop: CancellationToken,
}

/// Where a subscription's events go: its client's queue, each event as its subscription's.
#[derive(Debug, Clone)]
pub struct Sink {
    pub(crate) id: String,
    pub(crate) outgoing: mpsc::Sender<Outgoing>,
}

/// A client that keeps up no longer, or is gone: its queue had no room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Full;

/// A subscription that fell behind and was let go: the topic to subscribe to again from where it stood.
#[derive(Debug, Clone, PartialEq)]
pub struct Lagged(pub serde_json::Value);

impl Sink {
    /// The subscription's id, which its events carry.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Sends `event` where the client's queue has room.
    ///
    /// # Errors
    /// `Full` where it has none, or the client is gone: the subscription is to end.
    pub fn offer(&self, event: &impl Serialize) -> Result<(), Full> {
        match self.outgoing.try_send(text(event)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_) | TrySendError::Closed(_)) => Err(Full),
        }
    }

    /// Sends `event` with the bytes `payload` it carries, as one binary frame, where the client's queue has room.
    ///
    /// # Errors
    /// `Full` where it has none, or the client is gone.
    pub fn offer_with(&self, event: &impl Serialize, payload: &[u8]) -> Result<(), Full> {
        let header = serde_json::to_vec(event).unwrap_or_default();
        let length = u32::try_from(header.len()).unwrap_or(u32::MAX);
        let mut frame = bytes::BytesMut::with_capacity(4 + header.len() + payload.len());
        frame.extend_from_slice(&length.to_be_bytes());
        frame.extend_from_slice(&header);
        frame.extend_from_slice(payload);
        match self.outgoing.try_send(Outgoing::Binary(frame.freeze())) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_) | TrySendError::Closed(_)) => Err(Full),
        }
    }
}

/// What Siphon is built of as its plugins add to it: the kernel opened and the machine, for what a plugin starts on
/// them; the runtime's programs and handlers; the tools; the providers; the methods and topics.
pub struct Builder {
    pub kernel: Kernel,
    pub machine: Host,
    pub(crate) runtime: siphon_runtime::Builder,
    pub(crate) tools: Tools,
    pub(crate) providers: Vec<Provider>,
    pub(crate) registry: Registry,
}

impl std::fmt::Debug for Builder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Builder").field("registry", &self.registry).finish_non_exhaustive()
    }
}

impl Builder {
    /// The runtime's builder, as `add` leaves it: programs, handlers and requests the person answers added.
    pub fn runtime(&mut self, add: impl FnOnce(siphon_runtime::Builder) -> siphon_runtime::Builder) {
        let runtime = std::mem::replace(&mut self.runtime, siphon_runtime::Runtime::builder());
        self.runtime = add(runtime);
    }

    /// The tools of a thread's model and its scripts, as `add` leaves them.
    pub fn tools(&mut self, add: impl FnOnce(Tools) -> Tools) {
        self.tools = add(std::mem::take(&mut self.tools));
    }

    /// A provider models are reached through.
    pub fn provider(&mut self, provider: Provider) {
        self.providers.push(provider);
    }

    /// The methods of `set`, each read by `read` and done by `handle`.
    pub fn methods<M, F, Done>(&mut self, set: Set, read: fn(&str, serde_json::Value) -> serde_json::Result<M>, handle: F)
    where
        M: Send + 'static,
        F: Fn(Context, M) -> Done + Send + Sync + 'static,
        Done: Future<Output = Result<serde_json::Value, Error>> + Send + 'static,
    {
        let handle = Arc::new(handle);
        for (name, scope) in &set.methods {
            let handle = Arc::clone(&handle);
            let handler: Handler = Arc::new(move |context, name, params| match read(name, params) {
                Ok(method) => Box::pin(handle(context, method)),
                Err(error) => {
                    let refused = crate::connection::refusal("bad_params", &error.to_string());
                    Box::pin(std::future::ready(Err(refused)))
                }
            });
            self.registry.methods.insert(name, (*scope, handler));
        }
        self.registry.sets.push(set);
    }

    /// The topics of the family `T`, which a session needs `scope` to subscribe to, each told by `tell`.
    pub fn topics<T, F, Told>(&mut self, scope: Scope, tell: F)
    where
        T: DeserializeOwned + Send + 'static,
        F: Fn(Subscription, T) -> Told + Send + Sync + 'static,
        Told: Future<Output = Result<(), Lagged>> + Send + 'static,
    {
        let tell = Arc::new(tell);
        let teller: Teller = Arc::new(move |topic| {
            let read = T::deserialize(topic).ok()?;
            let tell = Arc::clone(&tell);
            Some(Box::new(move |subscription| -> Boxed<Result<(), Lagged>> {
                Box::pin(tell(subscription, read))
            }))
        });
        self.registry.topics.push((scope, teller));
    }

    /// Adds what `plugin` brings.
    pub fn plugin(&mut self, plugin: impl Plugin + 'static) -> &mut Self {
        Box::new(plugin).build(self);
        self
    }
}
