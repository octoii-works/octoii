//! `execute`: the model writes JavaScript that calls its tools — in loops, at once, working on their answers in code —
//! run on the script engine, the tools under `tools`, each taking the arguments its function takes. A script is a
//! process of its own under its call, as it may wait on a person through the host's restarts: each phase runs it again
//! from its start on an engine of that phase alone — the same code, seed, settlements and times, so it runs as it first
//! did — till it makes calls not yet settled. Those are taken on as every model's calls are, each weighed, asked and
//! exec'd as its own; once they ended, their results are settled into the next run of it. Its end is what it returned,
//! or what it threw, and what it wrote to its console, with the instructions its calls came upon.

use std::fmt::Write as _;

use crate::machine::paths::Base;
use crate::threads::Mode;
use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use siphon_runtime::{Changes, Ending, Program, Run};
use siphon_script::{Host, Limits, Progress, Script};

use crate::threads::CallStatus;

use crate::tools::calls::{Calls, Setting, State};
use crate::tools::effect::Effect;
use crate::tools::told::{Instructions, Told};
use crate::tools::tool::{Context, Refusal, Tool, Tools};

/// The arguments of `execute`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ExecuteArgs {
    /// JavaScript, run as the body of an async function: what it returns is what the script gives.
    pub code: String,
}

/// `execute`, its scripts calling the tools its `ScriptProgram` holds.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExecuteTool;

/// What a script's process is started with: its thread, its code, the MCP servers' tools it may call beside the
/// built-in ones, and how its calls are weighed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptInput {
    pub thread: String,
    pub code: String,
    pub catalog: Vec<String>,
    pub workspace: String,
    pub home: Option<String>,
    pub project: Option<String>,
    pub places: Vec<std::path::PathBuf>,
    pub mode: Mode,
    pub confines: bool,
}

/// A call of a script's settled: its id, whether it went well, what it gave, and when.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settled {
    pub id: u32,
    pub ok: bool,
    pub text: String,
    pub time_ms: i64,
}

/// Where a script stands: when it began, the calls settled, and the calls being taken on.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScriptState {
    pub began_ms: i64,
    pub settled: Vec<Settled>,
    pub calls: Option<Calls>,
    /// The instructions its calls came upon, each once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instructions: Vec<Instructions>,
}

/// What a script came to: what it returned, as JSON, or what it threw; what it wrote to its console; and the
/// instructions its calls came upon.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub console: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instructions: Vec<Instructions>,
}

impl Tool for ExecuteTool {
    const NAME: &'static str = "execute";
    const DESCRIPTION: &'static str = "Runs JavaScript as the body of an async function, calling tools through `tools`: `await tools.read({ path: \"a.txt\" })` resolves to what the tool gives, as text, and rejects where the tool refuses. Call them in loops, at once with Promise.all, and work on what they give in code. Gives what the script returned, as JSON, and what it wrote with console.log. Use it for work of many steps: many files read or changed, a page's many actions, a batch extracted. `tools` holds each of your tools by its name — but this one and those that ask the person or start threads — each taking the arguments its function does, and the tools of <catalog>.";
    type Args = ExecuteArgs;

    fn effect(&self, _: &ExecuteArgs, _: &Base) -> Effect {
        Effect::Calls
    }

    fn exec(&self, _: &Context, _: ExecuteArgs) -> impl std::future::Future<Output = Result<Told, Refusal>> + Send {
        std::future::ready(Err(Refusal::new("a script runs as a program of its own")))
    }

    fn start(
        &self,
        changes: &mut Changes,
        parent: &Bytes,
        key: &str,
        args: &ExecuteArgs,
        setting: &Setting<'_>,
    ) -> Option<serde_json::Result<()>> {
        let input = ScriptInput {
            thread: setting.thread.to_owned(),
            code: args.code.clone(),
            catalog: setting.catalog.to_vec(),
            workspace: setting.base.workspace.to_string_lossy().into_owned(),
            home: setting.base.home.as_ref().map(|home| home.to_string_lossy().into_owned()),
            project: setting.project.map(str::to_owned),
            places: setting.places.to_vec(),
            mode: setting.mode,
            confines: setting.confines,
        };
        Some(changes.start::<ScriptProgram>(parent.clone(), key, &input).map(|_| ()))
    }

    fn finished(&self, outcome: &beak::Outcome) -> Told {
        let read = match outcome {
            beak::Outcome::Completed(Some(result)) => serde_json::from_slice::<ScriptOutput>(result).ok(),
            _ => None,
        };
        let Some(output) = read else {
            return Told::from(vec![InputContent::text(format!("the script ended as {outcome:?}"))]);
        };
        let mut text = output.console.join("\n");
        match (output.value, output.error) {
            (_, Some(error)) => {
                let _ = write!(text, "\n[the script threw: {error}]");
            }
            (Some(value), None) => {
                let _ = write!(text, "\n{value}");
            }
            (None, None) => {}
        }
        Told {
            output: vec![InputContent::text(text.trim_start().to_owned())],
            structured: None,
            instructions: output.instructions,
            plan: crate::tools::Plan::default(),
        }
    }
}

/// The program a script's process runs, its scripts calling `tools`, what they plan for threads done by `primitives`.
#[derive(Clone)]
pub struct ScriptProgram {
    pub tools: Tools,
    pub primitives: std::sync::Arc<dyn crate::tools::Primitives>,
}

impl std::fmt::Debug for ScriptProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptProgram").field("tools", &self.tools).finish_non_exhaustive()
    }
}

/// The text of a call's output, as a script is given it.
fn text_of(output: &[InputContent]) -> String {
    output
        .iter()
        .filter_map(|part| match part {
            InputContent::InputText { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What a run of the script came to: its end, or the calls it made that are not yet settled.
enum Replayed {
    Ended(ScriptOutput),
    Calls(Vec<siphon_script::Call>),
}

impl ScriptProgram {
    /// Runs the script `input` gives from its start, settling each call as `state` holds it, to its end or to calls
    /// not yet settled.
    fn replay(&self, input: &ScriptInput, state: &ScriptState) -> Replayed {
        let names: Vec<String> = self.tools.declared().into_iter().map(|declared| declared.name).collect();
        let functions: Vec<&str> = names.iter().chain(&input.catalog).map(String::as_str).collect();
        let host = Host {
            namespace: "tools",
            functions: &functions,
            limits: Limits::default(),
            seed: u64::try_from(state.began_ms).unwrap_or_default(),
        };
        let (mut script, mut progress) = Script::start(&input.code, &host, state.began_ms);
        loop {
            match progress {
                Progress::Waiting(calls) => {
                    let mut unsettled = Vec::new();
                    let mut time = state.began_ms;
                    for call in calls {
                        match state.settled.iter().find(|settled| settled.id == call.id) {
                            Some(settled) => {
                                let json = serde_json::to_string(&settled.text).unwrap_or_default();
                                let _ = script.settle(call.id, if settled.ok { Ok(&json) } else { Err(&settled.text) });
                                time = time.max(settled.time_ms);
                            }
                            None => unsettled.push(call),
                        }
                    }
                    if !unsettled.is_empty() {
                        return Replayed::Calls(unsettled);
                    }
                    progress = script.resume(time);
                }
                Progress::Returned(value) => {
                    return Replayed::Ended(ScriptOutput {
                        value: Some(value),
                        error: None,
                        console: script.output(),
                        instructions: state.instructions.clone(),
                    });
                }
                Progress::Threw(thrown) => {
                    return Replayed::Ended(ScriptOutput {
                        value: None,
                        error: Some(format!("{}: {}", thrown.name, thrown.message)),
                        console: script.output(),
                        instructions: state.instructions.clone(),
                    });
                }
                Progress::OutOfSteps => {
                    return Replayed::Ended(ScriptOutput {
                        value: None,
                        error: Some("it ran past the steps a script may take".into()),
                        console: script.output(),
                        instructions: state.instructions.clone(),
                    });
                }
            }
        }
    }
}

impl Program for ScriptProgram {
    const KIND: &'static str = "siphon.script";
    type Input = ScriptInput;
    type Checkpoint = ScriptState;
    type Output = ScriptOutput;

    fn run(
        &self,
        run: &mut Run<'_>,
        input: ScriptInput,
        checkpoint: Option<ScriptState>,
    ) -> anyhow::Result<Ending<ScriptState, ScriptOutput>> {
        let mut state = checkpoint.unwrap_or_else(|| ScriptState {
            began_ms: run.time_ms(),
            ..ScriptState::default()
        });
        if state.calls.is_none() {
            match self.replay(&input, &state) {
                Replayed::Ended(output) => return Ok(Ending::Done(output)),
                Replayed::Calls(made) => {
                    let made = made
                        .into_iter()
                        .map(|call| (call.id.to_string(), call.function, call.arguments))
                        .collect();
                    state.calls = Some(Calls::new(run, made)?);
                }
            }
        }
        let base = Base {
            workspace: input.workspace.clone().into(),
            home: input.home.clone().map(Into::into),
        };
        let setting = Setting {
            thread: &input.thread,
            tools: &self.tools,
            primitives: &*self.primitives,
            catalog: &input.catalog,
            mode: input.mode,
            base: &base,
            project: input.project.as_deref(),
            places: &input.places,
            confines: input.confines,
        };
        let calls = state.calls.as_mut().expect("calls being taken on");
        if let Some(wait) = calls.advance(run, &setting)? {
            return Ok(Ending::Wait(state, wait));
        }
        let ended = state.calls.take().map(|calls| calls.0).unwrap_or_default();
        for call in ended {
            let State::Ended {
                status,
                output,
                instructions,
                ..
            } = call.state
            else {
                continue;
            };
            for each in instructions {
                if !state.instructions.iter().any(|told| told.path == each.path) {
                    state.instructions.push(each);
                }
            }
            state.settled.push(Settled {
                id: call.id.parse().unwrap_or_default(),
                ok: status == CallStatus::Ok,
                text: text_of(&output),
                time_ms: run.time_ms(),
            });
        }
        Ok(Ending::Next(state))
    }
}
