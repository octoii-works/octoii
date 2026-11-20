//! Where a call's exec request is answered, and where every call's model output is bounded as it ends: text past what
//! the model is told at once cut to its head and its newest tail, the whole kept in its thread's `outputs/<call>.txt`,
//! read back through the threads view where the host exposes it.

use octoii_siphon_lib::Batch;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::machine::Host;
use crate::machine::paths::Base;
use crate::threads::thread;
use octoii_ai::vocabulary::InputContent;
use octoii_siphon_lib::{Asked, RuntimeHandle, Unanswered};
use tokio_util::sync::CancellationToken;

use crate::tools::requests::Exec;
use crate::tools::told::Told;
use crate::tools::tool::{Context, Tools};

/// How many characters of a call's text the model is told at once; past them, its head and its newest tail.
pub const TEXT_CHARS_MAX: usize = 30_000;

/// Answers the exec request `asked` with `tools`, on the machine `host` gives.
///
/// # Errors
/// `Failed` if the request's command capability is unavailable or the tool refuses. `Interrupted` if the request
/// stops before the tool starts.
pub async fn answer_exec(tools: &Tools, host: &Host, runtime: &RuntimeHandle, asked: Asked<Exec>) -> Result<Told, Unanswered> {
    let request_commands = asked.commands().map_err(|error| Unanswered::Failed(error.to_string()))?;
    let cancelled = CancellationToken::new();
    let exec = asked.payload;
    let context = Context {
        thread: exec.thread,
        call: exec.call,
        base: Base {
            workspace: PathBuf::from(&exec.workspace),
            home: host.home.clone(),
        },
        writes: exec.writes,
        places: exec.places,
        host: host.clone(),
        runtime: runtime.clone(),
        request_commands,
        cancelled: cancelled.clone(),
    };
    let execution = async {
        if cancelled.is_cancelled() {
            return Err(Unanswered::Interrupted);
        }
        tools
            .exec(&exec.tool, context, exec.arguments)
            .await
            .map_err(|refusal| Unanswered::Failed(refusal.0))
    };
    tokio::pin!(execution);
    tokio::select! {
        biased;
        () = asked.stop.requested() => {
            cancelled.cancel();
            execution.await
        }
        result = &mut execution => result,
    }
}

/// `given` as the model is told it: its text, past `TEXT_CHARS_MAX`, cut to its head and its newest tail, the whole kept
/// in its thread's outputs and named.
pub(super) fn bounded(
    changes: &mut Batch<'_>,
    thread: &str,
    call: &str,
    threads: Option<&Path>,
    given: Vec<InputContent>,
) -> octoii_siphon_lib::Result<Vec<InputContent>> {
    let whole: String = given
        .iter()
        .filter_map(|part| match part {
            InputContent::InputText { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let chars = whole.chars().count();
    if chars <= TEXT_CHARS_MAX {
        return Ok(given);
    }
    let half = TEXT_CHARS_MAX / 2;
    let head: String = whole.chars().take(half).collect();
    let tail: String = whole.chars().skip(chars - half).collect();
    let mut cut = format!(
        "{head}\n<system-reminder>… {} characters left out …</system-reminder>\n{tail}",
        chars - TEXT_CHARS_MAX
    );
    match keep(changes, thread, call, threads, &whole)? {
        Some(place) => {
            let _ = write!(
                cut,
                "\n<system-reminder>the whole of it is kept at {place}; read it a page at a time</system-reminder>"
            );
        }
        None => cut.push_str(
            "\n<system-reminder>the whole of it is kept in the thread's volume; this host does not expose it for file reads</system-reminder>",
        ),
    }
    let mut out = vec![InputContent::text(cut)];
    out.extend(given.into_iter().filter(|part| !matches!(part, InputContent::InputText { .. })));
    Ok(out)
}

/// Keeps `whole` with the call's result: where the model reads it through the threads view, where the host exposes one.
fn keep(
    changes: &mut Batch<'_>,
    thread: &str,
    call: &str,
    threads: Option<&Path>,
    whole: &str,
) -> octoii_siphon_lib::Result<Option<String>> {
    let outputs = thread::within(thread, "outputs");
    let file = format!("{call}.txt");
    changes
        .mkdir(&outputs)?
        .write([&outputs[..], b"/", file.as_bytes()].concat(), whole)?;
    Ok(threads.map(|folder| folder.join(thread).join("outputs").join(&file).display().to_string()))
}
