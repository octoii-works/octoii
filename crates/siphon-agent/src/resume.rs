//! Going on at a time the person asked — when the quota its model's account used up is given again: a process beside
//! the thread's main process, of no turn, that sleeps till then, delivers the person's word to go on to the thread's
//! inbox, as `at` runs a job at its time, and ends; a turn begins with it. What the person sends meanwhile, or another
//! model they give the thread, ends it first, as they went on themselves.

use beak::Snapshot;
use serde::{Deserialize, Serialize};
use siphon_runtime::{Changes, Ending, Program, Run, Wait};
use siphon_threads::inbox::{self, Inbound};
use siphon_threads::{Entry, Window, thread};

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

impl ResumeInput {
    /// The resume of the thread `id` at `at`, delivering `actor`'s word, armed as `snapshot` leaves its log.
    ///
    /// # Errors
    /// The volume's failure; a log not as Siphon writes it.
    pub fn now(snapshot: &Snapshot, id: &str, at: i64, actor: &str) -> anyhow::Result<Self> {
        Ok(Self {
            thread: id.to_owned(),
            at,
            actor: actor.to_owned(),
            after: Window::read(snapshot, id)?.last().map(str::to_owned),
        })
    }
}

/// The program of a thread's resume, keyed `KEY` under its main process.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResumeProgram;

impl ResumeProgram {
    pub const KEY: &'static str = thread::RESUME_KEY;
}

/// Why a resume was not armed.
#[derive(Debug, thiserror::Error)]
pub enum ArmError {
    #[error("a resume is armed for the thread already")]
    Armed,
    #[error(transparent)]
    Beak(#[from] beak::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Adds to `changes` the resume of `input` armed, one that ended reaped as it is armed anew.
///
/// # Errors
/// `Armed` where one waits still; the volume's failure.
pub fn arm(snapshot: &Snapshot, changes: &mut Changes, input: &ResumeInput) -> Result<(), ArmError> {
    let path = beak::layout::process(&thread::main(&input.thread), ResumeProgram::KEY.as_bytes());
    match snapshot.record(&path)? {
        Some(record) if record.outcome.is_none() => return Err(ArmError::Armed),
        Some(_) => {
            changes.reap(path);
        }
        None => {}
    }
    changes.start::<ResumeProgram>(thread::main(&input.thread), ResumeProgram::KEY, input)?;
    Ok(())
}

/// Adds to `changes` the resume of the thread `id` let go of, where one is armed.
pub fn disarm(changes: &mut Changes, id: &str) {
    changes.force_remove(beak::layout::process(&thread::main(id), ResumeProgram::KEY.as_bytes()), None);
}

impl Program for ResumeProgram {
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
