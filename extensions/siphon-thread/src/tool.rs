//! The `thread` tool, by which a thread's model works with the person's other threads, as a shell's user lists, starts
//! and signals other sessions — only where the person asks about another thread: lists them as the person's thread
//! list has them, the most recently active first, a page at a time; finds what was said in them and reads it where it
//! was said (`reading`); starts one of its own, not a sub-thread, in the thread's folder with its model, its mode and
//! its effort, a message its first input; sends a thread a message, which reaches its inbox as a sub-thread's task
//! does, steering it where it runs; or stops a thread's turn. A message says which thread it comes from. Where the
//! model asks to be told, a process beside the thread's main process waits on the turn that takes the message, then
//! tells the thread what came of it, as a task delegated in the background is told. No thread is sent a message by
//! one whose mode reaches less far, which would reach further through it; a sub-thread works with no other thread.

use std::fmt::Write as _;

use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use siphon::machine::Base;
use siphon::threads::{Joins, Listed, MarksChange};

use crate::conversation::{Keywords, Matching};
use crate::reading;
use crate::result::{ThreadResult, ThreadStatus, ThreadSummary};
use siphon::tools::{Context, Effect, Plan, Refusal, Report, Spawned, Told, Tool};

/// What the model does with the person's other threads.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ThreadAction {
    /// List the person's threads, the most recently active first.
    List {
        /// List archived threads too.
        #[serde(default)]
        archived: bool,
        /// Threads to skip, for the next page.
        #[serde(default)]
        offset: usize,
    },
    /// Find what was said and each call made in the threads that holds keywords, the newest first.
    Search {
        /// The words to find, separated by spaces, whatever their case.
        keywords: String,
        /// Whether an entry holds every keyword (`all`, AND) or any one (`any`, OR).
        #[serde(default)]
        matching: Matching,
        /// Search this thread's id alone.
        thread: Option<String>,
        /// Search archived threads too.
        #[serde(default)]
        archived: bool,
        /// Entries to skip, for the next page.
        #[serde(default)]
        offset: usize,
    },
    /// Read a thread's conversation: what was said and each call made, a page at a time, the newest page first.
    Read {
        /// The thread's id.
        thread: String,
        /// Read the conversation about this entry's id, as search gives it.
        around: Option<String>,
        /// Read the page before this entry's id, as a page's note gives it.
        before: Option<String>,
        /// Read the page after this entry's id, as a page's note gives it.
        after: Option<String>,
    },
    /// Create a thread in this folder with your model and mode; `message` is its first input.
    Start {
        /// Everything the new thread needs; it does not see this conversation.
        message: String,
        /// A short title, shown to the person.
        title: Option<String>,
        /// Get a message when the turn that takes `message` ends.
        #[serde(default)]
        report: bool,
    },
    /// Send a thread a message; it takes it at its next step, or starts a turn with it.
    Send {
        /// The thread's id.
        thread: String,
        message: String,
        /// Get a message when the turn that takes `message` ends.
        #[serde(default)]
        report: bool,
    },
    /// Stop the turn a thread is working on.
    Stop {
        /// The thread's id.
        thread: String,
    },
    /// Queue a new, nonempty title for the thread; surrounding whitespace is trimmed.
    Rename { thread: String, title: String },
    /// Pin a thread in the person's list.
    Pin { thread: String },
    /// Unpin a thread in the person's list.
    Unpin { thread: String },
    /// Archive a thread; its work is not stopped.
    Archive { thread: String },
    /// Restore an archived thread to the person's list.
    Unarchive { thread: String },
    /// Hide a thread until a future instant; its work is not stopped.
    Snooze {
        thread: String,
        /// A future ISO timestamp with an explicit UTC offset, such as `2026-12-01T09:00:00+08:00` or `2026-12-01T01:00:00Z`.
        until: String,
    },
    /// End a thread's snooze without starting its work.
    Unsnooze { thread: String },
}

/// `thread`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadTool;

impl Tool for ThreadTool {
    const NAME: &'static str = "thread";
    const REACHES_THREADS: bool = true;
    const DESCRIPTION: &'static str = "Work with the person's other threads, which they see in their thread list.\n- list: list the threads, the most recently active first: each one's id, how it stands, when it was last active, its project and its title.\n- search: find what was said in the threads: the words the person, the threads and the models said and the calls the models made that hold `keywords` — all of them, or any with `matching: any` — the newest first, each with its thread, its id and the words around the match; `thread` searches one thread alone, and this thread is searched only when named so.\n- read: read a thread's conversation — what the person and the model said, each call and how it ended — the newest page first; `around` an entry's id from search reads the conversation about it; page with `before` or `after` as a page's note says.\n- start: create a thread in this folder with your model and mode; `message` is its first input.\n- send: send a thread a message; it takes it at its next step, or starts a turn with it.\n- stop: stop the turn a thread is working on.\n- rename: request a new, nonempty title; it applies when the thread takes its next step.\n- pin/unpin, archive/unarchive, snooze/unsnooze: organize the person's thread list without starting or stopping work. Snooze requires a future ISO timestamp with an explicit UTC offset. These actions and rename can target this thread too.\nUse it only when the person asks about threads, asks for a new thread, or asks to message, stop or organize a named thread; never for sub-agents, delegation or parallel work. To find what was said about something earlier, search for it rather than page through a thread. A new thread does not see this conversation: put everything it needs in `message`. With `report`, you get a message when the turn that takes your message ends; do not wait for it or poll. Link a thread in your reply as :thread{id=<id>}, or as ::thread{id=<id>} on its own line to show its card.";
    type Args = ThreadAction;

    fn output_schema() -> Option<serde_json::Value> {
        Some(schemars::schema_for!(ThreadResult).to_value())
    }

    fn effect(&self, action: &ThreadAction, _: &Base) -> Effect {
        match action {
            ThreadAction::List { .. } | ThreadAction::Search { .. } | ThreadAction::Read { .. } => Effect::Reads,
            _ => Effect::Delegates,
        }
    }

    async fn exec(&self, context: &Context, action: ThreadAction) -> Result<Told, Refusal> {
        let mut context = context.clone();
        context.cancelled = context.cancelled.child_token();
        let _cancel_on_drop = context.cancelled.clone().drop_guard();
        tokio::task::spawn_blocking(move || {
            reading::check_cancelled(&context)?;
            planned(&context, action)
        })
        .await
        .map_err(|error| Refusal(format!("reading threads failed: {error}")))?
    }
}

/// How many threads a page of the list holds.
const PAGE: usize = 50;

/// The page of the person's threads from `offset` on, the most recently active first — those archived among them where
/// `archived` — each a line: its id, how it stands, when it was last active, its project and its title.
fn listed(context: &Context, archived: bool, offset: usize) -> Result<Told, Refusal> {
    let mut rows = siphon::threads::list(&context.volume()).map_err(|error| Refusal(error.to_string()))?;
    rows.retain(|row| archived || !row.marks.archived);
    rows.sort_by_key(|row| std::cmp::Reverse(active_at(row)));
    let all = rows.len();
    let page: Vec<String> = rows.iter().skip(offset).take(PAGE).map(|row| line(row, &context.thread)).collect();
    let shown = offset + page.len();
    let note = if page.is_empty() {
        format!("no threads past offset {offset}; there are {all}")
    } else if shown < all {
        format!("threads {}-{shown} of {all}; more from offset {shown}", offset + 1)
    } else {
        format!("{all} threads")
    };
    let result = ThreadResult::List {
        threads: rows
            .iter()
            .skip(offset)
            .take(PAGE)
            .map(|row| ThreadSummary::of(row, &context.thread))
            .collect(),
        offset,
        total: all,
        next: (shown < all).then_some(shown),
    };
    Ok(reading::told(&page, &[note], &result))
}

/// When `row` was last active, in ms since the epoch: as its last turn ended, else as it began, else as it was made.
pub(crate) fn active_at(row: &Listed) -> i64 {
    row.status
        .ended_at
        .max(row.status.began_at)
        .or_else(|| siphon::threads::time_of(&row.id))
        .unwrap_or(0)
}

/// `row` as the list tells it, `own` the thread that asks: its id; how it stands — what it asks the person first, then
/// whether it works, then how its last turn ended; when it was last active; its project, or none; its title; and
/// whose sub-thread it is, whether it is archived, or whether it is the thread that asks.
fn line(row: &Listed, own: &str) -> String {
    let stands = match ThreadStatus::of(row) {
        ThreadStatus::Asking => "asks the person",
        ThreadStatus::Working => "working",
        ThreadStatus::New => "new",
        ThreadStatus::Replied => "replied",
        ThreadStatus::Failed => "failed",
        ThreadStatus::Stopped => "stopped",
    };
    let project = row.project.as_deref().unwrap_or("no project");
    let title = row.config.title.as_deref().unwrap_or("untitled");
    format!(
        "{}  {stands}  {}  {project}  {title}{}",
        row.id,
        time(active_at(row)),
        marks(row, own)
    )
}

/// The time `ms`, in milliseconds since the epoch, as a thread's tool tells it: to the second, in UTC.
pub(crate) fn time(ms: i64) -> String {
    jiff::Timestamp::from_millisecond(ms).map_or_else(|_| "-".to_owned(), |at| format!("{at:.0}"))
}

/// What a thread's line says of `row` past its title, `own` the thread that asks: whose sub-thread it is, whether it is
/// archived, and whether it is the thread that asks.
pub(crate) fn marks(row: &Listed, own: &str) -> String {
    let mut marks = String::new();
    if let Some(parent) = &row.parent {
        let _ = write!(marks, "  (sub-thread of {})", parent.thread);
    }
    if row.marks.pinned {
        marks.push_str("  (pinned)");
    }
    if let Some(until) = row.marks.snoozed_until {
        let _ = write!(marks, "  (snoozed until {})", time(until));
    }
    if row.marks.archived {
        marks.push_str("  (archived)");
    }
    if row.id == own {
        marks.push_str("  (this thread)");
    }
    marks
}

/// What a `thread` call does, planned for its caller to do with the call's end — a list, nothing — and what it tells.
#[expect(
    clippy::too_many_lines,
    reason = "the explicit thread actions each plan their operation and business result in one exhaustive dispatch"
)]
fn planned(context: &Context, action: ThreadAction) -> Result<Told, Refusal> {
    let own = context.thread.as_str();
    let mut plan = Plan::default();
    let (text, result) = match action {
        ThreadAction::List { archived, offset } => return listed(context, archived, offset),
        ThreadAction::Search {
            keywords,
            matching,
            thread,
            archived,
            offset,
        } => {
            let keywords = Keywords::of(&keywords, matching);
            return reading::search(context, &keywords, thread.as_deref(), archived, offset);
        }
        ThreadAction::Read {
            thread,
            around,
            before,
            after,
        } => return reading::read(context, &thread, &reading::Page::of(around, before, after)?),
        ThreadAction::Start { message, title, report } => {
            let started = plan.spawn(&context.volume(), title, Spawned::Thread)?;
            let delivery = plan.deliver(&context.volume(), &started, from(own, &message), Joins::Step, reported(report))?;
            let text = format!(
                "<system-reminder>thread {started} started with your message{}</system-reminder>\n<system-reminder>link it for the person as :thread{{id={started}}}</system-reminder>",
                told_note(report)
            );
            (
                text,
                ThreadResult::Start {
                    thread: started,
                    delivery,
                    report,
                },
            )
        }
        ThreadAction::Send { thread, message, report } => {
            let delivery = plan.deliver(&context.volume(), &thread, from(own, &message), Joins::Step, reported(report))?;
            let text = format!(
                "<system-reminder>thread {thread} has your message{}</system-reminder>",
                told_note(report)
            );
            (text, ThreadResult::Send { thread, delivery, report })
        }
        ThreadAction::Stop { thread } => {
            plan.stop(&thread);
            (
                format!("<system-reminder>thread {thread} was asked to stop its turn</system-reminder>"),
                ThreadResult::Stop { thread },
            )
        }
        ThreadAction::Rename { thread, title } => {
            let delivery = plan.rename(&context.volume(), &thread, &title)?;
            let title = title.trim().to_owned();
            (
                notice(&thread, "was asked to change its title"),
                ThreadResult::RenameRequested { thread, delivery, title },
            )
        }
        ThreadAction::Pin { thread } => {
            plan.mark(
                &thread,
                MarksChange {
                    pinned: Some(true),
                    ..MarksChange::default()
                },
            );
            (notice(&thread, "was pinned"), ThreadResult::Pinned { thread })
        }
        ThreadAction::Unpin { thread } => {
            plan.mark(
                &thread,
                MarksChange {
                    pinned: Some(false),
                    ..MarksChange::default()
                },
            );
            (notice(&thread, "was unpinned"), ThreadResult::Unpinned { thread })
        }
        ThreadAction::Archive { thread } => {
            plan.mark(
                &thread,
                MarksChange {
                    archived: Some(true),
                    ..MarksChange::default()
                },
            );
            (
                notice(&thread, "was archived; its work was not stopped"),
                ThreadResult::Archived { thread },
            )
        }
        ThreadAction::Unarchive { thread } => {
            plan.mark(
                &thread,
                MarksChange {
                    archived: Some(false),
                    ..MarksChange::default()
                },
            );
            (notice(&thread, "was unarchived"), ThreadResult::Unarchived { thread })
        }
        ThreadAction::Snooze { thread, until } => {
            let until = snoozed_until(&until, context.volume().now_ms())?;
            plan.mark(
                &thread,
                MarksChange {
                    snoozed_until: Some(until),
                    ..MarksChange::default()
                },
            );
            (
                notice(&thread, &format!("was snoozed until {}; its work was not stopped", time(until))),
                ThreadResult::Snoozed { thread, until },
            )
        }
        ThreadAction::Unsnooze { thread } => {
            plan.mark(
                &thread,
                MarksChange {
                    snoozed_until: Some(0),
                    ..MarksChange::default()
                },
            );
            (notice(&thread, "was unsnoozed"), ThreadResult::Unsnoozed { thread })
        }
    };
    Ok(Told {
        plan,
        ..result.told(vec![InputContent::text(text)])
    })
}

fn notice(thread: &str, action: &str) -> String {
    format!("<system-reminder>thread {thread} {action}</system-reminder>")
}

/// The explicit instant the snooze ends; a past instant is refused rather than implicitly clearing a snooze.
fn snoozed_until(until: &str, now: i64) -> Result<i64, Refusal> {
    let until = until
        .parse::<jiff::Timestamp>()
        .map_err(|error| Refusal(format!("give a snooze timestamp with an explicit UTC offset: {error}")))?
        .as_millisecond();
    if until <= now {
        return Err(Refusal("snooze until must be in the future".to_owned()));
    }
    Ok(until)
}

/// `message` as the thread `own` sends it: saying whose it is.
fn from(own: &str, message: &str) -> Vec<InputContent> {
    vec![
        InputContent::text(format!("<system-reminder>message from thread {own}</system-reminder>")),
        InputContent::text(siphon::reminder::escaped(message)),
    ]
}

/// Who is told how the turn that takes a message ends: the calling thread, by a message, where it asked; no one else.
fn reported(report: bool) -> Report {
    if report { Report::Inbox } else { Report::None }
}

/// What a call that asked to be told is answered of it.
fn told_note(report: bool) -> &'static str {
    if report {
        "; you will get a message when the turn that takes it ends: do not wait for it or poll"
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snooze_requires_an_explicit_offset_and_a_future_instant() {
        assert_eq!(snoozed_until("1970-01-01T08:00:01+08:00", 0).unwrap(), 1_000);
        assert_eq!(snoozed_until("1970-01-01T00:00:01.123Z", 0).unwrap(), 1_123);
        for until in ["1970-01-01T00:00:01", "1970-01-01", "tomorrow", "1970-01-01T00:00:00Z"] {
            assert!(snoozed_until(until, 0).is_err(), "{until}");
        }
        assert!(snoozed_until("1970-01-01T00:00:01Z", 1_000).unwrap_err().0.contains("future"));
    }
}
