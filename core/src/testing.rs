//! What tests of Octoii, of its extensions and of a host build on: Octoii's setup on a volume in memory that asks no
//! model; a client of a connection — its calls made, its answers and events read, a binary frame's event with the
//! bytes it carries; and model calls run as a turn runs them, for an extension's tests of its tools.

use std::sync::Arc;
use std::time::Duration;

use crate::protocol::{Command, CommandReply, Machine, Scope};
use bytes::Bytes;
use octoii_ai::host::{Credential, Credentials, Unavailable};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::connection::{Access, Outgoing, call, serve};
use crate::{Octoii, Setup};

/// The runtime used by in-memory machine fixtures.
pub use octoii_siphon_lib::Runtime;

/// Opens a runtime in memory for a machine fixture.
///
/// # Errors
/// The runtime builder or in-memory volume could not be opened.
pub fn memory_runtime() -> octoii_siphon_lib::Result<Runtime> {
    start_runtime(None, |_, _| Ok(()))
}

/// Opens a test volume and registers its execution before starting the local machine.
pub(crate) fn start_runtime(
    directory: Option<&std::path::Path>,
    register: impl FnOnce(&mut octoii_siphon_lib::Builder, &octoii_siphon_lib::RuntimeHandle) -> octoii_siphon_lib::Result<()>,
) -> octoii_siphon_lib::Result<Runtime> {
    let runtime = Runtime::open(directory, None)?;
    let handle = runtime.runtime_handle();
    let mut builder = octoii_siphon_lib::Builder::new(&handle)?;
    builder.local_machine()?;
    register(&mut builder, &handle)?;
    builder.start()?;
    Ok(runtime)
}

/// Explicit instructions for runtime tests; production instructions belong to the base extension.
pub const PROMPTS: crate::agent::Prompts = crate::agent::Prompts {
    title: "Write a short title alone.",
    recap: "Summarize the conversation for continuation.",
    review: "Judge whether the call is authorized. Return the verdict.",
};

/// No credentials: a test's threads ask no model.
struct Nothing;

impl Credentials for Nothing {
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

/// Octoii's setup for a test: the machine as `machine` gives it, a volume in memory, no providers and no models.
#[must_use]
pub fn setup(machine: crate::machine::Host) -> Setup {
    Setup {
        machine,
        sign_in: crate::providers::SignInSetup::default(),
        credentials: Arc::new(Nothing),
        transport: Arc::new(octoii_ai::transport::Network::default()),
        providers: Vec::new(),
        catalogue: Vec::new(),
        directory: None,
    }
}

/// A test's in-process client for command dispatch and the watch connection.
#[derive(Debug)]
pub struct Client {
    octoii: Octoii,
    access: Access,
    pub controls: mpsc::Sender<String>,
    sent: mpsc::Receiver<Outgoing>,
    passed: std::collections::VecDeque<(Value, Bytes)>,
    next: u64,
}

impl Client {
    /// A client of `siphon` whose session holds `scopes`, its queue `ahead` long.
    #[must_use]
    pub fn connect(siphon: &Octoii, scopes: Vec<Scope>, ahead: usize) -> Self {
        let (controls, incoming) = mpsc::channel(16);
        let (outgoing, sent) = mpsc::channel(ahead);
        let access = Access {
            session: "session-1".into(),
            scopes,
        };
        let machine = Machine::of("here".into(), &siphon.machine);
        tokio::spawn(serve(siphon.clone(), access.clone(), machine, incoming, outgoing));
        Self {
            octoii: siphon.clone(),
            access,
            controls,
            sent,
            passed: std::collections::VecDeque::new(),
            next: 0,
        }
    }

    /// The next message sent, with the bytes a binary frame carries after its event.
    ///
    /// # Panics
    /// Where none comes within five seconds, or it is not the protocol's.
    pub async fn received(&mut self) -> (Value, Bytes) {
        let sent = tokio::time::timeout(Duration::from_secs(5), self.sent.recv())
            .await
            .expect("a message in time")
            .expect("the connection open");
        match sent {
            Outgoing::Text(text) => (serde_json::from_str(&text).expect("JSON"), Bytes::new()),
            Outgoing::Binary(frame) => {
                let length = u32::from_be_bytes(frame[..4].try_into().expect("a length")) as usize;
                let event = serde_json::from_slice(&frame[4..4 + length]).expect("JSON");
                (event, frame.slice(4 + length..))
            }
        }
    }

    /// The next message sent.
    pub async fn message(&mut self) -> Value {
        self.received().await.0
    }

    /// Dispatches an HTTP command or query with the fixture's scopes.
    pub async fn command(&self, method: &str, params: Value) -> Value {
        let reply = match call(
            self.octoii.clone(),
            self.access.clone(),
            Command {
                method: method.to_owned(),
                params,
            },
        )
        .await
        {
            Ok(result) => CommandReply::Result { result },
            Err(error) => CommandReply::Error { error },
        };
        serde_json::to_value(reply).unwrap_or_default()
    }

    /// Controls a watch and returns its response, keeping events that arrive while it waits.
    ///
    /// # Panics
    /// Where the connection is gone.
    pub async fn watch(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next.to_string();
        let call = json!({ "id": id, "method": method, "params": params });
        self.controls.send(call.to_string()).await.expect("sent");
        loop {
            let (message, payload) = self.received().await;
            if message["id"] == id {
                return message;
            }
            self.passed.push_back((message, payload));
        }
    }

    /// Subscribes to `topic`: the subscription's id.
    ///
    /// # Panics
    /// Where it is refused.
    pub async fn subscribe(&mut self, topic: Value) -> String {
        let answer = self.watch("subscribe", json!({ "topic": topic })).await;
        answer["result"]["subscription"].as_str().expect("subscribed").to_owned()
    }

    /// The next event of the subscription `subscription` that `wanted` takes, with the bytes it carries.
    pub async fn event(&mut self, subscription: &str, wanted: impl Fn(&Value) -> bool) -> (Value, Bytes) {
        let taken = |message: &Value| message["subscription"] == subscription && wanted(message);
        if let Some(at) = self.passed.iter().position(|(message, _)| taken(message)) {
            let mut kept = self.passed.split_off(at);
            self.passed = kept.split_off(1);
            if let Some(found) = kept.pop_front() {
                return found;
            }
        }
        self.passed.clear();
        loop {
            let (message, payload) = self.received().await;
            if message["subscription"] == subscription && wanted(&message) {
                return (message, payload);
            }
        }
    }
}

/// What a call came to, as a test reads it: how it ended, its text, how many images it showed, and what it told typed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Called {
    pub content: Vec<octoii_ai::vocabulary::InputContent>,
    pub status: crate::threads::CallStatus,
    pub text: String,
    pub images: usize,
    pub structured_content: Option<Value>,
    pub metadata: Option<Value>,
}

/// Model calls run as a turn runs them, for an extension's tests of its tools: in full access, of the thread `t1`, each
/// exec'd by Octoii's own tools and those a test adds, a script's calls too; the person's trust asked as Octoii asks it.
#[derive(Debug)]
pub struct Calling {
    runtime: octoii_siphon_lib::Runtime,
}

/// What the caller of a test's calls is started with: where they work, the tools of <catalog>, and the calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Asking {
    workspace: std::path::PathBuf,
    catalog: Vec<String>,
    calls: Vec<(String, String, String)>,
}

/// Takes a test's calls on in full access, then ends with what each came to.
struct Caller {
    tools: crate::tools::Tools,
}

impl octoii_siphon_lib::Fiber for Caller {
    const KIND: &'static str = "siphon.testing.caller";
    type Input = Asking;
    type Checkpoint = crate::tools::Calls;
    type Output = Vec<Called>;

    fn run(
        &self,
        run: &mut octoii_siphon_lib::Run<'_>,
        input: Asking,
        checkpoint: Option<crate::tools::Calls>,
    ) -> anyhow::Result<octoii_siphon_lib::Ending<crate::tools::Calls, Vec<Called>>> {
        let mut calls = match checkpoint {
            Some(calls) => calls,
            None => crate::tools::Calls::new(run, input.calls.clone())?,
        };
        let base = crate::machine::paths::Base {
            workspace: input.workspace.clone(),
            home: None,
        };
        let setting = crate::tools::Setting {
            primitives: &crate::agent::ThreadPrimitives,
            thread: "t1",
            threads: None,
            tools: &self.tools,
            catalog: &input.catalog,
            mode: crate::threads::Mode::FullAccess,
            base: &base,
            project: None,
            places: &[],
            confines: octoii_siphon_lib::machine_confines(),
        };
        if let Some(wait) = calls.advance(run, &setting)? {
            return Ok(octoii_siphon_lib::Ending::Wait(calls, wait));
        }
        Ok(octoii_siphon_lib::Ending::Done(calls.0.into_iter().map(called).collect()))
    }
}

/// What `call` came to.
fn called(call: crate::tools::Call) -> Called {
    let crate::tools::State::Ended {
        status,
        output,
        structured_content,
        metadata,
        ..
    } = call.state
    else {
        return Called {
            content: Vec::new(),
            status: crate::threads::CallStatus::Interrupted,
            text: String::new(),
            images: 0,
            structured_content: None,
            metadata: None,
        };
    };
    let mut texts = Vec::new();
    let mut images = 0;
    for part in &output {
        match part {
            octoii_ai::vocabulary::InputContent::InputText { text } => texts.push(text.as_str()),
            octoii_ai::vocabulary::InputContent::InputImage { .. } => images += 1,
            octoii_ai::vocabulary::InputContent::InputFile { .. } | octoii_ai::vocabulary::InputContent::ResourceLink { .. } => {}
        }
    }
    let text = texts.join("\n");
    Called {
        content: output,
        status,
        text,
        images,
        structured_content,
        metadata,
    }
}

impl Calling {
    /// Calls run on `beak`'s volume in memory, by Octoii's own tools and those `add` adds.
    ///
    /// # Panics
    /// Where the volume cannot be opened.
    #[must_use]
    pub fn start(add: impl FnOnce(crate::tools::Tools) -> crate::tools::Tools) -> Self {
        let tools = add(crate::tools::Tools::new()
            .with(crate::tools::ReadTool)
            .with(crate::tools::WriteTool)
            .with(crate::tools::ShellTool)
            .with(crate::tools::ExecuteTool));
        let execs = tools.clone();
        let runtime = start_runtime(None, |builder, state| {
            let state = state.clone();
            builder
                .fiber(Caller { tools: tools.clone() })?
                .fiber(crate::tools::ScriptFiber {
                    tools,
                    primitives: Arc::new(crate::agent::ThreadPrimitives),
                })?
                .fiber(crate::trust::TrustFiber)?
                .ask_person::<crate::trust::Trust>()?
                .handle(
                    &tokio::runtime::Handle::current(),
                    move |asked: octoii_siphon_lib::Asked<crate::tools::Exec>| {
                        let tools = execs.clone();
                        let state = state.clone();
                        async move { crate::tools::answer_exec(&tools, &crate::machine::Host::default(), &state, asked).await }
                    },
                )?;
            Ok(())
        })
        .expect("started");
        Self { runtime }
    }

    /// What `calls` — each its id, its tool and its arguments as JSON — came to, made in `workspace` with the tools of
    /// <catalog> `catalog`, run under `key`.
    ///
    /// # Panics
    /// Where the volume fails, or the calls do not end in 30 s.
    pub async fn called(
        &self,
        key: &str,
        workspace: std::path::PathBuf,
        catalog: Vec<String>,
        calls: Vec<(String, String, String)>,
    ) -> Vec<Called> {
        let asking = Asking { workspace, catalog, calls };
        let mut changes = octoii_siphon_lib::Changes::new().expect("a batch");
        changes
            .mkdir(crate::threads::thread::path("t1"))
            .and_then(|changes| changes.mkdir(crate::threads::inbox::path("t1")))
            .and_then(|changes| changes.start::<Caller>(None, key, &asking))
            .expect("a thread and input");
        self.runtime.commit(changes).await.expect("started");
        let path = octoii_siphon_lib::fiber_path(b"", key.as_bytes());
        let mut committed = self.runtime.commits().expect("a commit watch");
        let waiting = async {
            loop {
                let record = self.runtime.snapshot().expect("a snapshot").fiber(&path).expect("a record");
                if let Some(octoii_siphon_lib::Outcome::Completed(Some(result))) = record.and_then(|record| record.outcome) {
                    return serde_json::from_slice(&result).expect("what it came to");
                }
                assert!(matches!(committed.next().await, Ok(Some(octoii_siphon_lib::CommitEvent::Head(_)))));
            }
        };
        tokio::time::timeout(Duration::from_secs(30), waiting).await.expect("ended in time")
    }
}

/// The machine's commands on a kernel of `beak`'s own, in memory, for an extension's tests of what it runs.
///
/// # Panics
/// Where the kernel cannot be opened.
#[must_use]
pub fn commands(runtime: &Runtime) -> crate::machine::Commands {
    crate::machine::Commands {
        runtime: runtime.runtime_handle(),
    }
}

impl Calling {
    /// The volume of the calls' runtime, as an extension reaches it.
    #[must_use]
    pub fn volume(&self) -> crate::volume::Volume {
        crate::volume::Volume {
            runtime: self.runtime.runtime_handle(),
        }
    }

    /// Asks the person `trust` as Octoii asks it, answers `trusted` as they would once it is asked, and returns once it
    /// is decided.
    ///
    /// # Panics
    /// Where the volume fails, or it is not asked and decided in 10 s each.
    pub async fn decide(&self, trust: &crate::trust::Trust, trusted: bool) {
        let key = crate::trust::TrustFiber::KEY.as_bytes();
        let mut changes = octoii_siphon_lib::Changes::new().expect("a batch");
        changes
            .start::<crate::trust::TrustFiber>(None, crate::trust::TrustFiber::KEY, trust)
            .expect("an input");
        self.runtime.commit(changes).await.expect("started");
        let process = octoii_siphon_lib::fiber_path(b"", key);
        let mut committed = self.runtime.commits().expect("a commit watch");
        let asked = async {
            loop {
                let snapshot = self.runtime.snapshot().expect("a snapshot");
                if let Some(prompt) = self
                    .runtime
                    .person_requests(&snapshot, &process)
                    .expect("prompts")
                    .into_iter()
                    .find(|prompt| prompt.id.key() == key)
                {
                    return prompt.id;
                }
                assert!(matches!(committed.next().await, Ok(Some(octoii_siphon_lib::CommitEvent::Head(_)))));
            }
        };
        let request = tokio::time::timeout(Duration::from_secs(10), asked).await.expect("asked in time");
        self.runtime
            .answer(&request, &json!({ "trusted": trusted }))
            .await
            .expect("answered");
        let decided = async {
            loop {
                let snapshot = self.runtime.snapshot().expect("a snapshot");
                if crate::trust::decided(&snapshot, &trust.folder, &trust.digest)
                    .expect("read")
                    .is_some()
                {
                    break;
                }
                assert!(matches!(committed.next().await, Ok(Some(octoii_siphon_lib::CommitEvent::Head(_)))));
            }
        };
        tokio::time::timeout(Duration::from_secs(10), decided)
            .await
            .expect("decided in time");
    }

    /// Ends the calls' runtime.
    ///
    /// # Panics
    /// The runtime or its shutdown task failed.
    pub async fn close(&self) {
        let runtime = self.runtime.clone();
        tokio::task::spawn_blocking(move || runtime.close())
            .await
            .expect("shutdown task finished")
            .expect("closed");
    }
}
