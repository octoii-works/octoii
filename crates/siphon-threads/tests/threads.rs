//! Threads on a runtime of their own: made once for a key, their run settings logged where their main process takes
//! them from the inbox — from what each was to what it is — inputs taken in the order they came, how the person keeps
//! them marked on their directory, stopped, deleted with what they delegated, and their log read on a page at a time.

use std::time::Duration;

use beak::Beak;
use octoii_ai::vocabulary::{InputContent, ReasoningEffort};
use siphon_runtime::{Ending, Options, Program, Run, Runtime, Wait};
use siphon_threads::log::{self, Entry, Writer};
use siphon_threads::{Changed, Config, ConfigChange, ConfigChanged, MainInput, MarksChange, Mode, NewThread, Parent, Threads, Turn, inbox};

/// A main process that takes its inbox into its log as it changes, and does nothing more.
struct Taker;

impl Program for Taker {
    const KIND: &'static str = "test.taker";
    type Input = MainInput;
    type Checkpoint = ();
    type Output = ();

    fn run(&self, run: &mut Run<'_>, input: MainInput, _: Option<()>) -> anyhow::Result<Ending<(), ()>> {
        let mut writer = Writer::open(run.snapshot(), &input.thread)?;
        inbox::take(run, &input.thread, &mut writer)?;
        Ok(Ending::Wait((), Wait::new().change(inbox::path(&input.thread))))
    }
}

/// A main process that never takes its inbox.
struct Idle;

impl Program for Idle {
    const KIND: &'static str = "test.idle";
    type Input = MainInput;
    type Checkpoint = ();
    type Output = ();

    fn run(&self, _: &mut Run<'_>, _: MainInput, _: Option<()>) -> anyhow::Result<Ending<(), ()>> {
        Ok(Ending::Wait((), Wait::new().deadline(i64::MAX)))
    }
}

fn threads<M: Program<Input = MainInput>>(beak: &Beak, main: M) -> Threads<M> {
    Threads::new(Runtime::builder().program(main).start(beak, Options::default()).expect("started"))
}

fn person(config: Config) -> NewThread {
    NewThread {
        actor: "person".into(),
        config,
        ..NewThread::default()
    }
}

/// The entries of the log of the thread `id`, once `done` holds of them.
async fn entries<M>(threads: &Threads<M>, id: &str, done: impl Fn(&[Entry]) -> bool) -> Vec<Entry> {
    let runtime = threads.runtime();
    let mut committed = runtime.committed();
    let waiting = async {
        loop {
            let read = log::read_after(&runtime.snapshot().expect("a snapshot"), id, None, u64::MAX).expect("read");
            let entries: Vec<Entry> = read.lines.iter().map(|line| log::entry_of(line).expect("an entry")).collect();
            if done(&entries) {
                return entries;
            }
            committed.changed().await.expect("the runtime runs");
        }
    };
    tokio::time::timeout(Duration::from_secs(10), waiting).await.expect("in time")
}

fn changes_of(entry: &Entry) -> Option<&ConfigChanged> {
    match entry {
        Entry::Config { changed, .. } => Some(changed),
        Entry::Input { .. } => None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_is_made_once_for_its_key_its_log_begun_with_how_it_runs() {
    let beak = Beak::new().expect("Beak");
    let threads = threads(&beak, Idle);
    let config = Config {
        model: Some("openrouter/deepseek".into()),
        title: Some("Plans".into()),
        ..Config::default()
    };
    let id = threads.create(&person(config.clone()), "make-1").await.expect("made");
    assert_eq!(
        threads.create(&person(Config::default()), "make-1").await.expect("made"),
        id,
        "made once for its key"
    );
    let listed = threads.listed(&id).expect("listed");
    assert_eq!((listed.config, listed.status.turn), (config, Turn::Created));
    let log = entries(&threads, &id, |entries| !entries.is_empty()).await;
    let Entry::Config { actor, changed, .. } = &log[0] else {
        panic!("begun with how it runs")
    };
    assert_eq!(actor, "person");
    assert_eq!(
        (changed.model.clone(), changed.mode.clone()),
        (
            Some(Changed {
                from: None,
                to: Some("openrouter/deepseek".into())
            }),
            None
        )
    );
    assert!(matches!(
        threads.deliver(&id, "person", vec![InputContent::text("hi")], "make-1").await,
        Err(siphon_threads::Error::Idempotency(siphon_threads::idempotency::Error::Reused))
    ));
    assert_eq!(threads.list().expect("listed").len(), 1);
    threads.runtime().close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_of_how_it_runs_is_logged_where_its_main_process_takes_it_from_what_it_was_to_what_it_is() {
    let beak = Beak::new().expect("Beak");
    let threads = threads(&beak, Taker);
    let id = threads
        .create(
            &person(Config {
                model: Some("a/one".into()),
                ..Config::default()
            }),
            "k1",
        )
        .await
        .expect("made");
    let change = ConfigChange {
        model: Some(Some("a/two".into())),
        effort: Some(Some(ReasoningEffort::High)),
        mode: Some(Some(Mode::FullAccess)),
        ..ConfigChange::default()
    };
    threads.configure(&id, "person", change).await.expect("configured");
    // The same again changes nothing, and is logged not at all.
    threads
        .configure(
            &id,
            "person",
            ConfigChange {
                model: Some(Some("a/two".into())),
                ..ConfigChange::default()
            },
        )
        .await
        .expect("configured");
    let delivered = threads
        .deliver(&id, "person", vec![InputContent::text("go")], "d1")
        .await
        .expect("delivered");
    let log = entries(&threads, &id, |entries| {
        entries.iter().any(|entry| matches!(entry, Entry::Input { .. }))
    })
    .await;
    assert_eq!(log.len(), 3, "{log:?}");
    let changed = changes_of(&log[1]).expect("a change");
    assert_eq!(
        changed.model,
        Some(Changed {
            from: Some("a/one".into()),
            to: Some("a/two".into())
        })
    );
    assert_eq!(
        changed.mode,
        Some(Changed {
            from: None,
            to: Some(Mode::FullAccess)
        })
    );
    let Entry::Input { content, delivery, .. } = &log[2] else {
        panic!("the input")
    };
    assert_eq!(content, &vec![InputContent::text("go")]);
    assert_eq!(delivery, &delivered, "the input as the inbox named it");
    let config = threads.listed(&id).expect("listed").config;
    assert_eq!(
        (config.model.as_deref(), config.effort),
        (Some("a/two"), Some(ReasoningEffort::High))
    );
    threads
        .configure(
            &id,
            "person",
            ConfigChange {
                effort: Some(None),
                ..ConfigChange::default()
            },
        )
        .await
        .expect("configured");
    let log = entries(&threads, &id, |entries| entries.len() == 4).await;
    assert_eq!(
        changes_of(&log[3]).expect("a change").effort,
        Some(Changed {
            from: Some(ReasoningEffort::High),
            to: None
        })
    );
    threads.runtime().close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn what_waits_in_the_inbox_is_withdrawn_or_stopped_and_how_the_person_keeps_a_thread_is_marked() {
    let beak = Beak::new().expect("Beak");
    let threads = threads(&beak, Idle);
    let id = threads.create(&person(Config::default()), "k1").await.expect("made");
    let first = threads
        .deliver(&id, "person", vec![InputContent::text("one")], "d1")
        .await
        .expect("delivered");
    assert_eq!(
        threads
            .deliver(&id, "person", vec![InputContent::text("one")], "d1")
            .await
            .expect("delivered"),
        first
    );
    threads
        .deliver(&id, "thread.other", vec![InputContent::text("two")], "d2")
        .await
        .expect("delivered");
    threads
        .deliver(&id, "person", vec![InputContent::text("three")], "d3")
        .await
        .expect("delivered");
    assert_eq!(threads.listed(&id).expect("listed").status.inbox.len(), 3);
    assert!(threads.withdraw(&id, &first).await.expect("withdrawn"));
    assert!(!threads.withdraw(&id, &first).await.expect("withdrawn"), "gone already");
    threads.stop(&id, "person").await.expect("stopped");
    let inbox = threads.listed(&id).expect("listed").status.inbox;
    assert_eq!(inbox.len(), 1, "the person's withdrawn, another's kept");
    let now = threads.runtime().now_ms();
    let marks = MarksChange {
        pinned: Some(true),
        snoozed_until: Some(now + 60_000),
        seen_until: Some(now),
        ..MarksChange::default()
    };
    threads.mark(&id, &marks).await.expect("marked");
    let kept = threads.listed(&id).expect("listed").marks;
    assert_eq!(
        (kept.pinned, kept.archived, kept.snoozed_until, kept.seen_until),
        (true, false, Some(now + 60_000), Some(now))
    );
    threads
        .mark(
            &id,
            &MarksChange {
                pinned: Some(false),
                snoozed_until: Some(now - 1),
                ..MarksChange::default()
            },
        )
        .await
        .expect("marked");
    let kept = threads.listed(&id).expect("listed").marks;
    assert_eq!((kept.pinned, kept.snoozed_until), (false, None), "unpinned and woken");
    threads.runtime().close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_is_deleted_with_every_thread_under_it() {
    let beak = Beak::new().expect("Beak");
    let threads = threads(&beak, Idle);
    let parent = threads.create(&person(Config::default()), "k1").await.expect("made");
    let sub = NewThread {
        parent: Some(Parent {
            thread: parent.clone(),
            call: "call-1".into(),
        }),
        ..person(Config::default())
    };
    let child = threads.create(&sub, "k2").await.expect("made");
    let grandchild = threads
        .create(
            &NewThread {
                parent: Some(Parent {
                    thread: child.clone(),
                    call: "call-2".into(),
                }),
                ..person(Config::default())
            },
            "k3",
        )
        .await
        .expect("made");
    let other = threads.create(&person(Config::default()), "k4").await.expect("made");
    assert_eq!(
        threads.listed(&child).expect("listed").parent.map(|parent| parent.call),
        Some("call-1".into())
    );
    let gone = threads.delete(&parent).await.expect("deleted");
    assert_eq!(gone, [parent.clone(), child, grandchild]);
    let left: Vec<String> = threads.list().expect("listed").into_iter().map(|listed| listed.id).collect();
    assert_eq!(left, [other]);
    assert!(matches!(threads.listed(&parent), Err(siphon_threads::Error::NoSuchThread)));
    threads.runtime().close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_log_is_read_on_a_page_at_a_time_each_line_once() {
    let beak = Beak::new().expect("Beak");
    let threads = threads(&beak, Taker);
    let id = threads.create(&person(Config::default()), "k1").await.expect("made");
    for round in 0..20 {
        threads
            .deliver(
                &id,
                "person",
                vec![InputContent::text(format!("input {round}"))],
                &format!("d{round}"),
            )
            .await
            .expect("delivered");
    }
    let whole = entries(&threads, &id, |entries| entries.len() == 21).await;
    let snapshot = threads.runtime().snapshot().expect("a snapshot");
    let mut read = Vec::new();
    let mut cursor = None;
    loop {
        let page = log::read_after(&snapshot, &id, cursor.as_ref(), 200).expect("read");
        assert!(!page.lines.is_empty() || !page.more);
        read.extend(page.lines.iter().map(|line| log::entry_of(line).expect("an entry")));
        cursor = page.cursor;
        if !page.more {
            break;
        }
    }
    assert_eq!(read, whole);
    let after = log::read_after(&snapshot, &id, cursor.as_ref(), 200).expect("read");
    assert!(after.lines.is_empty() && !after.more, "nothing past the end");
    threads.runtime().close().await;
}
