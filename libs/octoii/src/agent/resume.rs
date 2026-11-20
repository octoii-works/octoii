//! Going on at a time the person asked — when the quota its model's account used up is given again: a process beside
//! the thread's main process, of no turn, that sleeps till then, delivers the person's word to go on to the thread's
//! inbox, as `at` runs a job at its time, and ends; a turn begins with it. What the person sends meanwhile, or another
//! model they give the thread, ends it first, as they went on themselves.

use crate::threads::inbox::{self, Inbound, Joins};
use crate::threads::{Entry, Error, Window, thread};
use octoii_siphon_lib::{Batch, Ending, Fiber, Run, Snapshot, Status, Wait, fiber_path};
use serde::{Deserialize, Serialize};

/// The person's word it goes on with.
pub const WORD: &str = "Continue where you stopped: the account's usage limit has reset.";

/// What a resume is armed with: its thread, when it goes on, in milliseconds since the epoch, whose word it delivers,
/// and the last entry of the thread's window as it was armed — none for a window with none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeInput {
    pub thread: String,
    pub at: i64,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// The fiber of a thread's resume, keyed `KEY` under its main process.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResumeFiber;

impl ResumeFiber {
    pub const KEY: &'static str = thread::RESUME_KEY;
}

/// Stages replacing the thread's resume at `at`, or cancelling it when the deadline is absent.
///
/// # Errors
/// An unreadable window, a missing parent for the replacement, or the volume's failure.
pub(crate) fn reschedule(snapshot: &Snapshot, changes: &mut Batch<'_>, id: &str, at: Option<i64>, actor: &str) -> Result<(), Error> {
    let main = thread::main(id);
    let replacement = if let Some(at) = at {
        let input = ResumeInput {
            thread: id.to_owned(),
            at,
            actor: actor.to_owned(),
            after: Window::read(snapshot, id)?.last().map(str::to_owned),
        };
        let parent = snapshot
            .fiber(&main)?
            .ok_or_else(|| octoii_siphon_lib::Error::new(Status::NotFound, "the thread's main Fiber is missing"))?;
        Some((parent.id, input))
    } else {
        None
    };
    let path = fiber_path(&main, ResumeFiber::KEY.as_bytes());
    if let Some(record) = snapshot.fiber(&path)? {
        changes.force_remove(&record.id, None)?;
    }
    if let Some((parent, input)) = replacement {
        changes.start::<ResumeFiber>(Some(&parent), ResumeFiber::KEY, &input)?;
    }
    Ok(())
}

impl Fiber for ResumeFiber {
    const KIND: &'static str = "siphon.resume";
    type Input = ResumeInput;
    type Checkpoint = ();
    type Output = bool;

    fn run(&self, run: &mut Run<'_>, input: ResumeInput, _: Option<()>) -> anyhow::Result<Ending<(), bool>> {
        if went_on(run.snapshot(), &input.thread, input.after.as_deref())? {
            return Ok(Ending::Done(false));
        }
        if run.time_ms() < input.at {
            let wait = Wait::new().deadline(input.at).change(inbox::path(&input.thread));
            return Ok(Ending::Wait((), wait));
        }
        let word = Inbound::Input {
            actor: input.actor.clone(),
            content: vec![octoii_ai::vocabulary::InputContent::text(WORD)],
            reply_to: None,
            joins: Joins::Step,
        };
        let delivery = run.new_id()?;
        inbox::put(&mut run.changes, &input.thread, &delivery, &word)?;
        Ok(Ending::Done(true))
    }
}

/// Whether the person went on with the thread `id` themselves since the entry `after` of its log: an input of theirs,
/// or another model, waits in its inbox or was taken into its log.
fn went_on(snapshot: &Snapshot, id: &str, after: Option<&str>) -> anyhow::Result<bool> {
    let waits = inbox::waiting(snapshot, id)?.into_iter().any(|waiting| match waiting.inbound {
        Inbound::Input { .. } => true,
        Inbound::Config { change, .. } => change.model.is_some(),
    });
    if waits {
        return Ok(true);
    }
    let window = Window::read(snapshot, id)?;
    let since = after
        .and_then(|after| window.conversation.iter().position(|entry| entry.id() == after))
        .map_or(0, |at| at + 1);
    Ok(window.conversation[since..].iter().any(|entry| match entry {
        Entry::Input { .. } => true,
        Entry::Config { changed, .. } => changed.model.is_some(),
        _ => false,
    }))
}
