//! The calls a model made, as their caller — a turn, a script — takes them on in its phases, each a request of its
//! caller's: checked, weighed, the person or a second model asked where the weighing says, then exec'd. A call that
//! reads starts at once; one that may act starts once every call before it ended, and holds back every call after it
//! till it ended — as the model wrote them, one after another, what it acts on is in the state it left. What they come to
//! is kept in the caller's checkpoint, and each ends once, with what the model reads.

use crate::machine::paths::Base;
use crate::threads::{CallKey, CallStatus, Mode};
use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use siphon_runtime::{Answer, Run, Wait};

use crate::tools::effect::{Effect, Line};
use crate::tools::permissions::{Bounds, Grants, Verdict, Writes, patterns_of, weigh};
use crate::tools::plan::{Awaited, Primitives};
use crate::tools::projects;
use crate::tools::requests::{Asks, Decision, Exec, Judgment, Permission, Review, Shown};
use crate::tools::tool::Tools;

/// Where a call stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum State {
    Waiting,
    Asking,
    Judging,
    Running,
    /// Its tool's program runs, as a process under its caller keyed by the call's key.
    Program,
    /// It ends with how the turn that takes what it delivered ends, which the mail keyed by the delivery tells.
    Awaiting(Awaited),
    Ended {
        status: CallStatus,
        output: Vec<InputContent>,
        /// What it did, typed by its tool, where the tool told it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        structured: Option<serde_json::Value>,
        /// The instructions its tool came upon on its way, for the model where it was not given them yet.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        instructions: Vec<crate::tools::told::Instructions>,
    },
}

/// A call: the model's id of it, Siphon's key of it — which names its requests and its outputs — its tool, its
/// arguments as the model wrote them, whether it may act once that is known, and where it stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Call {
    pub id: String,
    pub key: String,
    pub tool: String,
    pub arguments: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acts: Option<bool>,
    pub state: State,
}

/// What the calls are weighed and exec'd by: the thread, its tools, what does what they plan for threads, the names of
/// the tools of <catalog> its model is told of, its mode, where its paths start, its project's folder where it has one,
/// the places beside its workspace it writes as freely, and whether the machine confines a command to a sandbox.
#[derive(Clone, Copy)]
pub struct Setting<'a> {
    pub thread: &'a str,
    pub tools: &'a Tools,
    pub primitives: &'a dyn Primitives,
    pub catalog: &'a [String],
    pub mode: Mode,
    pub base: &'a Base,
    pub project: Option<&'a str>,
    pub places: &'a [std::path::PathBuf],
    pub confines: bool,
}

impl std::fmt::Debug for Setting<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Setting")
            .field("thread", &self.thread)
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

/// How `call`, which waits on `awaited`, ended, as the mail of the turn that took its delivery tells — or the thread
/// deleted before it did; none while neither came.
fn awaited_end(run: &mut Run<'_>, setting: &Setting<'_>, call: &Call, awaited: &Awaited) -> anyhow::Result<Option<State>> {
    let ended_as = match crate::threads::inbox::ended_mail(run, &awaited.delivery)? {
        Some(ended) => Some(ended),
        None if run.snapshot().stat(&crate::threads::thread::path(&awaited.thread))?.is_none() => None,
        None => return Ok(None),
    };
    let told = setting.tools.awaited(
        &call.tool,
        &serde_json::from_str(&call.arguments)?,
        &awaited.thread,
        ended_as.as_ref(),
    );
    Ok(Some(State::Ended {
        status: if matches!(ended_as, Some(crate::threads::inbox::Ended::Replied { .. })) {
            CallStatus::Ok
        } else {
            CallStatus::ToolError
        },
        output: told.output,
        structured: told.structured,
        instructions: told.instructions,
    }))
}

/// Where a call whose exec told `told` stands, its plan done with the run's commit: ended with what it told, or waiting
/// on the turn that takes what it delivered; ended failed where its plan may not be done.
fn planned(run: &mut Run<'_>, setting: &Setting<'_>, call: &str, told: crate::tools::told::Told) -> anyhow::Result<State> {
    if !told.plan.is_empty()
        && let Err(refused) = setting.primitives.apply(run, setting.thread, call, &told.plan)?
    {
        return Ok(ended(CallStatus::ToolError, refused));
    }
    Ok(match told.plan.awaits {
        Some(awaited) => State::Awaiting(awaited),
        None => State::Ended {
            status: CallStatus::Ok,
            output: told.output,
            structured: told.structured,
            instructions: told.instructions,
        },
    })
}

/// A model's calls, as their caller takes them on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Calls(pub Vec<Call>);

fn ended(status: CallStatus, words: impl Into<String>) -> State {
    State::Ended {
        status,
        output: vec![InputContent::text(words)],
        structured: None,
        instructions: Vec::new(),
    }
}

fn shown(tool: &str, arguments: &str, effect: &Effect) -> Shown {
    let (does, justification) = match effect {
        Effect::Reads => (Vec::new(), None),
        Effect::Edits(paths) => (paths.iter().map(|path| format!("edits {}", path.display())).collect(), None),
        Effect::Runs { line, justification, .. } => {
            let commands = Line::read(line).map_or_else(|_| vec![line.clone()], |read| read.commands);
            (
                commands.into_iter().map(|command| format!("runs {command}")).collect(),
                justification.clone(),
            )
        }
        Effect::Acts { tool } => (vec![format!("acts through {tool}, which may do anything")], None),
        Effect::Browses => (vec!["works the browser".into()], None),
        Effect::Calls => (vec!["runs a script whose calls are each weighed".into()], None),
        Effect::Delegates => (vec!["hands a task to a sub-thread, whose calls are weighed in it".into()], None),
    };
    Shown {
        tool: tool.to_owned(),
        arguments: arguments.to_owned(),
        does,
        justification,
    }
}

impl Calls {
    /// The calls `made` — each the model's id of it, its tool and its arguments — each keyed by an id made now.
    ///
    /// # Errors
    /// Past the last id there is.
    pub fn new(run: &Run<'_>, made: Vec<(String, String, String)>) -> beak::Result<Self> {
        let mut keyed = Vec::with_capacity(made.len());
        for (call_id, tool, arguments) in made {
            keyed.push((
                CallKey {
                    call_id,
                    key: run.new_id()?,
                },
                tool,
                arguments,
            ));
        }
        Ok(Self::keyed(keyed))
    }

    /// The calls `made` — each keyed already, its tool and its arguments — as the reply that made them records their
    /// keys.
    #[must_use]
    pub fn keyed(made: Vec<(CallKey, String, String)>) -> Self {
        Self(
            made.into_iter()
                .map(|(CallKey { call_id, key }, tool, arguments)| Call {
                    id: call_id,
                    key,
                    tool,
                    arguments,
                    acts: None,
                    state: State::Waiting,
                })
                .collect(),
        )
    }

    /// Whether every call ended.
    #[must_use]
    pub fn ended(&self) -> bool {
        self.0.iter().all(|call| matches!(call.state, State::Ended { .. }))
    }

    /// Takes the calls on as far as they go with the run's commit: the answers come taken, the calls free to start
    /// started. What to wait for till they go on; none once every call ended.
    ///
    /// # Errors
    /// The volume's failure; what cannot be read or written as JSON.
    pub fn advance(&mut self, run: &mut Run<'_>, setting: &Setting<'_>) -> anyhow::Result<Option<Wait>> {
        for at in 0..self.0.len() {
            self.take_answer(at, run, setting)?;
        }
        self.start_free(run, setting)?;
        Ok(self.wait())
    }

    /// Takes the calls on as a stop has them end: the answers that come taken — a call cut off is answered so — and
    /// those not yet started ended, never run. What to wait for till those that run are answered; none once every
    /// call ended.
    ///
    /// # Errors
    /// The volume's failure; what cannot be read or written as JSON.
    pub fn stop(&mut self, run: &mut Run<'_>, setting: &Setting<'_>) -> anyhow::Result<Option<Wait>> {
        for at in 0..self.0.len() {
            match &self.0[at].state {
                State::Waiting => self.0[at].state = ended(CallStatus::Interrupted, "the turn was stopped before it ran"),
                State::Awaiting(awaited) => {
                    setting.primitives.withdraw(run, awaited)?;
                    self.0[at].state = ended(CallStatus::Interrupted, "stopped, and the thread's work on it with it");
                }
                _ => {}
            }
            self.take_answer(at, run, setting)?;
        }
        Ok(self.wait())
    }

    /// What to wait for till the calls that run are answered; none where none runs.
    fn wait(&self) -> Option<Wait> {
        let mut wait = Wait::new();
        let mut waits = false;
        for call in &self.0 {
            let key = match call.state {
                State::Asking => format!("{}.permission", call.key),
                State::Judging => format!("{}.review", call.key),
                State::Running => call.key.clone(),
                State::Program => {
                    wait = wait.child(call.key.clone());
                    waits = true;
                    continue;
                }
                State::Awaiting(ref awaited) => {
                    wait = wait
                        .mail(awaited.delivery.clone())
                        .change(crate::threads::thread::path(&awaited.thread));
                    waits = true;
                    continue;
                }
                State::Waiting | State::Ended { .. } => continue,
            };
            wait = wait.mail(key);
            waits = true;
        }
        waits.then_some(wait)
    }

    /// Takes the answer the call at `at` waits for, where it came.
    fn take_answer(&mut self, at: usize, run: &mut Run<'_>, setting: &Setting<'_>) -> anyhow::Result<()> {
        let call = &self.0[at];
        let next = match call.state {
            State::Asking => {
                let key = format!("{}.permission", call.key);
                let Some(answer) = run.answer::<Permission>(&key)? else {
                    return Ok(());
                };
                run.take_mail(&key);
                match answer {
                    Answer::Given(Decision::Allow { always }) => {
                        if always && let Some(folder) = setting.project {
                            let effect = setting
                                .tools
                                .effect(&call.tool, &serde_json::from_str(&call.arguments)?, setting.base)?;
                            let granted = projects::grant(run.snapshot(), folder, patterns_of(&effect))?;
                            run.changes.extend(granted);
                        }
                        self.exec(at, run, setting, Writes::Anywhere)?
                    }
                    Answer::Given(Decision::Deny { reason }) => ended(
                        CallStatus::Rejected,
                        reason.map_or_else(
                            || "the person declined it".into(),
                            |reason| format!("the person declined it: {reason}"),
                        ),
                    ),
                    Answer::Failed(_) | Answer::Interrupted => {
                        ended(CallStatus::Interrupted, "the call was cut off before the person answered")
                    }
                }
            }
            State::Judging => {
                let key = format!("{}.review", call.key);
                let Some(answer) = run.answer::<Review>(&key)? else { return Ok(()) };
                run.take_mail(&key);
                if let Answer::Given(Judgment { usage: Some(used), .. }) = &answer {
                    let process = bytes::Bytes::copy_from_slice(run.path());
                    run.changes.charge(process, crate::threads::ThreadUsage::counters_of(used));
                }
                match answer {
                    Answer::Given(judgment) if judgment.allow => self.exec(at, run, setting, Writes::Anywhere)?,
                    Answer::Given(judgment) => ended(CallStatus::Rejected, format!("blocked: {}", judgment.reason)),
                    // A judgment that came to nothing leaves the call to the person.
                    Answer::Failed(_) => self.ask(at, run, setting)?,
                    Answer::Interrupted => ended(CallStatus::Interrupted, "the call was cut off before it was judged"),
                }
            }
            State::Running if setting.tools.asks(&call.tool, &serde_json::from_str(&call.arguments)?).is_some() => {
                let Some(answer) = run.answer::<Asks>(&call.key)? else {
                    return Ok(());
                };
                run.take_mail(&call.key);
                match answer {
                    Answer::Given(given) => State::Ended {
                        status: CallStatus::Ok,
                        output: setting.tools.answered(&call.tool, &serde_json::from_str(&call.arguments)?, given),
                        structured: None,
                        instructions: Vec::new(),
                    },
                    Answer::Failed(words) => ended(CallStatus::ToolError, words),
                    Answer::Interrupted => ended(CallStatus::Interrupted, "the call was cut off before the person answered"),
                }
            }
            State::Running => {
                let Some(answer) = run.answer::<Exec>(&call.key)? else {
                    return Ok(());
                };
                run.take_mail(&call.key);
                match answer {
                    Answer::Given(told) => planned(run, setting, &call.key, told)?,
                    Answer::Failed(words) => ended(CallStatus::ToolError, words),
                    Answer::Interrupted => ended(CallStatus::Interrupted, "the call was cut off"),
                }
            }
            State::Awaiting(ref awaited) => match awaited_end(run, setting, call, awaited)? {
                Some(state) => state,
                None => return Ok(()),
            },
            State::Program => {
                let process = beak::layout::process(run.path(), call.key.as_bytes());
                let Some(outcome) = run.snapshot().record(&process)?.and_then(|record| record.outcome) else {
                    return Ok(());
                };
                run.changes.reap(process);
                let told = setting.tools.finished(&call.tool, &outcome);
                State::Ended {
                    status: if matches!(outcome, beak::Outcome::Completed(_)) {
                        CallStatus::Ok
                    } else {
                        CallStatus::ToolError
                    },
                    output: told.output,
                    structured: told.structured,
                    instructions: told.instructions,
                }
            }
            State::Waiting | State::Ended { .. } => return Ok(()),
        };
        self.0[at].state = next;
        Ok(())
    }

    /// Starts each waiting call no call before it holds back.
    fn start_free(&mut self, run: &mut Run<'_>, setting: &Setting<'_>) -> anyhow::Result<()> {
        let (mut unended, mut acting) = (false, false);
        for at in 0..self.0.len() {
            if self.0[at].state == State::Waiting {
                let call = &self.0[at];
                let effect = serde_json::from_str(&call.arguments)
                    .map_err(|error| crate::tools::tool::invalid(&error, &call.arguments))
                    .and_then(|arguments| setting.tools.effect(&call.tool, &arguments, setting.base));
                match effect {
                    Err(refusal) => {
                        let status = if setting.tools.has(&call.tool) {
                            CallStatus::InvalidArguments
                        } else {
                            CallStatus::Unavailable
                        };
                        self.0[at].state = ended(status, refusal.0);
                    }
                    Ok(effect) => {
                        let acts = !matches!(effect, Effect::Reads | Effect::Delegates);
                        self.0[at].acts = Some(acts);
                        if !(acting || (acts && unended)) {
                            self.0[at].state = self.start(at, run, setting, &effect)?;
                        }
                    }
                }
            }
            if !matches!(self.0[at].state, State::Ended { .. }) {
                unended = true;
                acting |= self.0[at].acts.unwrap_or(true);
            }
        }
        Ok(())
    }

    /// Starts the call at `at`, whose effect is `effect`, as its weighing says.
    fn start(&self, at: usize, run: &mut Run<'_>, setting: &Setting<'_>, effect: &Effect) -> anyhow::Result<State> {
        let grants = match setting.project {
            Some(folder) => projects::grants(run.snapshot(), folder)?,
            None => Grants::default(),
        };
        let bounds = Bounds {
            workspace: setting.base.workspace.clone(),
            places: setting.places.to_vec(),
            confines: setting.confines,
        };
        match weigh(effect, setting.mode, &grants, &bounds) {
            Verdict::Go(writes) => self.exec(at, run, setting, writes),
            Verdict::Ask => self.ask(at, run, setting),
            Verdict::Review => {
                let call = &self.0[at];
                let review = Review {
                    thread: setting.thread.to_owned(),
                    call: call.key.clone(),
                    shown: shown(&call.tool, &call.arguments, effect),
                };
                run.request(&format!("{}.review", call.key), &review)?;
                Ok(State::Judging)
            }
        }
    }

    fn ask(&self, at: usize, run: &mut Run<'_>, setting: &Setting<'_>) -> anyhow::Result<State> {
        let call = &self.0[at];
        let effect = setting
            .tools
            .effect(&call.tool, &serde_json::from_str(&call.arguments)?, setting.base)?;
        let permission = Permission {
            thread: setting.thread.to_owned(),
            call: call.key.clone(),
            shown: shown(&call.tool, &call.arguments, &effect),
            always: patterns_of(&effect),
        };
        run.request(&format!("{}.permission", call.key), &permission)?;
        Ok(State::Asking)
    }

    fn exec(&self, at: usize, run: &mut Run<'_>, setting: &Setting<'_>, writes: Writes) -> anyhow::Result<State> {
        let call = &self.0[at];
        let arguments: serde_json::Value = serde_json::from_str(&call.arguments)?;
        let parent = bytes::Bytes::copy_from_slice(run.path());
        if let Some(started) = setting
            .tools
            .start(&mut run.changes, &parent, &call.key, &call.tool, &arguments, setting)
        {
            started?;
            return Ok(State::Program);
        }
        if let Some(answer_schema) = setting.tools.asks(&call.tool, &arguments) {
            let asks = Asks {
                thread: setting.thread.to_owned(),
                call: call.key.clone(),
                tool: call.tool.clone(),
                arguments,
                answer_schema,
            };
            run.request(&call.key, &asks)?;
            return Ok(State::Running);
        }
        let exec = Exec {
            thread: setting.thread.to_owned(),
            call: call.key.clone(),
            tool: call.tool.clone(),
            arguments: serde_json::from_str(&call.arguments)?,
            writes,
            workspace: setting.base.workspace.to_string_lossy().into_owned(),
        };
        run.request(&call.key, &exec)?;
        Ok(State::Running)
    }
}
