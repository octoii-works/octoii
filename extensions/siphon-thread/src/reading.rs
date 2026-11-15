//! How the `thread` tool finds and reads what was said in the person's threads, as grep finds lines and less shows
//! them where they are: a search gives the entries of the threads' conversations that hold keywords — what was said
//! and each call made, not what a call returned, which tells again what was searched already — the newest first,
//! each with its thread, its id and the words around the match; a read gives a page of one conversation — the
//! newest, the one before or after an entry, or the one about an entry a search found.

use octoii_ai::vocabulary::InputContent;
use siphon::threads::{Entry, Listed};
use siphon::tools::{Context, Refusal, Told};

use crate::conversation::{self, Keywords};
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

    /// Where the page lies among `entries`, oldest first, from its first to past its last.
    fn span(&self, entries: &[Entry]) -> Result<std::ops::Range<usize>, Refusal> {
        let at = |id: &str| entries.partition_point(|entry| entry.id() < id);
        let (start, end) = match self {
            Self::Newest => (entries.len().saturating_sub(ENTRIES), entries.len()),
            Self::Before(id) => {
                let end = at(id);
                (end.saturating_sub(ENTRIES), end)
            }
            Self::After(id) => {
                let start = entries.partition_point(|entry| entry.id() <= id.as_str());
                (start, entries.len().min(start + ENTRIES))
            }
            Self::Around(id) => {
                let at = at(id);
                if entries.get(at).is_none_or(|entry| entry.id() != id) {
                    return Err(Refusal(format!("no entry {id} in this thread")));
                }
                let start = at.saturating_sub(ENTRIES / 2);
                (start, entries.len().min(start + ENTRIES))
            }
        };
        Ok(start..end)
    }
}

/// The page `page` of the conversation of the thread `thread`, oldest first, each entry as `conversation::told` tells
/// it; and notes of where the pages before and after it begin, or that it reaches the conversation's start.
pub fn read(context: &Context, thread: &str, page: &Page) -> Result<Told, Refusal> {
    let entries = entries_of(context, thread)?;
    let span = page.span(&entries)?;
    let shown: Vec<String> = entries[span.clone()].iter().filter_map(conversation::told).collect();
    let mut notes = Vec::new();
    match span.start {
        0 => notes.push("this reaches the conversation's start".to_owned()),
        start => notes.push(format!("older entries from before={}", entries[start].id())),
    }
    if span.end < entries.len() && span.end > 0 {
        notes.push(format!("newer entries from after={}", entries[span.end - 1].id()));
    }
    Ok(told(&shown, &notes))
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
    let mut hits = Vec::new();
    for row in &rows {
        for entry in &entries_of(context, &row.id)? {
            if !matches!(entry, Entry::Result { .. })
                && let Some(told) = conversation::told(entry)
                && let Some(words) = keywords.around(&told)
            {
                let entry = entry.id().to_owned();
                hits.push(Hit { row, entry, words });
            }
        }
    }
    hits.sort_by(|one, other| other.entry.cmp(&one.entry));
    let all = hits.len();
    let page: Vec<String> = hits
        .iter()
        .skip(offset)
        .take(HITS)
        .map(|hit| hit_told(hit, &context.thread))
        .collect();
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
    Ok(told(&page, &[note]))
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

/// The entries of the conversation of the thread `thread`, oldest first.
fn entries_of(context: &Context, thread: &str) -> Result<Vec<Entry>, Refusal> {
    siphon::threads::entries(&context.volume(), thread)
        .map_err(|error| Refusal(error.to_string()))?
        .ok_or_else(|| Refusal(format!("no thread {thread}")))
}

/// `lines`, then each of `notes` as a reminder, as a call tells them.
pub fn told(lines: &[String], notes: &[String]) -> Told {
    let mut text: Vec<String> = lines.to_vec();
    text.extend(notes.iter().map(|note| format!("<system-reminder>{note}</system-reminder>")));
    Told::from(vec![InputContent::text(text.join("\n"))])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_meet_without_repeating_or_skipping_entries() {
        let entries: Vec<Entry> = (0..150)
            .map(|at| Entry::Input {
                id: format!("{at:03}"),
                actor: "person".to_owned(),
                delivery: format!("delivery-{at}"),
                content: vec![InputContent::text(format!("message {at}"))],
                reply_to: None,
            })
            .collect();
        assert_eq!(Page::Newest.span(&entries).unwrap(), 90..150);
        assert_eq!(Page::Before("090".to_owned()).span(&entries).unwrap(), 30..90);
        assert_eq!(Page::Before("030".to_owned()).span(&entries).unwrap(), 0..30);
        assert_eq!(Page::After("029".to_owned()).span(&entries).unwrap(), 30..90);
        assert_eq!(Page::After("089".to_owned()).span(&entries).unwrap(), 90..150);
        assert_eq!(Page::Around("075".to_owned()).span(&entries).unwrap(), 45..105);
        assert_eq!(Page::Before("000".to_owned()).span(&entries).unwrap(), 0..0);
        assert_eq!(Page::After("149".to_owned()).span(&entries).unwrap(), 150..150);
        assert_eq!(Page::Newest.span(&[]).unwrap(), 0..0);
        assert!(Page::Around("missing".to_owned()).span(&entries).is_err());
    }
}
