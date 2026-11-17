//! The dispatcher: the phases due run a batch at a time, off the async threads.

use std::sync::Arc;
use std::time::Duration;

use super::Shared;
use crate::program::dispatch;

/// What the dispatcher found.
enum Dispatched {
    /// Runs were due, and ran: more may be.
    Ran,
    /// None was due: the kernel is next due then, or never.
    Idle(Option<i64>),
}

/// How long the dispatcher waits after a dispatch failed, before it tries again.
const FAILED_DISPATCH_WAIT: Duration = Duration::from_secs(1);

/// Runs the phases due, a batch at a time, off the async threads, till the runtime stops: between, it waits for a
/// commit, a wake, or the time the kernel is next due.
pub(super) async fn run_dispatcher(shared: Arc<Shared>) {
    while !shared.shutdown.is_cancelled() {
        let dispatching = Arc::clone(&shared);
        let wait = match tokio::task::spawn_blocking(move || dispatch_due(&dispatching)).await {
            Ok(Ok(Dispatched::Ran)) => continue,
            Ok(Ok(Dispatched::Idle(next))) => next.map(|at| Duration::from_millis(u64::try_from(at - shared.clock.now_ms()).unwrap_or(0))),
            Ok(Err(error)) => {
                tracing::error!(%error, "a dispatch failed");
                Some(FAILED_DISPATCH_WAIT)
            }
            Err(error) => {
                tracing::error!(%error, "a dispatch panicked");
                Some(FAILED_DISPATCH_WAIT)
            }
        };
        let due = async {
            match wait {
                Some(wait) => tokio::time::sleep(wait).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            () = shared.shutdown.cancelled() => return,
            () = shared.dispatch.notified() => {}
            () = due => {}
        }
    }
}

/// Runs each phase due now, each from the volume as the commit before it left it.
fn dispatch_due(shared: &Shared) -> beak::Result<Dispatched> {
    let now = shared.clock.now_ms();
    let runs = shared.kernel.runnable(now)?;
    if runs.is_empty() {
        return Ok(Dispatched::Idle(shared.kernel.next_due()?));
    }
    for run in &runs {
        let snapshot = shared.kernel.snapshot()?;
        // Beak runs the kinds registered alone, each registered with its program.
        if let Some(program) = shared.programs.get(&run.kind) {
            dispatch(&shared.kernel, &snapshot, program.as_ref(), run, now);
        }
    }
    Ok(Dispatched::Ran)
}
