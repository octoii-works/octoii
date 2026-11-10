//! `shell`: a command line run in the workspace by the machine's shell, its input closed, its output and errors one
//! stream — held to the workspace and the temporary folders in a sandbox where its weighing said so, or anywhere — and
//! a line after it of how it ended.

use std::time::Duration;

use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::Deserialize;
use siphon_machine::paths::Base;
use siphon_machine::{Errors, Program, RunError};

use crate::effect::Effect;
use crate::tool::{Context, Refusal, Tool};

/// How long a command line runs where its call says no other time, and at most.
const TIMEOUT_SECONDS: u64 = 120;
const TIMEOUT_SECONDS_MAX: u64 = 600;

/// The arguments of `shell`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ShellArgs {
    /// The command line.
    pub command: String,
    /// How long it may run, in seconds; 120 unless given.
    #[schemars(range(min = 1, max = 600))]
    pub timeout_seconds: Option<u64>,
    /// Run it beyond the sandbox, once the person lets it.
    #[serde(default)]
    pub escalate: bool,
    /// Why it must write beyond the sandbox, in a sentence the person reads.
    pub justification: Option<String>,
}

/// `shell`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ShellTool;

impl Tool for ShellTool {
    const NAME: &'static str = "shell";
    const DESCRIPTION: &'static str = "Runs a command line in the workspace in bash, its input closed; gives its output and errors as one stream, then a line of how it ended: its exit code, the signal that ended it, or its time run out. In the default mode it runs in a sandbox that writes the workspace and the temporary folders alone; one that must write beyond them — install a package for the machine, write elsewhere — is run again with escalate and a justification, and the person decides.";
    type Args = ShellArgs;

    fn effect(&self, args: &ShellArgs, _: &Base) -> Effect {
        Effect::Runs {
            line: args.command.clone(),
            outside_sandbox: args.escalate,
            justification: args.justification.clone(),
        }
    }

    async fn exec(&self, context: &Context, args: ShellArgs) -> Result<Vec<InputContent>, Refusal> {
        let seconds = args.timeout_seconds.unwrap_or(TIMEOUT_SECONDS).clamp(1, TIMEOUT_SECONDS_MAX);
        let program = Program {
            timeout: Duration::from_secs(seconds),
            errors: Errors::WithOutput,
            ..Program::new(vec!["bash".into(), "-c".into(), args.command], &context.base.workspace)
        };
        let machine = context.machine()?;
        let time = context.runtime.now_ms();
        let ran = machine.run(time, &context.process, &program, &context.cancelled).await;
        let (output, ended) = match ran {
            Ok(ran) => {
                let ended = match ran.exit {
                    beak::Exit::Code(code) => format!("[exit {code}]"),
                    beak::Exit::Signal(signal) => format!("[ended by signal {signal}]"),
                };
                (String::from_utf8_lossy(&ran.output).into_owned(), ended)
            }
            Err(RunError::TimedOut(_)) => (String::new(), format!("[its {seconds} s ran out; it was ended]")),
            Err(RunError::Stopped) => (String::new(), "[it was stopped]".into()),
            Err(RunError::MissingProgram(program)) => return Err(Refusal(format!("no {program} on the machine"))),
            Err(RunError::OutputTooLarge(max)) => (String::new(), format!("[it wrote past {max} bytes; it was ended]")),
            Err(RunError::Beak(error)) => return Err(Refusal(error.to_string())),
        };
        let separator = if output.is_empty() || output.ends_with('\n') { "" } else { "\n" };
        Ok(vec![InputContent::text(format!("{output}{separator}{ended}"))])
    }
}
