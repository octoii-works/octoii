//! What tests of Siphon, of its extensions and of a host build on: Siphon's setup on a volume in memory that asks no
//! model; a client of a connection — its calls made, its answers and events read, a binary frame's event with the
//! bytes it carries; and model calls run as a turn runs them, for an extension's tests of its tools.

use std::sync::Arc;
use std::time::Duration;

use crate::protocol::{Machine, Scope};
use bytes::Bytes;
use octoii_ai::host::{Credential, Credentials, Unavailable};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use siphon_runtime::Options;
use tokio::sync::mpsc;

use crate::connection::{Access, Outgoing, serve};
use crate::{Setup, Siphon};

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

/// Siphon's setup for a test: the machine as `machine` gives it, a volume in memory, no providers and no models.
#[must_use]
pub fn setup(machine: crate::machine::Host) -> Setup {
    Setup {
        machine,
        config: None,
        sign_in: crate::providers::SignInSetup::default(),
        credentials: Arc::new(Nothing),
        transport: Arc::new(octoii_ai::transport::Network::default()),
        providers: Vec::new(),
        catalogue: Vec::new(),
        options: Options::default(),
    }
}

/// A test's client of a connection: what it sends, what it is sent, and what it was sent and passed over while it
/// waited for an answer, kept for what it waits for next.
#[derive(Debug)]
pub struct Client {
    pub calls: mpsc::Sender<String>,
    sent: mpsc::Receiver<Outgoing>,
    passed: std::collections::VecDeque<(Value, Bytes)>,
    next: u64,
}

impl Client {
    /// A client of `siphon` whose session holds `scopes`, its queue `ahead` long.
    #[must_use]
    pub fn connect(siphon: &Siphon, scopes: Vec<Scope>, ahead: usize) -> Self {
        let (calls, incoming) = mpsc::channel(16);
        let (outgoing, sent) = mpsc::channel(ahead);
        let access = Access {
            session: "session-1".into(),
            scopes,
        };
        let machine = Machine::of("here".into(), &siphon.machine);
        tokio::spawn(serve(siphon.clone(), access, machine, incoming, outgoing));
        Self {
            calls,
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

    /// Calls `method` with `params`: its id.
    ///
    /// # Panics
    /// Where the connection is gone.
    pub async fn call(&mut self, method: &str, params: Value) -> String {
        self.next += 1;
        let id = self.next.to_string();
        let call = json!({ "id": id, "method": method, "params": params });
        self.calls.send(call.to_string()).await.expect("sent");
        id
    }

    /// The answer to the call `id`, what came before it kept for later.
    pub async fn answer(&mut self, id: &str) -> Value {
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
        let id = self.call("subscribe", json!({ "topic": topic })).await;
        let answer = self.answer(&id).await;
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
    pub status: crate::threads::CallStatus,
    pub text: String,
    pub images: usize,
    pub structured: Option<Value>,
}

/// Model calls run as a turn runs them, for an extension's tests of its tools: in full access, of the thread `t1`, each
/// exec'd by Siphon's own tools and those a test adds, a script's calls too; the person's trust asked as Siphon asks it.
#[derive(Debug)]
pub struct Calling {
    pub runtime: siphon_runtime::Runtime,
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

impl siphon_runtime::Program for Caller {
    const KIND: &'static str = "siphon.testing.caller";
    type Input = Asking;
    type Checkpoint = crate::tools::Calls;
    type Output = Vec<Called>;

    fn run(
        &self,
        run: &mut siphon_runtime::Run<'_>,
        input: Asking,
        checkpoint: Option<crate::tools::Calls>,
    ) -> anyhow::Result<siphon_runtime::Ending<crate::tools::Calls, Vec<Called>>> {
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
            tools: &self.tools,
            catalog: &input.catalog,
            mode: crate::threads::Mode::FullAccess,
            base: &base,
            project: None,
            places: &[],
            confines: beak::machine_confines(),
        };
        if let Some(wait) = calls.advance(run, &setting)? {
            return Ok(siphon_runtime::Ending::Wait(calls, wait));
        }
        Ok(siphon_runtime::Ending::Done(calls.0.into_iter().map(called).collect()))
    }
}

/// What `call` came to.
fn called(call: crate::tools::Call) -> Called {
    let crate::tools::State::Ended {
        status,
        output,
        structured,
        ..
    } = call.state
    else {
        return Called {
            status: crate::threads::CallStatus::Interrupted,
            text: String::new(),
            images: 0,
            structured: None,
        };
    };
    let mut texts = Vec::new();
    let mut images = 0;
    for part in &output {
        match part {
            octoii_ai::vocabulary::InputContent::InputText { text } => texts.push(text.as_str()),
            octoii_ai::vocabulary::InputContent::InputImage { .. } => images += 1,
            octoii_ai::vocabulary::InputContent::InputFile { .. } => {}
        }
    }
    Called {
        status,
        text: texts.join("\n"),
        images,
        structured,
    }
}

impl Calling {
    /// Calls run on `beak`'s volume in memory, by Siphon's own tools and those `add` adds.
    ///
    /// # Panics
    /// Where the volume cannot be opened.
    #[must_use]
    pub fn start(beak: &beak::Beak, add: impl FnOnce(crate::tools::Tools) -> crate::tools::Tools) -> Self {
        let tools = add(crate::tools::Tools::new()
            .with(crate::tools::ReadTool)
            .with(crate::tools::WriteTool)
            .with(crate::tools::ShellTool)
            .with(crate::tools::ExecuteTool));
        let execs = tools.clone();
        let runtime = siphon_runtime::Runtime::builder()
            .program(Caller { tools: tools.clone() })
            .program(crate::tools::ScriptProgram {
                tools,
                primitives: Arc::new(crate::agent::ThreadPrimitives),
            })
            .program(crate::trust::TrustProgram)
            .ask_person::<crate::trust::Trust>()
            .handle(move |asked: siphon_runtime::Asked<crate::tools::Exec>| {
                let tools = execs.clone();
                async move { crate::tools::answer_exec(&tools, &crate::machine::Host::default(), asked).await }
            })
            .start(beak, Options::default())
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
        let mut changes = siphon_runtime::Changes::new();
        changes.mkdir(crate::threads::thread::path("t1"));
        changes.start::<Caller>(Bytes::new(), key, &asking).expect("an input");
        self.runtime.commit(changes).await.expect("started");
        let path = beak::layout::process(b"", key.as_bytes());
        let mut committed = self.runtime.committed();
        let waiting = async {
            loop {
                let record = self.runtime.snapshot().expect("a snapshot").record(&path).expect("a record");
                if let Some(beak::Outcome::Completed(Some(result))) = record.and_then(|record| record.outcome) {
                    return serde_json::from_slice(&result).expect("what it came to");
                }
                committed.changed().await.expect("the runtime runs");
            }
        };
        tokio::time::timeout(Duration::from_secs(30), waiting).await.expect("ended in time")
    }
}
