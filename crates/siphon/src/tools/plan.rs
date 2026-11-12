//! What a call does to threads, as the primitives of a kernel's processes are to its programs: a thread made, a
//! message delivered to one, a turn stopped. Its tool plans them as it runs, minting their ids, and does none of them;
//! its caller does them as it takes the call's end, in the commit that ends it, as `Primitives` say — so a call run again
//! after a crash plans again and nothing is done twice. A call may end with how the turn that takes what it delivered
//! ends, which its caller waits on durably, as `wait` waits on a child.

use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use siphon_runtime::Run;

use crate::threads::inbox::Joins;
use crate::tools::tool::Refusal;

/// What a call does to threads, in order, and what it ends with: what its tool told, or how a turn ends.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<Step>,
    /// The delivery whose turn's end the call ends with, where it waits on one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub awaits: Option<Awaited>,
}

/// A delivery a call waits on: the thread it went to, and its id there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Awaited {
    pub thread: String,
    pub delivery: String,
}

/// A step of a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum Step {
    /// A thread made like the caller's — its folder, model, mode and effort — titled `title`: a sub-thread of the call,
    /// or a thread of the person's.
    Spawn {
        thread: String,
        entry: String,
        title: Option<String>,
        spawned: Spawned,
    },
    /// `content` delivered to the thread `thread` as `delivery`, from the caller's thread, joining its work as `joins`
    /// says, how the turn that takes it ends told as `report` says.
    Deliver {
        thread: String,
        delivery: String,
        content: Vec<InputContent>,
        joins: Joins,
        report: Report,
    },
    /// The turn the thread `thread` works on stopped.
    Stop { thread: String },
}

/// Whose a thread made is: a sub-thread of the call that made it, or a thread of the person's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Spawned {
    SubThread,
    Thread,
}

/// Who is told how the turn that takes a delivery ends: no one; the call, which ends with it; or the caller's thread,
/// by a message once it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Report {
    None,
    Call,
    Inbox,
}

impl Plan {
    /// Plans a thread made like the caller's, titled `title`, as `spawned` says: its id.
    ///
    /// # Errors
    /// A `Refusal` where no id could be minted.
    pub fn spawn(&mut self, volume: &crate::volume::Volume, title: Option<String>, spawned: Spawned) -> Result<String, Refusal> {
        let thread = minted(volume)?;
        let entry = minted(volume)?;
        self.steps.push(Step::Spawn {
            thread: thread.clone(),
            entry,
            title,
            spawned,
        });
        Ok(thread)
    }

    /// Plans `content` delivered to the thread `thread`, joining its work as `joins` says, told as `report` says — the
    /// call ending with it where it is the call's: the delivery's id.
    ///
    /// # Errors
    /// A `Refusal` where no id could be minted.
    pub fn deliver(
        &mut self,
        volume: &crate::volume::Volume,
        thread: &str,
        content: Vec<InputContent>,
        joins: Joins,
        report: Report,
    ) -> Result<String, Refusal> {
        let delivery = minted(volume)?;
        if report == Report::Call {
            self.awaits = Some(Awaited {
                thread: thread.to_owned(),
                delivery: delivery.clone(),
            });
        }
        self.steps.push(Step::Deliver {
            thread: thread.to_owned(),
            delivery: delivery.clone(),
            content,
            joins,
            report,
        });
        Ok(delivery)
    }

    /// Plans the turn the thread `thread` works on stopped.
    pub fn stop(&mut self, thread: &str) {
        self.steps.push(Step::Stop { thread: thread.to_owned() });
    }

    /// Whether it plans nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty() && self.awaits.is_none()
    }
}

/// An id minted from the volume.
fn minted(volume: &crate::volume::Volume) -> Result<String, Refusal> {
    volume.new_id().map_err(|error| Refusal(format!("no id was minted: {error}")))
}

/// What does a call's plan, in the commit of its caller's `run` that ends the call: the thread above Siphon's tools,
/// which knows how a thread is made and run.
pub trait Primitives: Send + Sync {
    /// Does `plan`, of the call keyed `call` of the thread `thread`, with the run's commit; why not, where a step may
    /// not be done — and then none is.
    ///
    /// # Errors
    /// The volume's failure.
    fn apply(&self, run: &mut Run<'_>, thread: &str, call: &str, plan: &Plan) -> anyhow::Result<Result<(), String>>;

    /// Takes back `awaited`, which a call waits on as it stops: the delivery where it waits in the inbox still, the turn
    /// that took it stopped otherwise.
    ///
    /// # Errors
    /// The volume's failure.
    fn withdraw(&self, run: &mut Run<'_>, awaited: &Awaited) -> anyhow::Result<()>;
}
