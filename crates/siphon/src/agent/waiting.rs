//! Being told how another thread's turn ends: a task given to a sub-thread, or a message sent another thread, waited on
//! by the turn that took it — by the call that gave it, or by a process beside the thread's main process, which, as
//! `at` mails a job's output to whoever queued it, delivers what came of it to the thread's inbox; and the reading of
//! the mail that tells how that turn ended.

use crate::threads::inbox::{self, Ended, Inbound, Joins};
use crate::threads::thread;
use beak::layout;
use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use siphon_runtime::{Ending, Program, Run, Wait};

/// A task given to a sub-thread, waited on: the thread that gave it, the sub-thread, and the task, by its id in the
/// sub-thread's inbox, which is also the key of the mail that tells how the turn that took it ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiting {
    pub thread: String,
    pub child: String,
    pub task: String,
}

/// How the sub-thread of `waiting` ended its work on the task, as its mail tells, taken with the run's commit; none
/// while it has not.
///
/// # Errors
/// The volume's failure; a mail not as a thread writes it.
pub fn ended_of(run: &mut Run<'_>, waiting: &Waiting) -> anyhow::Result<Option<Ended>> {
    let path = layout::mail(run.path(), waiting.task.as_bytes());
    let Some(_) = run.snapshot().stat(&path)? else { return Ok(None) };
    let payload = run.snapshot().read(&layout::payload(&path), 0, u64::MAX)?.unwrap_or_default();
    run.take_mail(&waiting.task);
    Ok(Some(serde_json::from_slice(&payload)?))
}

/// Whether the sub-thread of `waiting` went, deleted.
///
/// # Errors
/// The volume's failure.
pub fn gone(run: &Run<'_>, waiting: &Waiting) -> anyhow::Result<bool> {
    Ok(run.snapshot().stat(&thread::path(&waiting.child))?.is_none())
}

/// How a sub-thread's work ended, as `ended` says — none, deleted before it did — said of it.
#[must_use]
pub fn how(ended: Option<&Ended>) -> String {
    match ended {
        Some(Ended::Replied { .. }) => "finished: what it said last is here, and its whole work is in it".to_owned(),
        Some(Ended::Failed { reason }) => format!("failed: {reason}"),
        Some(Ended::Stopped) => "was stopped by the person before it finished".to_owned(),
        Some(Ended::Withdrawn) => "had the task taken back before it began it".to_owned(),
        None => "was deleted before it finished".to_owned(),
    }
}

/// The directory of the thread `id`, as the run's snapshot holds it.
///
/// # Errors
/// The volume's failure.
pub fn directory_of(run: &Run<'_>, id: &str) -> anyhow::Result<Option<beak::Directory>> {
    Ok(run.snapshot().stat(&thread::path(id))?.and_then(|entry| entry.directory))
}

/// The label `name` of `directory`; none where it has none.
#[must_use]
pub fn label(directory: &beak::Directory, name: &str) -> Option<String> {
    directory
        .label(name.as_bytes())
        .filter(|value| !value.is_empty())
        .map(|value| String::from_utf8_lossy(value).into_owned())
}

/// The program of a task given in the background — delegated to a sub-thread, or a message sent to another thread to be
/// told of — beside its thread's main process: waits on the thread that took it till the turn that took it ends, then
/// ends, what came of it delivered to the thread's inbox in the same commit.
#[derive(Debug, Clone, Copy, Default)]
pub struct DelegatedProgram;

impl Program for DelegatedProgram {
    const KIND: &'static str = "siphon.delegated";
    type Input = Waiting;
    type Checkpoint = ();
    type Output = ();

    fn run(&self, run: &mut Run<'_>, waiting: Waiting, _: Option<()>) -> anyhow::Result<Ending<(), ()>> {
        let ended = match ended_of(run, &waiting)? {
            Some(ended) => Some(ended),
            None if gone(run, &waiting)? => None,
            None => {
                let wait = Wait::new().mail(waiting.task.clone()).change(thread::path(&waiting.child));
                return Ok(Ending::Wait((), wait));
            }
        };
        let ours = directory_of(run, &waiting.child)?
            .and_then(|child| label(&child, thread::PARENT_LABEL))
            .as_deref()
            == Some(waiting.thread.as_str());
        let note = if ours {
            format!(
                "[the sub-thread {}, which you gave a task in the background, {}]",
                waiting.child,
                how(ended.as_ref())
            )
        } else {
            format!("[the thread {}, which you sent a message, {}]", waiting.child, how(ended.as_ref()))
        };
        let mut content = vec![InputContent::text(note)];
        if let Some(Ended::Replied { said }) = ended {
            content.push(InputContent::text(said));
        }
        let inbound = Inbound::Input {
            actor: waiting.child.clone(),
            content,
            reply_to: None,
            joins: Joins::Step,
        };
        let delivery = run.new_id()?;
        inbox::put(&mut run.changes, &waiting.thread, &delivery, &inbound)?;
        Ok(Ending::Done(()))
    }
}
