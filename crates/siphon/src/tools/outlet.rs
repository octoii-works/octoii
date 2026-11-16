//! Where a call's exec request is answered, and where every call's model output is bounded as it ends: text past what
//! the model is told at once cut to its head and its newest tail, the whole kept in its thread's `outputs/<call>.txt`,
//! read back through the threads view where the host exposes it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::machine::Host;
use crate::machine::paths::Base;
use crate::threads::thread;
use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use siphon_runtime::{Asked, Changes, Unanswered};

use crate::tools::requests::Exec;
use crate::tools::told::Told;
use crate::tools::tool::{Context, Tools};

/// How many characters of a call's text the model is told at once; past them, its head and its newest tail.
pub const TEXT_CHARS_MAX: usize = 30_000;

/// Answers the exec request `asked` with `tools`, on the machine `host` gives.
///
/// # Errors
/// `Failed` with the tool's refusal.
pub async fn answer_exec(tools: &Tools, host: &Host, asked: Asked<Exec>) -> Result<Told, Unanswered> {
    let exec = asked.payload;
    let context = Context {
        thread: exec.thread,
        process: asked.process,
        call: exec.call,
        base: Base {
            workspace: PathBuf::from(&exec.workspace),
            home: host.home.clone(),
        },
        writes: exec.writes,
        places: exec.places,
        kernel: asked.runtime.kernel().clone(),
        host: host.clone(),
        runtime: asked.runtime,
        cancelled: asked.cancelled,
    };
    tools
        .exec(&exec.tool, context, exec.arguments)
        .await
        .map_err(|refusal| Unanswered::Failed(refusal.0))
}

/// `given` as the model is told it: its text, past `TEXT_CHARS_MAX`, cut to its head and its newest tail, the whole kept
/// in its thread's outputs and named.
pub(super) fn bounded(
    changes: &mut Changes,
    thread: &str,
    call: &str,
    threads: Option<&Path>,
    given: Vec<InputContent>,
) -> Vec<InputContent> {
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
        return given;
    }
    let half = TEXT_CHARS_MAX / 2;
    let head: String = whole.chars().take(half).collect();
    let tail: String = whole.chars().skip(chars - half).collect();
    let mut cut = format!(
        "{head}\n<system-reminder>… {} characters left out …</system-reminder>\n{tail}",
        chars - TEXT_CHARS_MAX
    );
    match keep(changes, thread, call, threads, Bytes::from(whole)) {
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
    out
}

/// Keeps `whole` with the call's result: where the model reads it through the threads view, where the host exposes one.
fn keep(changes: &mut Changes, thread: &str, call: &str, threads: Option<&Path>, whole: Bytes) -> Option<String> {
    let outputs = thread::within(thread, "outputs");
    let file = format!("{call}.txt");
    changes
        .mkdir(outputs.clone())
        .write([&outputs[..], b"/", file.as_bytes()].concat(), whole);
    threads.map(|folder| folder.join(thread).join("outputs").join(&file).display().to_string())
}
