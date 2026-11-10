//! What the runtime is built of before it starts: the programs its processes run, the handlers of its requests, and
//! the requests a person answers — each erased to what the runtime calls.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use beak::{Beak, Kernel};
use bytes::Bytes;
use schemars::JsonSchema;
use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;
use tokio_util::task::{AbortOnDropHandle, TaskTracker};

use super::{Options, Runtime, Shared, dispatcher, events};
use crate::person::AskedOfPerson;
use crate::program::{Program, Runs};
use crate::request::{Asked, Request, Unanswered, mail_of};
use crate::streams::Streams;

pub(super) type Answering = Pin<Box<dyn Future<Output = (&'static str, Bytes)> + Send>>;

/// A handler of any request, as the runtime calls it: the payload as bytes in, the answer's mail out.
pub(super) trait Handles: Send + Sync {
    fn handle(&self, payload: &[u8], process: Bytes, key: String, cancelled: CancellationToken, streams: Streams) -> Answering;
}

struct Handler<R, F> {
    act: F,
    request: std::marker::PhantomData<fn(R)>,
}

impl<R, F, A> Handles for Handler<R, F>
where
    R: Request,
    F: Fn(Asked<R>) -> A + Send + Sync,
    A: Future<Output = Result<R::Answer, Unanswered>> + Send + 'static,
{
    fn handle(&self, payload: &[u8], process: Bytes, key: String, cancelled: CancellationToken, streams: Streams) -> Answering {
        let payload = match serde_json::from_slice::<R>(payload) {
            Ok(payload) => payload,
            Err(error) => {
                let why = Unanswered::Failed(format!("the request could not be read: {error}"));
                return Box::pin(std::future::ready(mail_of::<R::Answer>(&Err(why))));
            }
        };
        let acting = (self.act)(Asked {
            payload,
            process,
            key,
            cancelled,
            streams,
        });
        Box::pin(async move {
            // In a task of its own, so a handler that panics is answered for.
            let answered = match AbortOnDropHandle::new(tokio::spawn(acting)).await {
                Ok(answered) => answered,
                Err(error) => Err(Unanswered::Failed(format!("the handler panicked: {error}"))),
            };
            mail_of(&answered)
        })
    }
}

/// The schema a person's answer to a request fits, from its payload.
pub(super) type Schema = Arc<dyn Fn(&[u8]) -> anyhow::Result<serde_json::Value> + Send + Sync>;

/// The programs, handlers and requests a person answers, gathered before the runtime starts.
#[derive(Default)]
pub struct Builder {
    programs: Vec<(&'static str, beak::Program, Arc<dyn Runs>)>,
    handlers: HashMap<Bytes, Arc<dyn Handles>>,
    person: HashMap<Bytes, Schema>,
}

impl std::fmt::Debug for Builder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Builder").finish_non_exhaustive()
    }
}

impl Builder {
    /// Runs the processes of `P`'s kind with `program`.
    #[must_use]
    pub fn program<P: Program>(mut self, program: P) -> Self {
        let registered = beak::Program {
            version: P::VERSION,
            stop_handler: P::STOP_HANDLER,
            retries: P::RETRIES.count,
            retry_delay_min_ms: P::RETRIES.delay_min_ms,
            retry_delay_max_ms: P::RETRIES.delay_max_ms,
        };
        self.programs.push((P::KIND, registered, Arc::new(program)));
        self
    }

    /// Answers every request of `R` with `act`, in a task of its own.
    #[must_use]
    pub fn handle<R, F, A>(mut self, act: F) -> Self
    where
        R: Request,
        F: Fn(Asked<R>) -> A + Send + Sync + 'static,
        A: Future<Output = Result<R::Answer, Unanswered>> + Send + 'static,
    {
        let handler = Handler {
            act,
            request: std::marker::PhantomData,
        };
        self.handlers.insert(Bytes::from_static(R::NAME.as_bytes()), Arc::new(handler));
        self
    }

    /// Has every request of `R` answered by a person.
    #[must_use]
    pub fn ask_person<R>(mut self) -> Self
    where
        R: AskedOfPerson,
        R::Answer: JsonSchema,
    {
        let schema: Schema = Arc::new(|payload| Ok(serde_json::from_slice::<R>(payload)?.answer_schema()));
        self.person.insert(Bytes::from_static(R::NAME.as_bytes()), schema);
        self
    }

    /// Opens the kernel as `options` say, registers the programs and starts the runtime's tasks on the current Tokio
    /// runtime: the requests made before it opened are told first, a handler's answered interrupted.
    ///
    /// # Errors
    /// The volume's failure, as another holding its directory.
    pub fn start(self, beak: &Beak, options: Options) -> beak::Result<Runtime> {
        let (kernel, events) = Kernel::open(
            beak,
            &beak::Options {
                directory: options.directory.clone(),
                time: options.clock.now_ms(),
            },
        )?;
        let mut programs = HashMap::new();
        for (kind, registered, program) in self.programs {
            kernel.register(kind.as_bytes(), registered)?;
            programs.insert(Bytes::from_static(kind.as_bytes()), program);
        }
        let base = kernel.usage()?;
        let shared = Arc::new(Shared {
            committed: watch::Sender::new(kernel.snapshot()?.seq()),
            kernel,
            clock: options.clock,
            programs,
            handlers: self.handlers,
            person: self.person,
            streams: Streams::start(),
            dispatch: Notify::new(),
            shutdown: CancellationToken::new(),
        });
        let tasks = TaskTracker::new();
        tasks.spawn(dispatcher::run_dispatcher(Arc::clone(&shared)));
        tasks.spawn(events::handle_events(Arc::clone(&shared), events, options.collection, base));
        tasks.close();
        Ok(Runtime { shared, tasks })
    }
}
