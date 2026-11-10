//! Compaction: where a window of the conversation grows past what its model takes, everything before its latest work
//! is recapped by the model, and a new window begins — what the model is told, gathered anew, the recap, then the
//! person's latest input and the latest work, kept as they stand: nothing copied, every entry once in the log. A window
//! is cut before a step of the conversation — an input, or a reply with what came of it — so a call is never kept
//! without its result, and a cut within a turn keeps the input that began it as well. One process of the thread
//! compacts, keyed `compact` beside its main process: the person starts it as they ask, a turn as its window is too full
//! for its next request or its provider refused one as too long; a thread compacts once at a time. The segments of the
//! log wholly before the new window expire after the history's retention.

use beak::layout;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use siphon_runtime::{Answer, Changes, Ending, Program, Run, Wait};
use siphon_threads::log::{self, Writer};
use siphon_threads::window::{self, FROM_ANNOTATION};
use siphon_threads::{Config, Entry, Window, thread};

use crate::gather::Gather;
use crate::model::ModelRequest;
use crate::told::Told;
use crate::turn::charge;

/// The tokens a window keeps free, past its model's context less them it is compacted: room for what the next request
/// adds and for the reply.
pub const RESERVE_TOKENS: u64 = 16 * 1024;
/// The tokens of the latest entries a compaction keeps as they stand, at most — though always from the latest step on.
pub const KEPT_TOKENS: u64 = 20_000;

/// What a recap request asks of the model, after the conversation it recaps.
pub const RECAP: &str = "Write a recap of the conversation above, for a model that carries on the work with the recap in place of it, then the person's latest request and the latest messages as they are. That model is given the instructions and the environment anew, so leave them out. Keep:
- what the person asked for, in their own words where the words matter, and what is still to do;
- what was done and found: the files, the symbols, the commands and what they gave, the errors, word for word;
- the decisions made, and why;
- where the work stands: what is changed, what is tested, what is committed.
Where the conversation begins with an earlier recap, carry forward what still holds: this recap takes its place. Write the recap alone, with no greeting, no question and no tool call.";

/// Where a window is cut: the input it keeps as the person gave it, the latest before the cut — none where the cut
/// falls at an input; the first entry it keeps after; and the last entry the recap covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cut {
    pub kept: Option<String>,
    pub from: String,
    pub until: String,
}

/// The tokens an entry takes, as near as its bytes tell: one for every four.
fn tokens_of(entry: &Entry) -> u64 {
    serde_json::to_vec(entry).map_or(0, |bytes| bytes.len() as u64 / 4)
}

/// The tokens the window's next request takes, as near as its log tells: what its last request took, as its own last
/// reply says — its context, and what it wrote — and a token for every four bytes of what came since; of the whole
/// window where no reply of its own says.
#[must_use]
pub fn tokens(window: &Window) -> u64 {
    let said = window
        .conversation
        .iter()
        .enumerate()
        .skip(window.carried)
        .rev()
        .find_map(|(at, entry)| {
            let Entry::Reply { response, .. } = entry else { return None };
            let used = response.usage.as_ref()?;
            Some((at, response.context_tokens.unwrap_or(used.input_tokens) + used.output_tokens))
        });
    if let Some((at, context)) = said {
        return context + window.conversation[at + 1..].iter().map(tokens_of).sum::<u64>();
    }
    let recap = window.recap.as_ref().map_or(0, |recap| recap.len() as u64 / 4);
    window.told.iter().chain(&window.conversation).map(tokens_of).sum::<u64>() + recap
}

/// Whether `window` is past what a model of `context_length` tokens takes, less `RESERVE_TOKENS`; never where the
/// model's context is not known, which its provider's refusal of a request too long tells instead.
#[must_use]
pub fn is_full(window: &Window, context_length: Option<u64>) -> bool {
    context_length.is_some_and(|context| tokens(window) + RESERVE_TOKENS > context)
}

/// Where `window` is cut: before the earliest step whose entries from it on take at most `KEPT_TOKENS`, else before the
/// latest, the input of the step's turn kept with it; none where nothing but that input stands before it, to recap.
#[must_use]
pub fn cut(window: &Window) -> Option<Cut> {
    let entries = &window.conversation;
    let steps: Vec<usize> = (1..entries.len())
        .filter(|&at| matches!(entries[at], Entry::Input { .. } | Entry::Reply { .. }))
        .collect();
    let mut chosen = *steps.last()?;
    let (mut kept_tokens, mut next) = (0, entries.len());
    for &step in steps.iter().rev() {
        kept_tokens += entries[step..next].iter().map(tokens_of).sum::<u64>();
        next = step;
        if kept_tokens > KEPT_TOKENS {
            break;
        }
        chosen = step;
    }
    let kept = match &entries[chosen] {
        Entry::Input { .. } => None,
        _ => entries[..chosen]
            .iter()
            .rev()
            .find(|entry| matches!(entry, Entry::Input { .. }))
            .map(|input| input.id().to_owned()),
    };
    let recapped = entries[..chosen]
        .iter()
        .any(|entry| Some(entry.id()) != kept.as_deref() && !matches!(entry, Entry::Config { .. } | Entry::System { .. }));
    recapped.then(|| Cut {
        kept,
        from: entries[chosen].id().to_owned(),
        until: entries[chosen - 1].id().to_owned(),
    })
}

/// What a compaction is started with: its thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactInput {
    pub thread: String,
}

/// Where a compaction stands: its window recapped up to its cut by the request of `key`; then what the model is told
/// gathered anew by the request of `key`, its recap written by the request of `recap`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum CompactState {
    Recapping {
        key: String,
        cut: Cut,
    },
    Gathering {
        key: String,
        cut: Cut,
        recap: String,
        response: Box<octoii_ai::vocabulary::Response>,
    },
}

/// The program of a thread's compaction, keyed `KEY` under its main process.
#[derive(Debug, Clone, Copy, Default)]
pub struct CompactProgram;

impl CompactProgram {
    pub const KEY: &'static str = "compact";
}

/// The path of the compaction of the thread `id`.
#[must_use]
pub fn path(id: &str) -> Bytes {
    layout::process(&thread::main(id), CompactProgram::KEY.as_bytes())
}

/// Adds to `changes` the compaction of the thread `id` started, where it is not running — one that ended reaped as it
/// is started anew; where it runs, nothing, as a thread compacts once at a time.
///
/// # Errors
/// The volume's failure.
pub fn start(snapshot: &beak::Snapshot, changes: &mut Changes, id: &str) -> anyhow::Result<()> {
    match snapshot.record(&path(id))? {
        Some(record) if record.outcome.is_none() => return Ok(()),
        Some(_) => {
            changes.reap(path(id));
        }
        None => {}
    }
    changes.start::<CompactProgram>(thread::main(id), CompactProgram::KEY, &CompactInput { thread: id.to_owned() })?;
    Ok(())
}

impl Program for CompactProgram {
    const KIND: &'static str = "siphon.compact";
    type Input = CompactInput;
    type Checkpoint = CompactState;
    type Output = ();

    fn run(&self, run: &mut Run<'_>, input: CompactInput, state: Option<CompactState>) -> anyhow::Result<Ending<CompactState, ()>> {
        let id = &input.thread;
        match state {
            None => recap(run, id),
            Some(CompactState::Recapping { key, cut }) => recapped(run, id, key, cut),
            Some(CompactState::Gathering { key, cut, recap, response }) => compacted(run, id, &key, &cut, recap, response),
        }
    }
}

fn config_of(run: &Run<'_>, id: &str) -> anyhow::Result<Config> {
    let directory = run
        .snapshot()
        .stat(&thread::path(id))?
        .and_then(|entry| entry.directory)
        .unwrap_or_default();
    Ok(Config::of(&directory)?)
}

/// Asks the model to recap the window up to its cut, and sleeps till it answers; done where there is nothing to recap.
fn recap(run: &mut Run<'_>, id: &str) -> anyhow::Result<Ending<CompactState, ()>> {
    let Some(model) = config_of(run, id)?.model else {
        return Ok(Ending::Fail("the thread names no model".into()));
    };
    let window = Window::read(run.snapshot(), id)?;
    let Some(cut) = cut(&window) else { return Ok(Ending::Done(())) };
    let key = run.new_id()?;
    let request = ModelRequest {
        thread: id.to_owned(),
        until: cut.until.clone(),
        model,
        instruction: Some(RECAP.to_owned()),
    };
    run.request(&key, &request)?;
    Ok(Ending::Wait(
        CompactState::Recapping { key: key.clone(), cut },
        Wait::new().mail(key),
    ))
}

/// The recap came: what the model is told gathered anew; a recap that came to nothing kept, and the compaction failed.
fn recapped(run: &mut Run<'_>, id: &str, key: String, cut: Cut) -> anyhow::Result<Ending<CompactState, ()>> {
    let Some(answer) = run.answer::<ModelRequest>(&key)? else {
        let wait = Wait::new().mail(key.clone());
        return Ok(Ending::Wait(CompactState::Recapping { key, cut }, wait));
    };
    run.take_mail(&key);
    let response = match answer {
        Answer::Given(response) => response,
        Answer::Interrupted => return Ok(Ending::Fail("the recap was cut off".into())),
        Answer::Failed(words) => return Ok(Ending::Fail(words)),
    };
    charge(run, &response);
    if response.words().is_empty() {
        let why = response
            .failure
            .as_ref()
            .map_or_else(|| "the model wrote no recap".to_owned(), |failure| failure.message.clone());
        let mut writer = Writer::open(run.snapshot(), id)?;
        let entry = Entry::Discarded {
            id: run.new_id()?,
            request: key,
            response,
        };
        writer.append(&mut run.changes, &entry)?;
        return Ok(Ending::Fail(why));
    }
    let gathering = run.new_id()?;
    run.request(&gathering, &Gather { thread: id.to_owned() })?;
    let wait = Wait::new().mail(gathering.clone());
    Ok(Ending::Wait(
        CompactState::Gathering {
            key: gathering,
            cut,
            recap: key,
            response: Box::new(response),
        },
        wait,
    ))
}

/// What the model is told came: the compaction appended, and all it is told whole as the system entry that begins the
/// new window, from where the thread's directory now says the window begins; the segments wholly before it expiring.
fn compacted(
    run: &mut Run<'_>,
    id: &str,
    key: &str,
    cut: &Cut,
    recap: String,
    response: Box<octoii_ai::vocabulary::Response>,
) -> anyhow::Result<Ending<CompactState, ()>> {
    let Some(answer) = run.answer::<Gather>(key)? else {
        let state = CompactState::Gathering {
            key: key.to_owned(),
            cut: cut.clone(),
            recap,
            response,
        };
        return Ok(Ending::Wait(state, Wait::new().mail(key.to_owned())));
    };
    run.take_mail(key);
    let told: Told = match answer {
        Answer::Given(told) => told,
        Answer::Interrupted => return Ok(Ending::Fail("gathering what the model is told was cut off".into())),
        Answer::Failed(words) => return Ok(Ending::Fail(words)),
    };
    let first = cut.kept.as_deref().unwrap_or(&cut.from);
    let begins = log::cursor_of(run.snapshot(), id, window::from(run.snapshot(), id)?.as_ref(), first)?;
    let Some(begins) = begins else {
        return Ok(Ending::Fail("the entry the window keeps first is gone".into()));
    };
    let mut writer = Writer::open(run.snapshot(), id)?;
    let compaction = Entry::Compaction {
        id: run.new_id()?,
        request: recap,
        kept: cut.kept.clone(),
        from: cut.from.clone(),
        effort: config_of(run, id)?.effort,
        response: *response,
    };
    writer.append(&mut run.changes, &compaction)?;
    let system = Entry::System {
        id: run.new_id()?,
        sections: told
            .sections
            .into_iter()
            .map(|(name, text)| siphon_threads::Section { name, text: Some(text) })
            .collect(),
        tools: told.tools,
    };
    writer.append(&mut run.changes, &system)?;
    let annotation = (
        Bytes::from_static(FROM_ANNOTATION.as_bytes()),
        Some(Bytes::from(serde_json::to_vec(&begins)?)),
    );
    run.changes.meta(thread::path(id), Vec::new(), vec![annotation]);
    expire_before(run, id, &begins.segment)?;
    Ok(Ending::Done(()))
}

/// Has the segments of the thread `id`'s log wholly before `segment` expire after the history's retention, from when
/// each was last written to — as the next began — where the person keeps history for a while and not forever.
fn expire_before(run: &mut Run<'_>, id: &str, segment: &str) -> anyhow::Result<()> {
    let Some(days) = siphon_settings::environment(run.snapshot())?.history_retention_days else {
        return Ok(());
    };
    let segments = log::segments(run.snapshot(), id)?;
    let Some(end) = segments.iter().position(|(name, _)| name == segment) else {
        return Ok(());
    };
    for pair in segments[..=end].windows(2) {
        let Some(last_written) = log::time_of(&pair[1].0) else { continue };
        let at = last_written + i64::from(days) * 86_400_000;
        run.changes.expire(log::segment(id, &pair[0].0), Some(at));
    }
    Ok(())
}
