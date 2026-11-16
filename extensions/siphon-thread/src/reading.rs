//! How the `thread` tool finds and reads what was said in the person's threads, as grep finds lines and less shows
//! them where they are: a search gives the entries of the threads' conversations that hold keywords — what was said
//! and each call made, not what a call returned, which tells again what was searched already — the newest first,
//! each with its thread, its id and the words around the match; a read gives a page of one conversation — the
//! newest, the one before or after an entry, or the one about an entry a search found.

use octoii_ai::vocabulary::InputContent;
use siphon::threads::{Entry, History, Listed};
use siphon::tools::{Context, Refusal, Told};

use crate::conversation::{self, Keywords};
use crate::result::{ConversationEntry, SearchHit, ThreadResult, ThreadSummary};
use crate::tool::{marks, time};

/// How many entries a page of a conversation holds.
const ENTRIES: usize = 60;

/// How many entries a page of a search holds.
const HITS: usize = 20;

/// Which page of a conversation is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    /// The newest.
    Newest,
    /// The one about this entry: it in the middle.
    Around(String),
    /// The one that ends before this entry.
    Before(String),
    /// The one that begins after this entry.
    After(String),
}

impl Page {
    /// The page a call names by `around`, `before` or `after`, the newest where it names none.
    ///
    /// # Errors
    /// A call that names more than one.
    pub fn of(around: Option<String>, before: Option<String>, after: Option<String>) -> Result<Self, Refusal> {
        match (around, before, after) {
            (None, None, None) => Ok(Self::Newest),
            (Some(entry), None, None) => Ok(Self::Around(entry)),
            (None, Some(entry), None) => Ok(Self::Before(entry)),
            (None, None, Some(entry)) => Ok(Self::After(entry)),
            _ => Err(Refusal("give one of around, before and after".to_owned())),
        }
    }

    /// Read just the page requested, using entry IDs as anchors in one snapshot.
    fn entries(&self, history: &History) -> anyhow::Result<Vec<Entry>> {
        match self {
            Self::Newest => history.before(None, ENTRIES),
            Self::Before(id) => history.before(Some(id), ENTRIES),
            Self::After(id) => history.after(Some(id), ENTRIES),
            Self::Around(id) => {
                let older = history.before(Some(id), ENTRIES / 2 + 1)?;
                let anchor = (older.len() > ENTRIES / 2).then(|| older[0].id());
                let entries = history.after(anchor, ENTRIES)?;
                anyhow::ensure!(entries.iter().any(|entry| entry.id() == id), "no entry {id} in this thread");
                Ok(entries)
            }
        }
    }
}

/// A page of a conversation, with entry IDs for navigating to adjacent pages.
pub fn read(context: &Context, thread: &str, page: &Page) -> Result<Told, Refusal> {
    let history = history_of(context, thread)?;
    let entries = page.entries(&history).map_err(|error| Refusal(error.to_string()))?;
    check_cancelled(context)?;
    let visible: Vec<ConversationEntry> = entries.iter().filter_map(conversation::entry).collect();
    let shown: Vec<String> = visible.iter().map(ConversationEntry::told).collect();
    let mut notes = Vec::new();
    let mut before = None;
    let mut after = None;
    if let Some(first) = entries.first() {
        if history
            .before(Some(first.id()), 1)
            .map_err(|error| Refusal(error.to_string()))?
            .is_empty()
        {
            notes.push("this reaches the conversation's start".to_owned());
        } else {
            notes.push(format!("older entries from before={}", first.id()));
            before = Some(first.id().to_owned());
        }
        let last = entries.last().expect("a nonempty page");
        if !history
            .after(Some(last.id()), 1)
            .map_err(|error| Refusal(error.to_string()))?
            .is_empty()
        {
            notes.push(format!("newer entries from after={}", last.id()));
            after = Some(last.id().to_owned());
        }
    } else {
        notes.push("no entries on this page".to_owned());
    }
    let result = ThreadResult::Read {
        thread: thread.to_owned(),
        entries: visible,
        before,
        after,
    };
    Ok(told(&shown, &notes, &result))
}

/// Cancellation is checked between pages and entries, including while a blocking scan is running.
pub(crate) fn check_cancelled(context: &Context) -> Result<(), Refusal> {
    if context.cancelled.is_cancelled() {
        return Err(Refusal("thread history read cancelled".to_owned()));
    }
    Ok(())
}

/// A reverse scan holds one decoded entry and a bounded byte buffer for each conversation.
struct Scan {
    entries: siphon::threads::Entries,
    next: Option<Entry>,
}

impl Scan {
    fn advance(&mut self) -> Result<(), Refusal> {
        self.next = self.entries.next().transpose().map_err(|error| Refusal(error.to_string()))?;
        Ok(())
    }
}

/// A search's find: an entry of a thread's that holds the keywords, and the words of it around the first.
struct Hit<'a> {
    row: &'a Listed,
    entry: String,
    words: String,
}

/// The page from `offset` on of the entries of the person's threads that hold `keywords`, results aside, the newest
/// first — of the thread `thread` alone where named, else of every thread but the one that asks, archived ones among
/// them where `archived` — each told by its time, its thread, its id and the words around the match.
pub fn search(context: &Context, keywords: &Keywords, thread: Option<&str>, archived: bool, offset: usize) -> Result<Told, Refusal> {
    if keywords.are_none() {
        return Err(Refusal("give keywords to search for".to_owned()));
    }
    let mut rows = siphon::threads::list(&context.volume()).map_err(|error| Refusal(error.to_string()))?;
    match thread {
        Some(thread) => rows.retain(|row| row.id == thread),
        None => rows.retain(|row| (archived || !row.marks.archived) && row.id != context.thread),
    }
    if let (Some(thread), true) = (thread, rows.is_empty()) {
        return Err(Refusal(format!("no thread {thread}")));
    }
    // Merge the conversations by their next entry ID. Only one buffered scan per thread and
    // the requested result page are held, regardless of the number of matches or offset.
    let mut scans = Vec::new();
    let mut newest = std::collections::BinaryHeap::new();
    for (index, row) in rows.iter().enumerate() {
        let mut scan = Scan {
            entries: history_of(context, &row.id)?
                .reverse()
                .map_err(|error| Refusal(error.to_string()))?,
            next: None,
        };
        scan.advance()?;
        if let Some(entry) = &scan.next {
            newest.push((entry.id().to_owned(), index));
        }
        scans.push(scan);
    }
    let mut all = 0;
    let mut page = Vec::new();
    while let Some((_, index)) = newest.pop() {
        check_cancelled(context)?;
        let scan = &mut scans[index];
        let entry = scan.next.take().expect("the heap names a pending entry");
        if !matches!(entry, Entry::Result { .. })
            && let Some(text) = conversation::told(&entry)
            && let Some(words) = keywords.around(&text)
        {
            if all >= offset && page.len() < HITS {
                page.push(Hit {
                    row: &rows[index],
                    entry: entry.id().to_owned(),
                    words,
                });
            }
            all += 1;
        }
        scan.advance()?;
        if let Some(entry) = &scan.next {
            newest.push((entry.id().to_owned(), index));
        }
    }
    let shown = offset + page.len();
    let note = if all == 0 {
        "no entries hold those keywords".to_owned()
    } else if page.is_empty() {
        format!("no entries past offset {offset}; there are {all}")
    } else if shown < all {
        format!("entries {}-{shown} of {all}; more from offset {shown}", offset + 1)
    } else {
        format!("{all} entries")
    };
    let shown_lines: Vec<String> = page.iter().map(|hit| hit_told(hit, &context.thread)).collect();
    let result = ThreadResult::Search {
        hits: page
            .into_iter()
            .map(|hit| SearchHit {
                thread: ThreadSummary::of(hit.row, &context.thread),
                at: siphon::threads::time_of(&hit.entry),
                entry: hit.entry,
                excerpt: hit.words,
            })
            .collect(),
        offset,
        total: all,
        next: (shown < all).then_some(shown),
    };
    Ok(told(&shown_lines, &[note], &result))
}

/// `hit` as a search tells it, `own` the thread that asks: when its entry was made, its thread's id and title marked as
/// the list marks them, then its entry's id and the words around the match.
fn hit_told(hit: &Hit<'_>, own: &str) -> String {
    let when = siphon::threads::time_of(&hit.entry).map_or_else(|| "-".to_owned(), time);
    let title = hit.row.config.title.as_deref().unwrap_or("untitled");
    format!(
        "{when}  {}  {title}{}\n  entry {}: {}",
        hit.row.id,
        marks(hit.row, own),
        hit.entry,
        hit.words
    )
}

/// Open a stable view of a conversation through Siphon's public interface.
fn history_of(context: &Context, thread: &str) -> Result<History, Refusal> {
    check_cancelled(context)?;
    History::open(&context.volume(), thread)
        .map_err(|error| Refusal(error.to_string()))?
        .ok_or_else(|| Refusal(format!("no thread {thread}")))
}

/// `lines`, then each of `notes` as a reminder, as a call tells them.
pub fn told(lines: &[String], notes: &[String], result: &ThreadResult) -> Told {
    let mut text: Vec<String> = lines.to_vec();
    text.extend(notes.iter().map(|note| format!("<system-reminder>{note}</system-reminder>")));
    result.told(vec![InputContent::text(text.join("\n"))])
}
