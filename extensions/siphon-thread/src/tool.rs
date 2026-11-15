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
use std::future::Future;

use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use siphon::machine::Base;
use siphon::threads::{Joins, Listed, Turn};

use crate::conversation::{Keywords, Matching};
use crate::reading;
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
}

/// `thread`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadTool;

impl Tool for ThreadTool {
    const NAME: &'static str = "thread";
    const REACHES_THREADS: bool = true;
    const DESCRIPTION: &'static str = "Work with the person's other threads, which they see in their thread list.\n- list: list the threads, the most recently active first: each one's id, how it stands, when it was last active, its project and its title.\n- search: find what was said in the threads: the words the person, the threads and the models said and the calls the models made that hold `keywords` — all of them, or any with `matching: any` — the newest first, each with its thread, its id and the words around the match; `thread` searches one thread alone, and this thread is searched only when named so.\n- read: read a thread's conversation — what the person and the model said, each call and how it ended — the newest page first; `around` an entry's id from search reads the conversation about it; page with `before` or `after` as a page's note says.\n- start: create a thread in this folder with your model and mode; `message` is its first input.\n- send: send a thread a message; it takes it at its next step, or starts a turn with it.\n- stop: stop the turn a thread is working on.\nUse it only when the person asks about other threads, asks for a new thread, or names a thread to message or stop; never for sub-agents, delegation or parallel work. To find what was said about something earlier, search for it rather than page through a thread. A new thread does not see this conversation: put everything it needs in `message`. With `report`, you get a message when the turn that takes your message ends; do not wait for it or poll. Link a thread in your reply as :thread{id=<id>}, or as ::thread{id=<id>} on its own line to show its card.";
    type Args = ThreadAction;

    fn effect(&self, action: &ThreadAction, _: &Base) -> Effect {
        match action {
            ThreadAction::List { .. } | ThreadAction::Search { .. } | ThreadAction::Read { .. } => Effect::Reads,
            _ => Effect::Delegates,
        }
    }

    fn exec(&self, context: &Context, action: ThreadAction) -> impl Future<Output = Result<Told, Refusal>> + Send {
        std::future::ready(planned(context, action))
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
    Ok(reading::told(&page, &[note]))
}

/// When `row` was last active, in ms since the epoch: as its last turn ended, else as it began, else as it was made.
fn active_at(row: &Listed) -> i64 {
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
    let stands = if !row.status.asks.is_empty() {
        "asks the person"
    } else if row.status.turn == Turn::Running || !row.status.inbox.is_empty() {
        "working"
    } else {
        match row.status.turn {
            Turn::Created => "new",
            Turn::Running | Turn::Replied => "replied",
            Turn::Failed => "failed",
            Turn::Stopped => "stopped",
        }
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
    if row.marks.archived {
        marks.push_str("  (archived)");
    }
    if row.id == own {
        marks.push_str("  (this thread)");
    }
    marks
}

/// What a `thread` call does, planned for its caller to do with the call's end — a list, nothing — and what it tells.
fn planned(context: &Context, action: ThreadAction) -> Result<Told, Refusal> {
    let own = context.thread.as_str();
    let mut plan = Plan::default();
    let text = match action {
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
            plan.deliver(&context.volume(), &started, from(own, &message), Joins::Step, reported(report))?;
            format!(
                "<system-reminder>thread {started} started with your message{}</system-reminder>\n<system-reminder>link it for the person as :thread{{id={started}}}</system-reminder>",
                told_note(report)
            )
        }
        ThreadAction::Send { thread, message, report } => {
            plan.deliver(&context.volume(), &thread, from(own, &message), Joins::Step, reported(report))?;
            format!(
                "<system-reminder>thread {thread} has your message{}</system-reminder>",
                told_note(report)
            )
        }
        ThreadAction::Stop { thread } => {
            plan.stop(&thread);
            format!("<system-reminder>thread {thread} was asked to stop its turn</system-reminder>")
        }
    };
    Ok(Told {
        plan,
        ..Told::from(vec![InputContent::text(text)])
    })
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
