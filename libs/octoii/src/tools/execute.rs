//! `execute`: the model writes JavaScript that calls its tools — in loops, at once, working on their answers in code —
//! run on the script engine, the tools under `tools`, each taking the arguments its function takes. A script is a
//! process of its own under its call, as it may wait on a person through the host's restarts: each phase runs it again
//! from its start on an engine of that phase alone — the same code, seed, settlements and times, so it runs as it first
//! did — till it makes calls not yet settled. Those are taken on as every model's calls are, each weighed, asked and
//! exec'd as its own; once they ended, their results are settled into the next run of it. Its end is what it returned,
//! or what it threw, and what it wrote to its console, with the instructions its calls came upon.

use std::fmt::Write as _;

use crate::machine::paths::Base;
use crate::threads::{Mode, Parent, SubThreadKind, thread};
use octoii_ai::vocabulary::InputContent;
use octoii_siphon_lib::{Batch, Ending, Fiber, FiberId, Run};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use siphon_script::{Host, Limits, Progress, Script};

use crate::threads::CallStatus;

use crate::tools::calls::{Calls, Setting, State};
use crate::tools::effect::Effect;
use crate::tools::told::{Instructions, Told};
use crate::tools::{Context, Finished, Refusal, Tool, Tools};

/// The arguments of `execute`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ExecuteArgs {
    /// JavaScript run as the body of an async function; its return value is the result.
    pub code: String,
}

/// `execute`, its scripts calling the tools its `ScriptFiber` holds.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExecuteTool;

/// What a script's process is started with: its thread, its code, the MCP servers' tools it may call beside the
/// built-in ones, and how its calls are weighed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptInput {
    pub thread: String,
    pub threads: Option<std::path::PathBuf>,
    pub code: String,
    pub catalog: Vec<String>,
    pub workspace: String,
    pub home: Option<String>,
    pub project: Option<String>,
    pub places: Vec<std::path::PathBuf>,
    pub mode: Mode,
    pub confines: bool,
}

/// One of a script's calls that ended, as the tool boundary recorded it. Images and delivered files stay in the
/// script's explicit outer output; a call's text and typed result are enough for its own tool's presentation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExecutedCall {
    pub id: u32,
    pub key: String,
    pub tool: String,
    pub arguments: String,
    pub status: CallStatus,
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "structuredContent")]
    pub structured_content: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// The actual tool calls a completed script made, in the order it made them, including those of a script that threw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteResult {
    pub calls: Vec<ExecutedCall>,
}

/// A call of a script's settled: what it did, what its replay receives, and when it settled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settled {
    pub call: ExecutedCall,
    pub result: Result<serde_json::Value, String>,
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
    pub images: Vec<InputContent>,
    pub resources: Vec<InputContent>,
    pub calls: Vec<ExecutedCall>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instructions: Vec<Instructions>,
}

impl Tool for ExecuteTool {
    const NAME: &'static str = "execute";
    const DESCRIPTION: &'static str = "Run JavaScript as the body of an async function, with your tools on `tools`: `await tools.read({ path: \"a.txt\" })` returns { content, structuredContent?, metadata? } and rejects on failure. Use loops, Promise.all and code to work on the results. Text and images reach the model through explicit text(value), console.log, image(contentBlock), and the return value. No return or return undefined emits no value; explicit null remains null. Images use input_image blocks from tool results; metadata is not sent automatically. File delivery links are retained automatically, including when the script throws. Use it for work of many steps: reading or changing many files, many browser actions, batch extraction. `tools` has each of your tools by name, except execute and tools that ask the person or start threads, plus the tools listed in <catalog>.";
    type Args = ExecuteArgs;

    fn output_schema() -> Option<serde_json::Value> {
        Some(schemars::schema_for!(ExecuteResult).to_value())
    }

    fn effect(&self, _: &ExecuteArgs, _: &Base) -> Effect {
        Effect::Calls
    }

    fn exec(&self, _: &Context, _: ExecuteArgs) -> impl std::future::Future<Output = Result<Told, Refusal>> + Send {
        std::future::ready(Err(Refusal::new("a script runs in its own process")))
    }

    fn start(
        &self,
        changes: &mut Batch<'_>,
        parent: &FiberId,
        key: &str,
        args: &ExecuteArgs,
        setting: &Setting<'_>,
    ) -> Option<octoii_siphon_lib::Result<()>> {
        let input = ScriptInput {
            thread: setting.thread.to_owned(),
            threads: setting.threads.map(std::path::Path::to_owned),
            code: args.code.clone(),
            catalog: setting.catalog.to_vec(),
            workspace: setting.base.workspace.to_string_lossy().into_owned(),
            home: setting.base.home.as_ref().map(|home| home.to_string_lossy().into_owned()),
            project: setting.project.map(str::to_owned),
            places: setting.places.to_vec(),
            mode: setting.mode,
            confines: setting.confines,
        };
        Some(changes.start::<ScriptFiber>(Some(parent), key, &input).map(|_| ()))
    }

    fn finished(&self, outcome: &octoii_siphon_lib::Outcome) -> Finished {
        let read = match outcome {
            octoii_siphon_lib::Outcome::Completed(Some(result)) => serde_json::from_slice::<ScriptOutput>(result).ok(),
            _ => None,
        };
        let Some(output) = read else {
            return Finished {
                status: CallStatus::ToolError,
                told: Told::from(vec![InputContent::text(format!("the script ended as {outcome:?}"))]),
            };
        };
        let status = if output.error.is_some() {
            CallStatus::ToolError
        } else {
            CallStatus::Ok
        };
        let mut text = crate::reminder::escaped(&output.console.join("\n")).into_owned();
        match (output.value, output.error) {
            (_, Some(error)) => {
                let _ = write!(
                    text,
                    "\n<system-reminder>the script threw: {}</system-reminder>",
                    crate::reminder::escaped(&error)
                );
            }
            (Some(value), None) => {
                let _ = write!(text, "\n{}", crate::reminder::escaped(&value));
            }
            (None, None) => {}
        }
        Finished {
            status,
            told: Told {
                content: std::iter::once(InputContent::text(text.trim_start().to_owned()))
                    .chain(output.images)
                    .chain(output.resources)
                    .collect(),
                structured_content: Some(serde_json::json!(ExecuteResult { calls: output.calls })),
                metadata: None,
                instructions: output.instructions,
                plan: crate::tools::Plan::default(),
            },
        }
    }
}

/// The fiber a script's process runs, its scripts calling `tools`, what they plan for threads done by `primitives`.
#[derive(Clone)]
pub struct ScriptFiber {
    pub tools: Tools,
    pub primitives: std::sync::Arc<dyn crate::tools::Primitives>,
}

impl std::fmt::Debug for ScriptFiber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptFiber").field("tools", &self.tools).finish_non_exhaustive()
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

/// Finish with explicit outputs only. Resource references must have come from this script's calls.
fn script_output(script: &mut Script, state: &ScriptState, value: Option<String>, mut error: Option<String>) -> ScriptOutput {
    let mut images = Vec::new();
    for json in script.images() {
        let image = serde_json::from_str::<InputContent>(&json);
        match image {
            Ok(image @ InputContent::InputImage { .. })
                if state.settled.iter().any(|settled| {
                    settled
                        .result
                        .as_ref()
                        .ok()
                        .and_then(|result| result["content"].as_array())
                        .is_some_and(|content| content.contains(&serde_json::json!(image)))
                }) =>
            {
                images.push(image);
            }
            _ => {
                error = Some("image() expects an input_image block returned by a tool in this script".into());
                break;
            }
        }
    }
    ScriptOutput {
        value,
        error,
        resources: state
            .settled
            .iter()
            .filter_map(|settled| settled.result.as_ref().ok())
            .filter_map(|result| result["content"].as_array())
            .flatten()
            .filter_map(|value| serde_json::from_value::<InputContent>(value.clone()).ok())
            .filter(|part| matches!(part, InputContent::ResourceLink { .. }))
            .collect(),
        console: script.output(),
        images,
        calls: state.settled.iter().map(|settled| settled.call.clone()).collect(),
        instructions: state.instructions.clone(),
    }
}

/// What a run of the script came to: its end, or the calls it made that are not yet settled.
enum Replayed {
    Ended(ScriptOutput),
    Calls(Vec<siphon_script::Call>),
}

impl ScriptFiber {
    /// Runs the script `input` gives from its start, settling each call as `state` holds it, to its end or to calls
    /// not yet settled.
    fn replay(&self, input: &ScriptInput, state: &ScriptState, kind: Option<SubThreadKind>) -> Replayed {
        let names: Vec<String> = self.tools.declared().into_iter().map(|declared| declared.name).collect();
        let functions: Vec<&str> = names
            .iter()
            .chain(&input.catalog)
            .map(String::as_str)
            .filter(|name| self.tools.allows(name, kind))
            .collect();
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
                        match state.settled.iter().find(|settled| settled.call.id == call.id) {
                            Some(settled) => {
                                let json = settled.result.as_ref().map(serde_json::Value::to_string);
                                let _ = script.settle(call.id, json.as_deref().map_err(|error| error.as_str()));
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
                    return Replayed::Ended(script_output(&mut script, state, value, None));
                }
                Progress::Threw(thrown) => {
                    return Replayed::Ended(script_output(
                        &mut script,
                        state,
                        None,
                        Some(format!("{}: {}", thrown.name, thrown.message)),
                    ));
                }
                Progress::OutOfSteps => {
                    return Replayed::Ended(script_output(
                        &mut script,
                        state,
                        None,
                        Some("it ran past the steps a script may take".into()),
                    ));
                }
            }
        }
    }
}

impl Fiber for ScriptFiber {
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
            let directory = run.snapshot().stat(&thread::path(&input.thread))?.and_then(|entry| entry.directory);
            let parent = directory.as_ref().map(Parent::of).transpose()?.flatten();
            match self.replay(&input, &state, parent.map(|parent| parent.kind)) {
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
            threads: input.threads.as_deref(),
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
        let ended = state.calls.take().map_or_default(|calls| calls.0);
        for call in ended {
            let State::Ended {
                status,
                output,
                instructions,
                structured_content,
                metadata,
            } = call.state
            else {
                continue;
            };
            for each in instructions {
                if !state.instructions.iter().any(|told| told.path == each.path) {
                    state.instructions.push(each);
                }
            }
            let text = text_of(&output);
            let result = if status == CallStatus::Ok {
                Ok(serde_json::json!({
                    "content": output,
                    "structuredContent": structured_content,
                    "metadata": metadata,
                }))
            } else {
                Err(text.clone())
            };
            state.settled.push(Settled {
                call: ExecutedCall {
                    id: call.id.parse().unwrap_or_default(),
                    key: call.key,
                    tool: call.tool,
                    arguments: call.arguments,
                    status,
                    output: text,
                    structured_content,
                    metadata,
                },
                result,
                time_ms: run.time_ms(),
            });
        }
        Ok(Ending::Next(state))
    }
}
