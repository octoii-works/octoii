//! The calls a model made, as their caller — a turn, a script — takes them on in its phases, each a request of its
//! caller's: checked, weighed, the person or a second model asked where the weighing says, then exec'd. A call that
//! reads starts at once; one that may act starts once every call before it ended, and holds back every call after it
//! till it ended — as the model wrote them, one after another, what it acts on is in the state it left. What they come to
//! is kept in the caller's checkpoint, and each ends once, with what the model reads.

use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use siphon_machine::paths::Base;
use siphon_runtime::{Answer, Run, Wait};
use siphon_threads::Mode;

use crate::effect::{Effect, Line};
use crate::permissions::{Bounds, Grants, Verdict, Writes, patterns_of, weigh};
use crate::projects;
use crate::requests::{Asks, Decision, Exec, Permission, Review, Shown};
use crate::tool::Tools;

/// How a call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Ok,
    /// Its tool is none of those the model may call.
    Unavailable,
    InvalidArguments,
    /// Its tool refused, or failed.
    ToolError,
    /// The person, or a second model, did not let it go ahead.
    Rejected,
    /// It was cut off with nothing to give: by a stop, or by the host's end.
    Interrupted,
}

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
    Ended {
        status: Status,
        output: Vec<InputContent>,
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

/// What the calls are weighed and exec'd by: the thread, its tools, the names of the MCP servers' tools its model is
/// told of, its mode, where its paths start, its project's folder where it has one, and whether the machine confines a
/// command to a sandbox.
#[derive(Debug, Clone, Copy)]
pub struct Setting<'a> {
    pub thread: &'a str,
    pub tools: &'a Tools,
    pub catalog: &'a [String],
    pub mode: Mode,
    pub base: &'a Base,
    pub project: Option<&'a str>,
    pub confines: bool,
}

/// A model's calls, as their caller takes them on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Calls(pub Vec<Call>);

fn ended(status: Status, words: impl Into<String>) -> State {
    State::Ended {
        status,
        output: vec![InputContent::text(words)],
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
        let mut calls = Vec::with_capacity(made.len());
        for (id, tool, arguments) in made {
            calls.push(Call {
                id,
                key: run.new_id()?,
                tool,
                arguments,
                acts: None,
                state: State::Waiting,
            });
        }
        Ok(Self(calls))
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
                State::Waiting | State::Ended { .. } => continue,
            };
            wait = wait.mail(key);
            waits = true;
        }
        Ok(waits.then_some(wait))
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
                        Status::Rejected,
                        reason.map_or_else(
                            || "the person declined it".into(),
                            |reason| format!("the person declined it: {reason}"),
                        ),
                    ),
                    Answer::Failed(_) | Answer::Interrupted => {
                        ended(Status::Interrupted, "the call was cut off before the person answered")
                    }
                }
            }
            State::Judging => {
                let key = format!("{}.review", call.key);
                let Some(answer) = run.answer::<Review>(&key)? else { return Ok(()) };
                run.take_mail(&key);
                match answer {
                    Answer::Given(judgment) if judgment.allow => self.exec(at, run, setting, Writes::Anywhere)?,
                    Answer::Given(judgment) => ended(Status::Rejected, format!("blocked: {}", judgment.reason)),
                    // A judgment that came to nothing leaves the call to the person.
                    Answer::Failed(_) => self.ask(at, run, setting)?,
                    Answer::Interrupted => ended(Status::Interrupted, "the call was cut off before it was judged"),
                }
            }
            State::Running if setting.tools.asks(&call.tool, &serde_json::from_str(&call.arguments)?).is_some() => {
                let Some(answer) = run.answer::<Asks>(&call.key)? else {
                    return Ok(());
                };
                run.take_mail(&call.key);
                match answer {
                    Answer::Given(given) => State::Ended {
                        status: Status::Ok,
                        output: setting.tools.answered(&call.tool, &serde_json::from_str(&call.arguments)?, given),
                    },
                    Answer::Failed(words) => ended(Status::ToolError, words),
                    Answer::Interrupted => ended(Status::Interrupted, "the call was cut off before the person answered"),
                }
            }
            State::Running => {
                let Some(answer) = run.answer::<Exec>(&call.key)? else {
                    return Ok(());
                };
                run.take_mail(&call.key);
                match answer {
                    Answer::Given(output) => State::Ended {
                        status: Status::Ok,
                        output,
                    },
                    Answer::Failed(words) => ended(Status::ToolError, words),
                    Answer::Interrupted => ended(Status::Interrupted, "the call was cut off"),
                }
            }
            State::Program => {
                let process = beak::layout::process(run.path(), call.key.as_bytes());
                let Some(outcome) = run.snapshot().record(&process)?.and_then(|record| record.outcome) else {
                    return Ok(());
                };
                run.changes.reap(process);
                State::Ended {
                    status: if matches!(outcome, beak::Outcome::Completed(_)) {
                        Status::Ok
                    } else {
                        Status::ToolError
                    },
                    output: setting.tools.finished(&call.tool, &outcome),
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
                    .map_err(|error| crate::tool::Refusal(format!("invalid arguments: {error}")))
                    .and_then(|arguments| setting.tools.effect(&call.tool, &arguments, setting.base));
                match effect {
                    Err(refusal) => {
                        let status = if setting.tools.has(&call.tool) {
                            Status::InvalidArguments
                        } else {
                            Status::Unavailable
                        };
                        self.0[at].state = ended(status, refusal.0);
                    }
                    Ok(effect) => {
                        let acts = effect != Effect::Reads;
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
            confines: setting.confines,
        };
        match weigh(effect, setting.mode, &grants, &bounds) {
            Verdict::Go(writes) => self.exec(at, run, setting, writes),
            Verdict::Ask => self.ask(at, run, setting),
            Verdict::Review => {
                let call = &self.0[at];
                let review = Review {
                    thread: setting.thread.to_owned(),
                    call: shown(&call.tool, &call.arguments, effect),
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
            call: shown(&call.tool, &call.arguments, &effect),
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
