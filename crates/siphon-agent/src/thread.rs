//! A thread's main process. At rest it takes what waits in the inbox into the log — a change of how the thread runs
//! where it stands — and where an input waits, has what the model is told gathered, then begins a turn with it in one
//! commit: what of it the model was not told appended as a system entry, the person asked to trust what its project
//! names where they decided nothing on it, the inputs taken in, the thread's title asked of its model where it has
//! none, and the turn started — bounded by what the thread's budget leaves — unless the budget is spent or its model's
//! account is used up. Once the turn ended, it settles it, as
//! a parent reaps a child: every call it left without a result answered, its `turn-end` appended, and how and when it
//! ended set; so a turn killed, or one that failed, is settled all the same.

use beak::{Outcome, layout};
use bytes::Bytes;
use octoii_ai::vocabulary::{FunctionCallOutput, InputContent, Item};
use serde::{Deserialize, Serialize};
use siphon_runtime::{Answer, Ending, Program, Retries, Run, Wait};
use siphon_threads::inbox::{self, Ended, Inbound};
use siphon_threads::log::Writer;
use siphon_threads::{CallStatus, Config, Entry, MainInput, Outcome as TurnOutcome, Turn, Usage, Window, thread};
use siphon_tools::mcp::TrustProgram;

use crate::gather::Gather;
use crate::title::{TitleInput, TitleProgram};
use crate::told::{self, Told};
use crate::turn::{TurnInput, TurnProgram};

/// The annotation on a turn's process that bounds what its requests may cost, as a cgroup's `*.max` bounds its group.
fn cost_limit() -> Bytes {
    [layout::MAX_ANNOTATION_PREFIX, b"cost-nanodollars"].concat().into()
}

/// Where a thread's main process stands: at rest; gathering what its model is told, by the request of `key`; or its
/// turn running, the process keyed `turn`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum MainState {
    Rest,
    Gathering { key: String },
    Running { turn: String },
}

/// The program of a thread's main process.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadProgram;

impl Program for ThreadProgram {
    const KIND: &'static str = "siphon.thread";
    const RETRIES: Retries = Retries {
        count: 3,
        delay_min_ms: 1_000,
        delay_max_ms: 10_000,
    };
    type Input = MainInput;
    type Checkpoint = MainState;
    type Output = ();

    fn run(&self, run: &mut Run<'_>, input: MainInput, state: Option<MainState>) -> anyhow::Result<Ending<MainState, ()>> {
        let id = &input.thread;
        match state.unwrap_or(MainState::Rest) {
            MainState::Rest => rest(run, id),
            MainState::Gathering { key } => gathered(run, id, &key),
            MainState::Running { turn } => running(run, id, &turn),
        }
    }
}

/// At rest: a turn a main process before this one left running settled; what waits in the inbox taken in, and where
/// an input waits, what the model is told gathered; else asleep on the inbox.
fn rest(run: &mut Run<'_>, id: &str) -> anyhow::Result<Ending<MainState, ()>> {
    if turn_of(run, id)? == Turn::Running {
        settle(
            run,
            id,
            TurnOutcome::Failed,
            Some("its thread's process ended while it ran".into()),
            None,
        )?;
        return Ok(Ending::Next(MainState::Rest));
    }
    let waiting = inbox::waiting(run.snapshot(), id)?;
    if waiting.iter().any(|each| matches!(each.inbound, Inbound::Input { .. })) {
        let key = run.new_id()?;
        run.request(&key, &Gather { thread: id.to_owned() })?;
        return Ok(Ending::Wait(MainState::Gathering { key: key.clone() }, Wait::new().mail(key)));
    }
    if !waiting.is_empty() {
        let mut writer = Writer::open(run.snapshot(), id)?;
        inbox::take(run, id, &mut writer)?;
    }
    Ok(Ending::Wait(MainState::Rest, Wait::new().change(inbox::path(id))))
}

fn turn_of(run: &Run<'_>, id: &str) -> anyhow::Result<Turn> {
    let directory = run
        .snapshot()
        .stat(&thread::path(id))?
        .and_then(|entry| entry.directory)
        .unwrap_or_default();
    Ok(directory
        .annotation(siphon_threads::TURN_ANNOTATION.as_bytes())
        .map(|value| serde_json::from_slice(value))
        .transpose()?
        .unwrap_or_default())
}

/// What the model is told came, by the request of `key`: a turn begun with it; where it could not be gathered, the
/// inputs that wait ended as a turn that failed before its model was asked.
fn gathered(run: &mut Run<'_>, id: &str, key: &str) -> anyhow::Result<Ending<MainState, ()>> {
    let Some(answer) = run.answer::<Gather>(key)? else {
        return Ok(Ending::Wait(
            MainState::Gathering { key: key.to_owned() },
            Wait::new().mail(key.to_owned()),
        ));
    };
    run.take_mail(key);
    match answer {
        Answer::Given(told) => begin(run, id, told),
        Answer::Interrupted => Ok(Ending::Next(MainState::Rest)),
        Answer::Failed(why) => {
            let reason = format!("what the model is told could not be gathered: {why}");
            fail_before(run, id, &reason)
        }
    }
}

/// Begins a turn with `told`, in one commit, as the module says.
fn begin(run: &mut Run<'_>, id: &str, told: Told) -> anyhow::Result<Ending<MainState, ()>> {
    let mut writer = Writer::open(run.snapshot(), id)?;
    let window = Window::read(run.snapshot(), id)?;
    let sections = told::news(&told::held(&window), &told.sections);
    let tools = if window.begins() { told.tools } else { Vec::new() };
    if !sections.is_empty() || !tools.is_empty() {
        let entry = Entry::System {
            id: run.new_id()?,
            sections,
            tools,
        };
        writer.append(&mut run.changes, &entry)?;
    }
    if let Some(trust) = told.asks {
        let asking = layout::process(run.path(), TrustProgram::KEY.as_bytes());
        if run.snapshot().record(&asking)?.is_none() {
            run.start::<TrustProgram>(TrustProgram::KEY, &trust)?;
        }
    }
    let taken = inbox::take(run, id, &mut writer)?;
    if !taken.iter().any(|each| matches!(each.entry, Entry::Input { .. })) {
        return Ok(Ending::Next(MainState::Rest));
    }
    let taken: Vec<Entry> = taken.into_iter().map(|each| each.entry).collect();
    let config = Config::of(
        &run.snapshot()
            .stat(&thread::path(id))?
            .and_then(|entry| entry.directory)
            .unwrap_or_default(),
    )?;
    let left = budget_left(run, id, &config)?;
    if left == Some(0) {
        return end_before(run, id, &mut writer, &taken, "its thread's budget is spent");
    }
    if let Some(model) = &config.model
        && let Some(used_up) = siphon_accounts::used_up(run.snapshot(), siphon_accounts::provider_of(model))?
    {
        let reason = format!("its model's account is used up till {} ms since the epoch", used_up.reset_at_ms);
        return end_before(run, id, &mut writer, &taken, &reason);
    }
    if config.title.is_none() && config.model.is_some() {
        let titling = layout::process(run.path(), TitleProgram::KEY.as_bytes());
        if run.snapshot().record(&titling)?.is_none() {
            run.start::<TitleProgram>(TitleProgram::KEY, &TitleInput { thread: id.to_owned() })?;
        }
    }
    let turn = run.new_id()?;
    run.start::<TurnProgram>(
        &turn,
        &TurnInput {
            thread: id.to_owned(),
            catalog: told.catalog,
        },
    )?;
    if let Some(left) = left {
        let process = layout::process(run.path(), turn.as_bytes());
        run.changes
            .meta(process, Vec::new(), vec![(cost_limit(), Some(Bytes::from(left.to_string())))]);
    }
    let now = run.time_ms();
    siphon_threads::began(&mut run.changes, id, now);
    Ok(Ending::Wait(MainState::Running { turn: turn.clone() }, Wait::new().child(turn)))
}

/// What the thread's budget leaves its next turn, in billionths of a dollar; none for a thread with no budget.
fn budget_left(run: &Run<'_>, id: &str, config: &Config) -> anyhow::Result<Option<u64>> {
    let Some(budget) = config.budget_nanodollars else { return Ok(None) };
    let used = run
        .snapshot()
        .record(&thread::main(id))?
        .and_then(|record| Usage::of(&record.usage))
        .map_or(0, |usage| usage.cost_nanodollars);
    Ok(Some(budget.saturating_sub(used)))
}

/// Ends the inputs that wait as a turn that failed before its model was asked, for `reason`.
fn fail_before(run: &mut Run<'_>, id: &str, reason: &str) -> anyhow::Result<Ending<MainState, ()>> {
    let mut writer = Writer::open(run.snapshot(), id)?;
    let taken: Vec<Entry> = inbox::take(run, id, &mut writer)?.into_iter().map(|each| each.entry).collect();
    if !taken.iter().any(|entry| matches!(entry, Entry::Input { .. })) {
        return Ok(Ending::Next(MainState::Rest));
    }
    end_before(run, id, &mut writer, &taken, reason)
}

/// Ends the inputs just taken, `taken`, as a turn that failed before its model was asked, for `reason`: its end
/// appended with the same commit, and who waits on them told.
fn end_before(run: &mut Run<'_>, id: &str, writer: &mut Writer, taken: &[Entry], reason: &str) -> anyhow::Result<Ending<MainState, ()>> {
    let entry = Entry::TurnEnd {
        id: run.new_id()?,
        turn: None,
        outcome: TurnOutcome::Failed,
        reason: Some(reason.to_owned()),
    };
    writer.append(&mut run.changes, &entry)?;
    let now = run.time_ms();
    siphon_threads::ended(&mut run.changes, id, Turn::Failed, now);
    let ended = Ended::Failed { reason: reason.to_owned() };
    tell(run, taken, &ended)?;
    Ok(Ending::Next(MainState::Rest))
}

/// Tells who waits on each input of `entries` how the turn that took it ended, as `ended` says.
fn tell(run: &mut Run<'_>, entries: &[Entry], ended: &Ended) -> anyhow::Result<()> {
    let mut told = siphon_runtime::Changes::new();
    for entry in entries {
        if let Entry::Input {
            reply_to: Some(reply_to), ..
        } = entry
        {
            inbox::tell(run.snapshot(), &mut told, reply_to, ended)?;
        }
    }
    run.changes.extend(told);
    Ok(())
}

/// The turn keyed `turn` running: settled once it ended, and reaped.
fn running(run: &mut Run<'_>, id: &str, turn: &str) -> anyhow::Result<Ending<MainState, ()>> {
    let process = layout::process(run.path(), turn.as_bytes());
    let outcome = run.snapshot().record(&process)?.and_then(|record| record.outcome);
    let Some(outcome) = outcome else {
        return Ok(Ending::Wait(
            MainState::Running { turn: turn.to_owned() },
            Wait::new().child(turn.to_owned()),
        ));
    };
    let (ended, reason) = match outcome {
        Outcome::Completed(_) => (TurnOutcome::Replied, None),
        Outcome::Failed { error, .. } => (TurnOutcome::Failed, Some(said(error.as_deref(), "it failed"))),
        Outcome::Stopped { reason, .. } if reason.as_deref() == Some(layout::LIMIT_REASON) => {
            (TurnOutcome::Stopped, Some("its thread's budget ran out".to_owned()))
        }
        Outcome::Stopped { reason, .. } => (TurnOutcome::Stopped, reason.map(|reason| said(Some(&reason), "stopped"))),
    };
    settle(run, id, ended, reason, Some(turn))?;
    run.changes.reap(process);
    Ok(Ending::Next(MainState::Rest))
}

/// What `bytes`, an error or a reason Beak keeps, says: a fault's message, or the text as it is; `otherwise` for none.
fn said(bytes: Option<&[u8]>, otherwise: &str) -> String {
    let Some(bytes) = bytes.filter(|bytes| !bytes.is_empty()) else {
        return otherwise.to_owned();
    };
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|value| value["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned())
}

/// Settles the turn `turn` — none for one whose process is gone — as it ended: every call the replies since the last
/// turn's end made that has no result answered interrupted, its end appended, how and when it ended set, and who
/// waits on its inputs told.
fn settle(run: &mut Run<'_>, id: &str, ended: TurnOutcome, reason: Option<String>, turn: Option<&str>) -> anyhow::Result<()> {
    let window = Window::read(run.snapshot(), id)?;
    let since = window
        .conversation
        .iter()
        .rposition(|entry| matches!(entry, Entry::TurnEnd { .. }))
        .map_or(0, |at| at + 1);
    let mut made = Vec::new();
    let mut answered = Vec::new();
    let mut said = String::new();
    for entry in &window.conversation[since..] {
        match entry {
            Entry::Reply { response, .. } => {
                made.extend(response.output.iter().filter_map(|item| match item {
                    Item::FunctionCall(call) => Some(call.call_id.clone()),
                    _ => None,
                }));
                let words = response.words();
                if !words.is_empty() {
                    said = words;
                }
            }
            Entry::Result { output, .. } => answered.push(output.call_id.clone()),
            _ => {}
        }
    }
    let mut writer = Writer::open(run.snapshot(), id)?;
    for call_id in made.into_iter().filter(|call_id| !answered.contains(call_id)) {
        let entry = Entry::Result {
            id: run.new_id()?,
            call: String::new(),
            status: CallStatus::Interrupted,
            output: FunctionCallOutput {
                call_id,
                output: vec![InputContent::text("cut off as its turn ended; it may or may not have acted")],
                is_error: true,
            },
        };
        writer.append(&mut run.changes, &entry)?;
    }
    let told = match ended {
        TurnOutcome::Replied => Ended::Replied { said },
        TurnOutcome::Failed => Ended::Failed {
            reason: reason.clone().unwrap_or_default(),
        },
        TurnOutcome::Stopped => Ended::Stopped,
    };
    tell(run, &window.conversation[since..], &told)?;
    let entry = Entry::TurnEnd {
        id: run.new_id()?,
        turn: turn.map(str::to_owned),
        outcome: ended,
        reason,
    };
    writer.append(&mut run.changes, &entry)?;
    let status = match ended {
        TurnOutcome::Replied => Turn::Replied,
        TurnOutcome::Failed => Turn::Failed,
        TurnOutcome::Stopped => Turn::Stopped,
    };
    let now = run.time_ms();
    siphon_threads::ended(&mut run.changes, id, status, now);
    Ok(())
}
