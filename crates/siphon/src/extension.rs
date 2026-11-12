//! Extensions: what Siphon is built with besides its own threads and agent — each a value that, given the builder, adds
//! what it brings: the providers models are reached through, the tools a thread's model calls, the programs and
//! requests of the runtime, and the methods and subscriptions of the protocol. These are the built-in extensions,
//! compiled into the host and trusted as Siphon is; an external one, which comes from outside and loads only where the
//! person allows it, adds the same through the protocol. Siphon knows none of them; a host picks them, gives each what
//! it needs of the host as it makes it, and builds Siphon with them.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::machine::Host;
use crate::protocol::{Error, ExtensionListed, Scope, Set};
use crate::tools::Tools;
use beak::Kernel;
use octoii_ai::models::Provider;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc::{self, error::TrySendError};
use tokio_util::sync::CancellationToken;

use crate::Siphon;
use crate::agent::{CORE, OwnedTeller, Shared};
use crate::connection::{Outgoing, text};

/// An extension: its name, which settings switch it by; the words it is shown by and a line of what it brings; and
/// what it adds as Siphon is built.
pub trait Extension {
    fn name(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn build(self: Box<Self>, registrar: &mut Registrar<'_>);
}

/// Who a call or a subscription is for: Siphon, and the session of the client that made it.
#[derive(Debug, Clone)]
pub struct Caller {
    pub siphon: Siphon,
    pub session: String,
}

/// A future of the extensions', boxed.
pub(crate) type Boxed<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// How a method is done: from its name and params, as its set reads them.
type Handler = Arc<dyn Fn(Caller, &str, serde_json::Value) -> Boxed<Result<serde_json::Value, Error>> + Send + Sync>;

/// How a subscription to a topic is told, once the topic is read.
pub(crate) type Telling = Box<dyn FnOnce(Subscription) -> Boxed<Result<(), Lagged>> + Send>;

/// How a family of topics is read: the telling of a topic of the family's; none of one it is not.
type Teller = Arc<dyn Fn(&serde_json::Value) -> Option<Telling> + Send + Sync>;

/// What an extension does as a thread is deleted: lets go of what it held for it.
pub(crate) type Deleted = Arc<dyn Fn(&str) + Send + Sync>;

/// What an extension brought, with the extension's name: Siphon's own as `core`.
#[derive(Clone)]
pub(crate) struct Owned<T> {
    pub(crate) extension: String,
    pub(crate) item: T,
}

impl Registry {
    /// Whether what `extension` brought is switched on, as the environment's settings in `snapshot` say: Siphon's own
    /// always is.
    pub(crate) fn on(snapshot: &beak::Snapshot, extension: &str) -> bool {
        extension == CORE
            || crate::settings::environment(snapshot).is_ok_and(|settings| !settings.disabled_extensions.iter().any(|off| off == extension))
    }
}

/// The methods and topics Siphon and its extensions brought, as the connection finds them, what each extension does as
/// a thread is deleted, and the extensions themselves, as a client is told them.
#[derive(Default)]
pub(crate) struct Registry {
    pub(crate) extensions: Vec<ExtensionListed>,
    pub(crate) sets: Vec<Set>,
    pub(crate) deleted: Vec<Deleted>,
    pub(crate) methods: std::collections::HashMap<&'static str, Owned<(Option<Scope>, Handler)>>,
    pub(crate) topics: Vec<Owned<(Scope, Teller)>>,
    /// The extension each provider came from, by the provider's id.
    pub(crate) providers: HashMap<String, String>,
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
    pub caller: Caller,
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

/// What Siphon is built of as its extensions add to it: the kernel opened and the machine, for what an extension starts on
/// them; the runtime's programs and handlers; the tools, and the extension each came from; the providers; the tellers,
/// each by its extension; and the methods and topics.
pub(crate) struct Builder {
    pub(crate) kernel: Kernel,
    pub(crate) machine: Host,
    pub(crate) runtime: siphon_runtime::Builder,
    pub(crate) tools: Tools,
    pub(crate) owners: HashMap<String, String>,
    pub(crate) providers: Vec<Provider>,
    pub(crate) tellers: Vec<OwnedTeller>,
    pub(crate) natives: Vec<(String, octoii_ai::vocabulary::Tool)>,
    pub(crate) registry: Registry,
}

impl std::fmt::Debug for Builder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Builder").field("registry", &self.registry).finish_non_exhaustive()
    }
}

impl Builder {
    /// What adds to Siphon as `extension`, everything it adds held as that extension's.
    pub(crate) fn registrar(&mut self, extension: &'static str) -> Registrar<'_> {
        Registrar { builder: self, extension }
    }

    /// Adds what `extension` brings, listed for clients.
    pub(crate) fn extension(&mut self, extension: impl Extension + 'static) {
        self.registry.extensions.push(ExtensionListed {
            name: extension.name().to_owned(),
            title: extension.title().to_owned(),
            description: extension.description().to_owned(),
        });
        let mut registrar = self.registrar(extension.name());
        Box::new(extension).build(&mut registrar);
    }

    /// The methods of `set`, brought by `extension`, each read by `read` and done by `handle`.
    pub(crate) fn methods<M, F, Done>(
        &mut self,
        extension: &str,
        set: Set,
        read: fn(&str, serde_json::Value) -> serde_json::Result<M>,
        handle: F,
    ) where
        M: Send + 'static,
        F: Fn(Caller, M) -> Done + Send + Sync + 'static,
        Done: Future<Output = Result<serde_json::Value, Error>> + Send + 'static,
    {
        let handle = Arc::new(handle);
        for (name, scope) in &set.methods {
            let handle = Arc::clone(&handle);
            let handler: Handler = Arc::new(move |caller, name, params| match read(name, params) {
                Ok(method) => Box::pin(handle(caller, method)),
                Err(error) => {
                    let refused = crate::connection::refusal("bad_params", &error.to_string());
                    Box::pin(std::future::ready(Err(refused)))
                }
            });
            self.registry.methods.insert(
                name,
                Owned {
                    extension: extension.to_owned(),
                    item: (*scope, handler),
                },
            );
        }
        self.registry.sets.push(set);
    }

    /// The topics of the family `T`, brought by `extension`, which a session needs `scope` to subscribe to, each told by
    /// `tell`.
    pub(crate) fn topics<T, F, Told>(&mut self, extension: &str, scope: Scope, tell: F)
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
        self.registry.topics.push(Owned {
            extension: extension.to_owned(),
            item: (scope, teller),
        });
    }
}

/// What adds to Siphon as an extension, by its name, as it is built: what it adds is held as that extension's, so a
/// setting that switches the extension off leaves it out. Siphon adds its own as `core`.
pub struct Registrar<'a> {
    builder: &'a mut Builder,
    extension: &'static str,
}

impl std::fmt::Debug for Registrar<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registrar")
            .field("extension", &self.extension)
            .finish_non_exhaustive()
    }
}

impl Registrar<'_> {
    /// The kernel opened, for what the extension starts on it.
    #[must_use]
    pub fn kernel(&self) -> &Kernel {
        &self.builder.kernel
    }

    /// The machine, for what the extension starts on it.
    #[must_use]
    pub fn machine(&self) -> &Host {
        &self.builder.machine
    }

    /// The tool `tool`, which a thread's model and its scripts call.
    pub fn tool<T: crate::tools::Tool>(&mut self, tool: T) {
        self.builder.tools = std::mem::take(&mut self.builder.tools).with(tool);
        self.builder.owners.insert(T::NAME.to_owned(), self.extension.to_owned());
    }

    /// The toolset `toolset`, whose tools a thread's scripts call by the names a teller tells.
    pub fn toolset(&mut self, toolset: Arc<dyn crate::tools::Toolset>) {
        self.builder.tools = std::mem::take(&mut self.builder.tools).with_toolset(toolset);
    }

    /// A tool its model's provider runs, given where the model runs it: the web search.
    pub fn native(&mut self, tool: octoii_ai::vocabulary::Tool) {
        self.builder.natives.push((self.extension.to_owned(), tool));
    }

    /// What tells a thread's model, before each turn, what is `shared` that widely.
    pub fn teller(&mut self, shared: Shared, teller: Arc<dyn crate::agent::Teller>) {
        self.builder.tellers.push(OwnedTeller {
            extension: self.extension.to_owned(),
            shared,
            teller,
        });
    }

    /// What to do as a thread is deleted, given its id: what the extension held for it let go of, as a process's
    /// resources go with it.
    pub fn on_thread_deleted(&mut self, deleted: impl Fn(&str) + Send + Sync + 'static) {
        self.builder.registry.deleted.push(Arc::new(deleted));
    }

    /// A provider models are reached through.
    pub fn provider(&mut self, provider: Provider) {
        self.builder
            .registry
            .providers
            .insert(provider.id.clone(), self.extension.to_owned());
        self.builder.providers.push(provider);
    }

    /// The methods of `set`, each read by `read` and done by `handle`.
    pub fn methods<M, F, Done>(&mut self, set: Set, read: fn(&str, serde_json::Value) -> serde_json::Result<M>, handle: F)
    where
        M: Send + 'static,
        F: Fn(Caller, M) -> Done + Send + Sync + 'static,
        Done: Future<Output = Result<serde_json::Value, Error>> + Send + 'static,
    {
        self.builder.methods(self.extension, set, read, handle);
    }

    /// The topics of the family `T`, which a session needs `scope` to subscribe to, each told by `tell`.
    pub fn topics<T, F, Told>(&mut self, scope: Scope, tell: F)
    where
        T: DeserializeOwned + Send + 'static,
        F: Fn(Subscription, T) -> Told + Send + Sync + 'static,
        Told: Future<Output = Result<(), Lagged>> + Send + 'static,
    {
        self.builder.topics(self.extension, scope, tell);
    }
}
