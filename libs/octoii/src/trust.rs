//! The person's trust in what a project's configuration runs — what an extension runs that a project names, as the MCP
//! servers its `.mcp.json` names, run once the person trusts them, as a shell's hook loads a folder's environment file
//! once the person allowed it. A decision is on
//! the configuration as it was as they decided, by the SHA-256 of its bytes, kept in the project's `siphon.trust`
//! annotation, so a configuration changed since is one they have not decided on. A thread whose project's
//! configuration they have not decided on asks them by a process of its own, beside its turns, which ends once they
//! answered — recording what they decided — or once the decision is made otherwise, as another thread asked it. The
//! person may decide unasked as well, or decide again.

use bytes::Bytes;
use octoii_siphon_lib::Snapshot;
use octoii_siphon_lib::{Answer, AskedOfPerson, Changes, Ending, Fiber, Request, Run, Wait};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::threads::thread;
use crate::tools::projects;

/// The annotation that holds the decision.
const ANNOTATION: &str = "siphon.trust";

/// What the person is asked: whether what the project of `folder` names in its configuration — the file at `path`, of
/// the digest `digest` — may run: what it names, each as it would run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ts_rs::TS)]
pub struct Trust {
    pub folder: String,
    pub path: String,
    pub digest: String,
    pub servers: Vec<ShownServer>,
}

/// What a configuration names as the person is shown it: its name, and what it runs, or where it is reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ts_rs::TS)]
pub struct ShownServer {
    pub name: String,
    pub runs: String,
}

/// The person's answer: whether what the configuration names may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Trusted {
    pub trusted: bool,
}

impl Request for Trust {
    const NAME: &'static str = "siphon.trust";
    type Answer = Trusted;
}

impl AskedOfPerson for Trust {
    fn answer_schema(&self) -> serde_json::Value {
        schemars::schema_for!(Self::Answer).to_value()
    }
}

/// A decision, as the annotation keeps it.
#[derive(Debug, Serialize, Deserialize)]
struct Decision {
    digest: String,
    trusted: bool,
}

/// The digest of a configuration's bytes, as a decision names it.
#[must_use]
pub fn digest(bytes: &[u8]) -> String {
    projects::sha256_hex(bytes)
}

/// What the person decided on the configuration of the project of `folder` of the digest `digest`: whether it may
/// run; none where they decided nothing on it as it is.
///
/// # Errors
/// The volume's failure.
pub fn decided(snapshot: &Snapshot, folder: &str, digest: &str) -> octoii_siphon_lib::Result<Option<bool>> {
    let kept = projects::annotation(snapshot, folder, ANNOTATION)?;
    let decision = kept.and_then(|kept| serde_json::from_slice::<Decision>(&kept).ok());
    Ok(decision
        .filter(|decision| decision.digest == digest)
        .map(|decision| decision.trusted))
}

/// The changes that record the person's decision on the configuration of the project of `folder` of the digest
/// `digest`, in place of any before.
///
/// # Errors
/// The volume's failure.
pub fn decide(snapshot: &Snapshot, folder: &str, digest: &str, trusted: bool) -> octoii_siphon_lib::Result<Changes> {
    let decision = Decision {
        digest: digest.to_owned(),
        trusted,
    };
    let value = Bytes::from(serde_json::to_vec(&decision).unwrap_or_default());
    projects::annotate(snapshot, folder, ANNOTATION, value)
}

/// The process that asks the person what a `Trust` asks, keyed as `KEY` under the thread that needs it.
#[derive(Debug, Clone, Copy, Default)]
pub struct TrustFiber;

impl TrustFiber {
    /// The key of the process under its thread, and of its request.
    pub const KEY: &'static str = "trust";
}

impl Fiber for TrustFiber {
    const KIND: &'static str = "siphon.trust";
    type Input = Trust;
    type Checkpoint = ();
    type Output = ();

    fn run(&self, run: &mut Run<'_>, trust: Trust, asked: Option<()>) -> anyhow::Result<Ending<(), ()>> {
        if decided(run.snapshot(), &trust.folder, &trust.digest)?.is_some() {
            return Ok(Ending::Done(()));
        }
        let wait = Wait::new().mail(Self::KEY).change(projects::path(&trust.folder));
        if asked.is_none() {
            run.request(Self::KEY, &trust)?;
            return Ok(Ending::Wait((), wait));
        }
        let Some(answer) = run.answer::<Trust>(Self::KEY)? else {
            return Ok(Ending::Wait((), wait));
        };
        run.take_mail(Self::KEY)?;
        if let Answer::Given(Trusted { trusted }) = answer {
            let decision = decide(run.snapshot(), &trust.folder, &trust.digest, trusted)?;
            run.changes.extend(decision);
        }
        Ok(Ending::Done(()))
    }
}

/// Asks the person `trust` beside the main process of the thread `thread`, which needs it — where nothing asks it there
/// yet.
///
/// # Errors
/// The volume's failure.
pub async fn ask(volume: &crate::volume::Volume, thread: &str, trust: &Trust) -> anyhow::Result<()> {
    let main = thread::main(thread);
    let asking = octoii_siphon_lib::process_path(&main, TrustFiber::KEY.as_bytes());
    if volume.snapshot()?.fiber(&asking)?.is_some() {
        return Ok(());
    }
    let mut changes = Changes::new();
    changes.start::<TrustFiber>(main, TrustFiber::KEY, trust)?;
    volume.runtime.commit(changes).await?;
    Ok(())
}
