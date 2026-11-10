//! Where a call's exec request is answered: its tool exec'd in the context its request names, and what it gives bounded
//! as every tool's is — text past what the model is told at once cut to its head and its newest tail, the whole kept in
//! its thread's `outputs/<call>.txt`, read back through the threads view as any file is.

use std::fmt::Write as _;
use std::path::PathBuf;

use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use siphon_machine::Host;
use siphon_machine::paths::Base;
use siphon_runtime::{Asked, Changes, Unanswered};
use siphon_threads::thread;

use crate::requests::Exec;
use crate::tool::{Context, Tools};

/// How many characters of a call's text the model is told at once; past them, its head and its newest tail.
pub const TEXT_CHARS_MAX: usize = 30_000;

/// Answers the exec request `asked` with `tools`, on the machine `host` gives.
///
/// # Errors
/// `Failed` with the tool's refusal.
pub async fn answer_exec(tools: &Tools, host: &Host, asked: Asked<Exec>) -> Result<Vec<InputContent>, Unanswered> {
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
        kernel: asked.runtime.kernel().clone(),
        host: host.clone(),
        runtime: asked.runtime,
        cancelled: asked.cancelled,
    };
    let given = tools
        .exec(&exec.tool, context.clone(), exec.arguments)
        .await
        .map_err(|refusal| Unanswered::Failed(refusal.0))?;
    Ok(bounded(&context, given).await)
}

/// `given` as the model is told it: its text, past `TEXT_CHARS_MAX`, cut to its head and its newest tail, the whole kept
/// in its thread's outputs and named.
async fn bounded(context: &Context, given: Vec<InputContent>) -> Vec<InputContent> {
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
    let mut cut = format!("{head}\n[… {} characters left out …]\n{tail}", chars - TEXT_CHARS_MAX);
    match keep(context, Bytes::from(whole)).await {
        Some(place) => {
            let _ = write!(cut, "\n[the whole of it is kept at {place}; read it a page at a time]");
        }
        None => cut.push_str("\n[the whole of it could not be kept]"),
    }
    let mut out = vec![InputContent::text(cut)];
    out.extend(given.into_iter().filter(|part| !matches!(part, InputContent::InputText { .. })));
    out
}

/// Keeps `whole` in the call's thread's outputs: where the model reads it, through the threads view, or the volume's
/// path where the host shows the threads nowhere; none where it could not be kept.
async fn keep(context: &Context, whole: Bytes) -> Option<String> {
    let outputs = thread::within(&context.thread, "outputs");
    let file = format!("{}.txt", context.call);
    let mut changes = Changes::new();
    changes
        .mkdir(outputs.clone())
        .write([&outputs[..], b"/", file.as_bytes()].concat(), whole);
    context.runtime.commit(changes).await.ok()?;
    let shown = match &context.host.threads {
        Some(folder) => folder
            .join(String::from_utf8_lossy(&thread::path(&context.thread)).as_ref())
            .join("outputs")
            .join(&file)
            .display()
            .to_string(),
        None => format!("{}/{file}", String::from_utf8_lossy(&outputs)),
    };
    Some(shown)
}
