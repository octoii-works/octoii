//! A thread's turn, a process under its main process: it asks its model by a `siphon.model` request, appends the reply
//! once, whole, takes on the calls the reply made — each weighed, asked and exec'd as its own, a call that reads at
//! once, one that may act after those before it — appending each result as it and every call before it ended, in the
//! order the model made them; then takes in what waits in the thread's inbox, so what the person sent while the calls
//! ran steers the turn at its next step — what waits for the next turn left for it — and asks again, until a reply calls nothing. It asks again after a failure
//! that may pass, and has its window compacted before a request it is too full for, or once after its provider refused
//! one as too long. Asked to stop, it keeps what had come of the reply and what its calls gave as they were cut off, and
//! ends every call it never ran as stopped, so every call the log holds has its result.

use std::sync::Arc;

use octoii_ai::models::Models;

use crate::agent::primitives::ThreadPrimitives;
use crate::machine::paths::Base;
use crate::threads::log::Writer;
use crate::threads::{CallKey, CallStatus, Config, Entry, SIPHON, ThreadUsage, Window, inbox, thread};
use crate::tools::projects::Place;
use crate::tools::{Calls, Setting, State, Tools};
use octoii_ai::vocabulary::{
    Failure, FailureKind, FunctionCallOutput, Incomplete, InputContent, Item, ItemStatus, Response, ResponseStatus,
};
use serde::{Deserialize, Serialize};
use siphon_runtime::{Answer, Ending, Program, Retries, Run, Wait};

use crate::agent::compaction;
use crate::agent::model::ModelRequest;

/// How many times a turn asks again after its model failed in a way that may pass, before the turn fails.
pub const RETRIES_MAX: u32 = 10;
/// How long a turn waits before asking again where the provider says nothing: doubling from the first, to the most.
const RETRY_FIRST_MS: i64 = 2_000;
const RETRY_MOST_MS: i64 = 60_000;
/// The longest a turn waits where the provider says how long.
const RETRY_AFTER_MOST_MS: i64 = 15 * 60_000;

/// What a turn is started with: its thread, and the names of the MCP servers' tools its model was told of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnInput {
    pub thread: String,
    pub catalog: Vec<String>,
}

/// Where a turn stands: about to ask, its `attempt` counting the failures that may pass, `compacted` where its window
/// was compacted for this request; asked, by the request of `key`; waiting for the thread's compaction to end; or
/// taking on the calls of a reply, those before `appended` with their results in the log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum TurnState {
    Asking {
        attempt: u32,
        #[serde(default)]
        compacted: bool,
    },
    Asked {
        key: String,
        attempt: u32,
        #[serde(default)]
        compacted: bool,
    },
    Compacting {
        attempt: u32,
    },
    Calling {
        calls: Calls,
        appended: usize,
    },
}

impl TurnState {
    /// About to ask for the first time, or after its calls.
    const FRESH: Self = Self::Asking {
        attempt: 0,
        compacted: false,
    };
}

/// The program of a thread's turns: its calls taken on with `tools`, their paths starting from where the `machine`
/// has the thread work; how full a window may grow told by the `models`' facts.
#[derive(Debug, Clone)]
pub struct TurnProgram {
    pub tools: Tools,
    pub machine: crate::machine::Host,
    pub models: Arc<Models>,
}

/// How a turn ended, as its process completes: the failure of its model that ended it, as the provider told it; none
/// where it replied. A fault of the turn's own fails its process.
pub type Ended = Option<Failure>;

impl Program for TurnProgram {
    const KIND: &'static str = crate::threads::thread::TURN_KIND;
    const STOP_HANDLER: bool = true;
    const RETRIES: Retries = Retries {
        count: 3,
        delay_min_ms: 1_000,
        delay_max_ms: 10_000,
    };
    type Input = TurnInput;
    type Checkpoint = TurnState;
    type Output = Ended;

    fn run(&self, run: &mut Run<'_>, input: TurnInput, state: Option<TurnState>) -> anyhow::Result<Ending<TurnState, Ended>> {
        let state = state.unwrap_or(TurnState::FRESH);
        if run.is_stop_handler() {
            return self.stop(run, &input, state);
        }
        match state {
            TurnState::Asking { attempt, compacted } => self.ask(run, &input.thread, attempt, compacted),
            TurnState::Asked { key, attempt, compacted } => self.answered(run, &input, &key, attempt, compacted),
            TurnState::Compacting { attempt } => compaction_ended(run, &input.thread, attempt),
            TurnState::Calling { calls, appended } => self.calling(run, &input, calls, appended),
        }
    }
}

/// Has the thread's window compacted — its compaction started where it is not running — and sleeps on it: not the
/// turn's own process, it is waited on by its record's changes.
fn compact(run: &mut Run<'_>, id: &str, attempt: u32) -> anyhow::Result<Ending<TurnState, Ended>> {
    let mut started = siphon_runtime::Changes::new();
    compaction::start(run.snapshot(), &mut started, id)?;
    run.changes.extend(started);
    Ok(Ending::Wait(
        TurnState::Compacting { attempt },
        Wait::new().change(compaction::path(id)),
    ))
}

/// The thread's compaction ended: the model asked on the window it began; the turn fails as it failed.
fn compaction_ended(run: &Run<'_>, id: &str, attempt: u32) -> anyhow::Result<Ending<TurnState, Ended>> {
    let record = run.snapshot().record(&compaction::path(id))?;
    match record.and_then(|record| record.outcome) {
        None => Ok(Ending::Wait(
            TurnState::Compacting { attempt },
            Wait::new().change(compaction::path(id)),
        )),
        Some(beak::Outcome::Completed(_)) => Ok(Ending::Next(TurnState::Asking { attempt, compacted: true })),
        Some(beak::Outcome::Failed { error, .. }) => Ok(Ending::Fail(format!(
            "its window could not be compacted: {}",
            String::from_utf8_lossy(&error.unwrap_or_default())
        ))),
        Some(beak::Outcome::Stopped { .. }) => Ok(Ending::Fail("its window's compaction was stopped".into())),
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

impl TurnProgram {
    /// Asks the thread's model for what follows its window, and sleeps till the answer comes; has the window compacted
    /// first where it is too full for the request, once for it.
    fn ask(&self, run: &mut Run<'_>, id: &str, attempt: u32, compacted: bool) -> anyhow::Result<Ending<TurnState, Ended>> {
        let config = config_of(run, id)?;
        let Some(model) = config.model else {
            return Ok(Ending::Fail("the thread names no model".into()));
        };
        let window = Window::read(run.snapshot(), id)?;
        let context = self.models.model(&model).and_then(|served| served.context_length);
        if !compacted && compaction::is_full(&window, context) && compaction::cut(&window).is_some() {
            return compact(run, id, attempt);
        }
        let Some(until) = window.last() else {
            return Ok(Ending::Fail("the thread's window is empty".into()));
        };
        let key = run.new_id()?;
        let request = ModelRequest {
            thread: id.to_owned(),
            until: until.to_owned(),
            model,
            instruction: None,
        };
        run.request(&key, &request)?;
        let wait = Wait::new().mail(key.clone());
        Ok(Ending::Wait(TurnState::Asked { key, attempt, compacted }, wait))
    }

    /// The model's answer to the request of `key`: its reply appended and the calls it made taken on; asked again
    /// where it was cut off, or failed in a way that may pass.
    fn answered(
        &self,
        run: &mut Run<'_>,
        input: &TurnInput,
        key: &str,
        attempt: u32,
        compacted: bool,
    ) -> anyhow::Result<Ending<TurnState, Ended>> {
        let Some(answer) = run.answer::<ModelRequest>(key)? else {
            let wait = Wait::new().mail(key.to_owned());
            let state = TurnState::Asked {
                key: key.to_owned(),
                attempt,
                compacted,
            };
            return Ok(Ending::Wait(state, wait));
        };
        run.take_mail(key);
        let response = match answer {
            Answer::Given(response) => response,
            Answer::Interrupted => return Ok(Ending::Next(TurnState::Asking { attempt, compacted })),
            Answer::Failed(words) => return Ok(Ending::Fail(words)),
        };
        charge(run, &response);
        let id = &input.thread;
        let mut writer = Writer::open(run.snapshot(), id)?;
        if response.status == ResponseStatus::Failed {
            keep_used_up(run, &response)?;
            let entry = Entry::Discarded {
                id: run.new_id()?,
                request: key.to_owned(),
                response: response.clone(),
            };
            writer.append(&mut run.changes, &entry)?;
            let too_long = response
                .failure
                .as_ref()
                .is_some_and(|failure| failure.kind == FailureKind::ContextOverflow);
            if too_long && !compacted && compaction::cut(&Window::read(run.snapshot(), id)?).is_some() {
                return compact(run, id, attempt);
            }
            return Ok(failed(run, &response, attempt));
        }
        let keys = keyed(run, &response)?;
        let (made, cut) = calls_of(&response, &keys);
        let at_limit = response.incomplete == Some(Incomplete::MaxOutputTokens);
        let reply = Entry::Reply {
            id: run.new_id()?,
            request: key.to_owned(),
            response,
            calls: keys,
            cancelled: false,
        };
        writer.append(&mut run.changes, &reply)?;
        for call in &cut {
            append_unrun(
                run,
                &mut writer,
                call,
                "its arguments were cut off as it was written; it was not run",
            )?;
        }
        if made.is_empty() {
            // A reply its limit cut off is told to go on, once: where the input before it told so already, it ends.
            if at_limit && !told_to_go_on(&Window::read(run.snapshot(), id)?) {
                let entry = Entry::Input {
                    id: run.new_id()?,
                    actor: SIPHON.to_owned(),
                    delivery: run.new_id()?,
                    content: vec![InputContent::text(GO_ON)],
                    reply_to: None,
                };
                writer.append(&mut run.changes, &entry)?;
                return Ok(Ending::Next(TurnState::FRESH));
            }
            return Ok(Ending::Done(None));
        }
        self.calling(run, input, Calls::keyed(made), 0)
    }

    /// The calls of a reply taken on as far as they go, each result appended as it and every call before it ended;
    /// once every call ended, what waits in the inbox taken in, and the model asked again.
    fn calling(&self, run: &mut Run<'_>, input: &TurnInput, mut calls: Calls, appended: usize) -> anyhow::Result<Ending<TurnState, Ended>> {
        let config = config_of(run, &input.thread)?;
        let base = self.base(&config);
        let project = Place::of(&base.workspace).project.to_string_lossy().into_owned();
        let places = self.machine.places_of(&input.thread);
        let setting = Setting {
            primitives: &ThreadPrimitives,
            thread: &input.thread,
            tools: &self.tools,
            catalog: &input.catalog,
            mode: config.mode.unwrap_or_default(),
            base: &base,
            project: Some(&project),
            places: &places,
            confines: beak::machine_confines(),
        };
        let wait = calls.advance(run, &setting)?;
        let mut writer = Writer::open(run.snapshot(), &input.thread)?;
        let appended = append_results(run, &mut writer, &input.thread, &calls, appended)?;
        if let Some(wait) = wait {
            return Ok(Ending::Wait(TurnState::Calling { calls, appended }, wait));
        }
        inbox::take_steering(run, &input.thread, &mut writer)?;
        Ok(Ending::Next(TurnState::FRESH))
    }

    /// Where the thread's paths start: the folder the machine has it work in.
    fn base(&self, config: &Config) -> Base {
        Base {
            workspace: self.machine.workspace_of(config.workspace.as_deref()),
            home: self.machine.home.clone(),
        }
    }

    /// The turn's stop handler: what had come of the reply kept, and every call ended — those that ran as their cut
    /// gave, the rest never run — each with its result; then the turn ends stopped.
    fn stop(&self, run: &mut Run<'_>, input: &TurnInput, state: TurnState) -> anyhow::Result<Ending<TurnState, Ended>> {
        let mut writer = Writer::open(run.snapshot(), &input.thread)?;
        match state {
            TurnState::Asking { .. } | TurnState::Compacting { .. } => Ok(Ending::Stopped(None)),
            TurnState::Asked { key, attempt, compacted } => {
                let Some(answer) = run.answer::<ModelRequest>(&key)? else {
                    let wait = Wait::new().mail(key.clone());
                    return Ok(Ending::Wait(TurnState::Asked { key, attempt, compacted }, wait));
                };
                run.take_mail(&key);
                let Answer::Given(response) = answer else {
                    return Ok(Ending::Stopped(None));
                };
                charge(run, &response);
                let keys = keyed(run, &response)?;
                let entry = if response.status == ResponseStatus::Failed || response.output.is_empty() {
                    Entry::Discarded {
                        id: run.new_id()?,
                        request: key,
                        response,
                    }
                } else {
                    Entry::Reply {
                        id: run.new_id()?,
                        request: key,
                        response,
                        calls: keys.clone(),
                        cancelled: true,
                    }
                };
                let replied = matches!(entry, Entry::Reply { .. });
                writer.append(&mut run.changes, &entry)?;
                if replied {
                    for call in &keys {
                        append_unrun(run, &mut writer, call, "the turn was stopped before it ran")?;
                    }
                }
                Ok(Ending::Stopped(None))
            }
            TurnState::Calling { mut calls, appended } => {
                let config = config_of(run, &input.thread)?;
                let base = self.base(&config);
                let setting = Setting {
                    primitives: &ThreadPrimitives,
                    thread: &input.thread,
                    tools: &self.tools,
                    catalog: &input.catalog,
                    mode: config.mode.unwrap_or_default(),
                    base: &base,
                    project: None,
                    places: &[],
                    confines: beak::machine_confines(),
                };
                let wait = calls.stop(run, &setting)?;
                let appended = append_results(run, &mut writer, &input.thread, &calls, appended)?;
                match wait {
                    Some(wait) => Ok(Ending::Wait(TurnState::Calling { calls, appended }, wait)),
                    None => Ok(Ending::Stopped(None)),
                }
            }
        }
    }
}

/// Each call `response` made — cut off as it was written or not — keyed by an id made now, in the order the model made
/// them.
fn keyed(run: &Run<'_>, response: &Response) -> beak::Result<Vec<CallKey>> {
    let mut keys = Vec::new();
    for item in &response.output {
        let Item::FunctionCall(call) = item else { continue };
        keys.push(CallKey {
            call_id: call.call_id.clone(),
            key: run.new_id()?,
        });
    }
    Ok(keys)
}

/// The calls `response` made, each as `keys` keys it, with its function and its arguments; and those cut off as they
/// were written, which are never run.
fn calls_of(response: &Response, keys: &[CallKey]) -> (Vec<(CallKey, String, String)>, Vec<CallKey>) {
    let mut made = Vec::new();
    let mut cut = Vec::new();
    let calls = response.output.iter().filter_map(|item| match item {
        Item::FunctionCall(call) => Some(call),
        _ => None,
    });
    for (call, key) in calls.zip(keys) {
        if call.status == Some(ItemStatus::Incomplete) {
            cut.push(key.clone());
        } else {
            made.push((key.clone(), call.name.clone(), call.arguments.clone()));
        }
    }
    (made, cut)
}

/// What a reply its output limit cut off is told, as a system reminder.
pub const GO_ON: &str = "<system-reminder>your reply was cut off at the output limit: continue from where it stopped</system-reminder>";

/// Whether the last input of `window`'s told a reply cut off at its limit to go on.
fn told_to_go_on(window: &Window) -> bool {
    window
        .conversation
        .iter()
        .rev()
        .find_map(|entry| match entry {
            Entry::Input { actor, content, .. } => Some(actor == SIPHON && content == &[InputContent::text(GO_ON)]),
            _ => None,
        })
        .unwrap_or(false)
}

/// Appends with the run's commit the result of each call from `appended` on that ended, while every call before it
/// did: where the next result goes. The instructions a call's tool came upon go after its output where the thread's
/// window does not hold them yet, so the model is given each once a window.
fn append_results(run: &mut Run<'_>, writer: &mut Writer, thread: &str, calls: &Calls, appended: usize) -> anyhow::Result<usize> {
    let mut at = appended;
    let mut held: Option<String> = None;
    while let Some(call) = calls.0.get(at) {
        let State::Ended {
            status,
            output,
            structured,
            instructions,
        } = &call.state
        else {
            break;
        };
        let mut output = output.clone();
        if !instructions.is_empty() {
            let held = match &mut held {
                Some(held) => held,
                None => held.insert(window_text(&Window::read(run.snapshot(), thread)?)),
            };
            for each in instructions {
                if !held.contains(&each.opening()) {
                    output.push(InputContent::text(each.told()));
                    held.push_str(&each.opening());
                }
            }
        }
        let entry = Entry::Result {
            id: run.new_id()?,
            call: call.key.clone(),
            status: *status,
            output: FunctionCallOutput {
                call_id: call.id.clone(),
                output,
                is_error: *status != CallStatus::Ok,
            },
            structured: structured.clone(),
        };
        writer.append(&mut run.changes, &entry)?;
        at += 1;
    }
    Ok(at)
}

/// The text of what `window` gave the model of its own: its sections and its calls' outputs.
fn window_text(window: &Window) -> String {
    let mut text = String::new();
    for entry in window.told.iter().chain(&window.conversation) {
        match entry {
            Entry::System { sections, .. } => {
                for section in sections {
                    text.push_str(section.text.as_deref().unwrap_or_default());
                }
            }
            Entry::Result { output, .. } => {
                for part in &output.output {
                    if let InputContent::InputText { text: words } = part {
                        text.push_str(words);
                    }
                }
            }
            _ => {}
        }
    }
    text
}

/// Appends the result of the call `call`, which was never run, saying why.
fn append_unrun(run: &mut Run<'_>, writer: &mut Writer, call: &CallKey, why: &str) -> anyhow::Result<()> {
    let entry = Entry::Result {
        id: run.new_id()?,
        call: call.key.clone(),
        status: CallStatus::Interrupted,
        output: FunctionCallOutput {
            call_id: call.call_id.clone(),
            output: vec![InputContent::text(why)],
            is_error: true,
        },
        structured: None,
    };
    writer.append(&mut run.changes, &entry)?;
    Ok(())
}

/// Charges what `response` says its request used to the process of `run` and every group above it, as a cgroup
/// charges its ancestors.
pub(crate) fn charge(run: &mut Run<'_>, response: &Response) {
    let Some(used) = &response.usage else { return };
    let counters = ThreadUsage::counters_of(used);
    if !counters.is_empty() {
        let process = bytes::Bytes::copy_from_slice(run.path());
        run.changes.charge(process, counters);
    }
}

/// Keeps the quota of the account of `response`'s model used up, where its failure says it is till a time to come.
fn keep_used_up(run: &mut Run<'_>, response: &Response) -> anyhow::Result<()> {
    let Some(failure) = &response.failure else { return Ok(()) };
    let Some(reset_at_ms) = failure.reset_at_ms.filter(|at| *at > run.time_ms()) else {
        return Ok(());
    };
    if failure.kind != octoii_ai::vocabulary::FailureKind::Quota {
        return Ok(());
    }
    let used_up = crate::accounts::UsedUp {
        reset_at_ms,
        window_minutes: failure.window_minutes,
    };
    let mut kept = siphon_runtime::Changes::new();
    crate::accounts::keep_used_up(run.snapshot(), &mut kept, crate::accounts::provider_of(&response.model), used_up)?;
    run.changes.extend(kept);
    Ok(())
}

/// After the model failed with `response`: asked again once the wait passes, where the failure may pass and retries
/// are left; else the turn ends with the failure, as the provider told it.
fn failed(run: &Run<'_>, response: &Response, attempt: u32) -> Ending<TurnState, Ended> {
    let Some(failure) = &response.failure else {
        return Ending::Done(Some(Failure::new(FailureKind::InvalidResponse, "the model failed, saying nothing")));
    };
    if !failure.retryable() || attempt + 1 > RETRIES_MAX {
        return Ending::Done(Some(failure.clone()));
    }
    let wait = match failure.retry_after_ms {
        Some(after) => i64::try_from(after).unwrap_or(i64::MAX).min(RETRY_AFTER_MOST_MS),
        None => (RETRY_FIRST_MS << attempt.min(16)).min(RETRY_MOST_MS),
    };
    let state = TurnState::Asking {
        attempt: attempt + 1,
        compacted: false,
    };
    Ending::Wait(state, Wait::new().deadline(run.time_ms() + wait))
}
