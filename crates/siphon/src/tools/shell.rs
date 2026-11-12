//! `shell`: a command line run in the workspace by the machine's shell, its input closed, its output and errors one
//! stream — held to the workspace and the places beside it in a sandbox where its weighing said so, or anywhere — and
//! a line after it of how it ended.

use std::time::Duration;

use crate::machine::paths::Base;
use crate::machine::{Errors, Program, RunError};
use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::tools::effect::Effect;
use crate::tools::told::{CommandEnd, Structured, Told};
use crate::tools::tool::{Context, Refusal, Tool};

/// How long a command line runs where its call says no other time, and at most.
const TIMEOUT_SECONDS: u64 = 120;
const TIMEOUT_SECONDS_MAX: u64 = 600;

/// The arguments of `shell`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShellArgs {
    /// The command to run.
    pub command: String,
    /// Time limit in seconds; 120 by default, at most 600.
    #[schemars(range(min = 1, max = 600))]
    pub timeout_seconds: Option<u64>,
    /// Run outside the sandbox, once the person approves.
    #[serde(default)]
    pub escalate: bool,
    /// Why it must write outside the sandbox, in one sentence for the person.
    pub justification: Option<String>,
}

/// `shell`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShellTool;

impl Tool for ShellTool {
    const NAME: &'static str = "shell";
    const DESCRIPTION: &'static str = "Run a bash command in the workspace (no need to cd there). Its input is closed. Returns stdout and stderr together, then a note of how it ended: exit N, a signal, or a timeout. The time limit is 120 seconds; set `timeout_seconds` for up to 600. For reading, searching and editing files, use read, grep, glob and edit instead. In default mode commands run in a sandbox that can write only the workspace and the folders <environment> lists; to write anywhere else (such as a global install), run the command again with `escalate` and a `justification`, and the person decides.";
    type Args = ShellArgs;

    fn effect(&self, args: &ShellArgs, _: &Base) -> Effect {
        Effect::Runs {
            line: args.command.clone(),
            outside_sandbox: args.escalate,
            justification: args.justification.clone(),
        }
    }

    async fn exec(&self, context: &Context, args: ShellArgs) -> Result<Told, Refusal> {
        let seconds = args.timeout_seconds.unwrap_or(TIMEOUT_SECONDS).clamp(1, TIMEOUT_SECONDS_MAX);
        let program = Program {
            timeout: Duration::from_secs(seconds),
            errors: Errors::WithOutput,
            set: context.variables(),
            ..Program::new(vec!["bash".into(), "-c".into(), args.command], &context.base.workspace)
        };
        let machine = context.machine()?;
        let time = context.runtime.now_ms();
        let ran = machine.run(time, &context.process, &program, &context.cancelled).await;
        let (output, end) = match ran {
            Ok(ran) => {
                let end = match ran.exit {
                    beak::Exit::Code(code) => CommandEnd::Exited { code },
                    beak::Exit::Signal(signal) => CommandEnd::Signalled { signal },
                };
                (crate::reminder::escaped(&String::from_utf8_lossy(&ran.output)).into_owned(), end)
            }
            Err(RunError::TimedOut(_)) => (String::new(), CommandEnd::TimedOut { seconds }),
            Err(RunError::Stopped) => (String::new(), CommandEnd::Stopped),
            Err(RunError::MissingProgram(program)) => return Err(Refusal(format!("no {program} on the machine"))),
            Err(RunError::OutputTooLarge(bytes)) => (String::new(), CommandEnd::TooLarge { bytes }),
            Err(RunError::Beak(error)) => return Err(Refusal(error.to_string())),
        };
        let ended = match end {
            CommandEnd::Exited { code } => format!("<system-reminder>exit {code}</system-reminder>"),
            CommandEnd::Signalled { signal } => format!("<system-reminder>ended by signal {signal}</system-reminder>"),
            CommandEnd::TimedOut { seconds } => format!("<system-reminder>its {seconds} s ran out; it was ended</system-reminder>"),
            CommandEnd::Stopped => "<system-reminder>it was stopped</system-reminder>".into(),
            CommandEnd::TooLarge { bytes } => format!("<system-reminder>it wrote past {bytes} bytes; it was ended</system-reminder>"),
        };
        let separator = if output.is_empty() || output.ends_with('\n') { "" } else { "\n" };
        let words = vec![InputContent::text(format!("{output}{separator}{ended}"))];
        Ok(Told::structured(words, &Structured::Ran { ended: end }))
    }
}
