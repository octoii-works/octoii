//! What is kept running by its key: started once for all who ask while it starts, given to each who asks after,
//! started anew where it ended or could not start, let go of once nothing asked for it over its idle time.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use siphon::{Ends, Kept};

/// A thing kept: its number among those started, and whether it ended.
#[derive(Debug, Clone)]
struct Running {
    number: usize,
    ended: Arc<AtomicBool>,
}

impl Ends for Running {
    fn ended(&self) -> bool {
        self.ended.load(Ordering::SeqCst)
    }
}

/// A start of the next thing, counted by `starts`, done after a moment.
fn start(starts: &Arc<AtomicUsize>) -> impl Future<Output = Result<Running, String>> + Send + 'static {
    let starts = Arc::clone(starts);
    async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok(Running {
            number: starts.fetch_add(1, Ordering::SeqCst),
            ended: Arc::new(AtomicBool::new(false)),
        })
    }
}

#[tokio::test(start_paused = true)]
async fn what_is_kept_is_started_once_given_after_started_anew_where_it_ended_and_let_go_of_when_idle() {
    let kept: Kept<&str, Running, String> = Kept::new(Duration::from_secs(60));
    let starts = Arc::new(AtomicUsize::new(0));
    let (first, second) = tokio::join!(kept.get("a", start(&starts)), kept.get("a", start(&starts)));
    let (first, second) = (first.expect("started"), second.expect("started"));
    assert_eq!(
        (first.number, second.number),
        (0, 0),
        "who asks while it starts waits on the one start"
    );
    assert_eq!(kept.get("a", start(&starts)).await.expect("kept").number, 0);
    assert_eq!(
        kept.held(vec!["a", "b"])
            .await
            .iter()
            .map(|held| held.as_ref().map(|running| running.number))
            .collect::<Vec<_>>(),
        [Some(0), None],
        "what is held is told, nothing started"
    );
    assert_eq!(starts.load(Ordering::SeqCst), 1);

    first.ended.store(true, Ordering::SeqCst);
    assert_eq!(kept.held(vec!["a"]).await[0].as_ref().map(|running| running.number), None);
    assert_eq!(kept.get("a", start(&starts)).await.expect("started anew").number, 1);

    let failing = kept.get("b", async { Err::<Running, _>("refused".to_owned()) }).await;
    assert_eq!(failing.expect_err("could not start"), "refused");
    assert_eq!(kept.get("b", start(&starts)).await.expect("started this time").number, 2);

    tokio::time::sleep(Duration::from_secs(50)).await;
    assert_eq!(
        kept.get("a", start(&starts)).await.expect("kept").number,
        1,
        "asked for, its idle time begins again"
    );
    tokio::time::sleep(Duration::from_secs(70)).await;
    assert_eq!(
        kept.held(vec!["a", "b"]).await.iter().filter(|held| held.is_some()).count(),
        0,
        "let go of once idle"
    );
    assert_eq!(kept.get("a", start(&starts)).await.expect("started anew").number, 3);
}
