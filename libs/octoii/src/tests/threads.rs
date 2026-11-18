//! Threads on a runtime of their own: made once for a key, their run settings logged where their main process takes
//! them from the inbox — from what each was to what it is — inputs taken in the order they came, how the person keeps
//! them marked on their directory, stopped, deleted with what they delegated, and their log read on a page at a time.

use std::time::Duration;

use crate::threads::config::{Changed, ConfigChange, ConfigChanged};
use crate::threads::log::{self, Entry, Writer};
use crate::threads::{Config, MainInput, MarksChange, Mode, NewThread, Parent, Threads, Turn, inbox};
use beak::Beak;
use bytes::Bytes;
use octoii_ai::vocabulary::{InputContent, ReasoningEffort};
use siphon_runtime::{Ending, Fiber, Options, Run, Runtime, Wait};

/// A main process that takes its inbox into its log as it changes, and does nothing more.
struct Taker;

impl Fiber for Taker {
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

impl Fiber for Idle {
    const KIND: &'static str = "test.idle";
    type Input = MainInput;
    type Checkpoint = ();
    type Output = ();

    fn run(&self, _: &mut Run<'_>, _: MainInput, _: Option<()>) -> anyhow::Result<Ending<(), ()>> {
        Ok(Ending::Wait((), Wait::new().deadline(i64::MAX)))
    }
}

fn threads<M: Fiber<Input = MainInput>>(beak: &Beak, main: M) -> Threads<M> {
    Threads::new(Runtime::builder().fiber(main).start(beak, Options::default()).expect("started"))
}

fn person(config: Config) -> NewThread {
    NewThread {
        actor: "person".into(),
        config,
        ..NewThread::default()
    }
}

/// Commits a real thread plan so these tests exercise the same primitive boundary as tool calls.
struct ApplyPlan;
impl Fiber for ApplyPlan {
    const KIND: &'static str = "test.thread_plan";
    type Input = (String, crate::tools::Plan);
    type Checkpoint = ();
    type Output = Result<(), String>;
    fn run(&self, run: &mut Run<'_>, (own, plan): Self::Input, _: Option<()>) -> anyhow::Result<Ending<(), Self::Output>> {
        use crate::tools::Primitives as _;
        Ok(Ending::Done(crate::agent::ThreadPrimitives.apply(run, &own, "organize", &plan)?))
    }
}

fn organizing_threads(beak: &Beak, options: Options) -> Threads<Taker> {
    Threads::new(Runtime::builder().fiber(Taker).fiber(ApplyPlan).start(beak, options).unwrap())
}

async fn applied(runtime: &Runtime, own: &str, plan: crate::tools::Plan) -> Result<(), String> {
    let key = runtime.new_id().unwrap();
    let process = beak::layout::process(b"", key.as_bytes());
    let mut changes = siphon_runtime::Changes::new();
    changes.start::<ApplyPlan>(Bytes::new(), &key, &(own.to_owned(), plan)).unwrap();
    runtime.commit(changes).await.unwrap();
    let mut committed = runtime.committed();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(outcome) = runtime
                .snapshot()
                .unwrap()
                .record(&process)
                .unwrap()
                .and_then(|record| record.outcome)
            {
                let beak::Outcome::Completed(Some(result)) = outcome else {
                    panic!("the plan failed to run: {outcome:?}")
                };
                return serde_json::from_slice(&result).unwrap();
            }
            committed.changed().await.unwrap();
        }
    })
    .await
    .expect("the plan ended")
}

async fn inbox_empty(threads: &Threads<Taker>, id: &str) {
    let mut committed = threads.runtime().committed();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !threads.listed(id).unwrap().status.inbox.is_empty() {
            committed.changed().await.unwrap();
        }
    })
    .await
    .expect("the title request was taken");
}

/// Establishes an absent main process for revival tests, without waiting for graceful termination.
async fn force_remove_main(runtime: &Runtime, id: &str) {
    let mut changes = siphon_runtime::Changes::new();
    changes.force_remove(crate::threads::thread::main(id), None);
    runtime.commit(changes).await.unwrap();
    let mut committed = runtime.committed();
    tokio::time::timeout(Duration::from_secs(10), async {
        while runtime
            .snapshot()
            .unwrap()
            .record(&crate::threads::thread::main(id))
            .unwrap()
            .is_some()
        {
            committed.changed().await.unwrap();
        }
    })
    .await
    .unwrap_or_else(|error| {
        let snapshot = runtime.snapshot().unwrap();
        panic!(
            "the main process was not removed: {error}; record={:?}; directory={:?}",
            snapshot.record(&crate::threads::thread::main(id)).unwrap(),
            snapshot.stat(&crate::threads::thread::main(id)).unwrap()
        );
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn organizing_plans_reject_higher_mode_missing_targets_and_empty_titles_atomically() {
    use crate::tools::{Plan, Step};
    let beak = Beak::new().unwrap();
    let threads = organizing_threads(&beak, Options::default());
    let own = threads.create(&person(Config::default()), "own").await.unwrap();
    let high = threads
        .create(
            &person(Config {
                mode: Some(Mode::FullAccess),
                ..Config::default()
            }),
            "high",
        )
        .await
        .unwrap();
    let volume = crate::volume::Volume {
        runtime: threads.runtime().clone(),
    };
    let mut rename_high = Plan::default();
    rename_high.mark(
        &own,
        MarksChange {
            pinned: Some(true),
            ..MarksChange::default()
        },
    );
    rename_high.rename(&volume, &high, "Higher").unwrap();
    let mut mark_high = Plan::default();
    mark_high.rename(&volume, &own, "Own").unwrap();
    mark_high.mark(
        &high,
        MarksChange {
            archived: Some(true),
            ..MarksChange::default()
        },
    );
    let mut missing = Plan::default();
    missing.mark(
        &own,
        MarksChange {
            pinned: Some(true),
            ..MarksChange::default()
        },
    );
    missing.mark(
        "missing",
        MarksChange {
            pinned: Some(true),
            ..MarksChange::default()
        },
    );
    let invalid_title = Plan {
        steps: vec![
            Step::Mark {
                thread: own.clone(),
                change: MarksChange {
                    pinned: Some(true),
                    ..MarksChange::default()
                },
            },
            Step::Rename {
                thread: own.clone(),
                delivery: volume.new_id().unwrap(),
                title: " \n\t".into(),
            },
        ],
        awaits: None,
    };
    for plan in [rename_high, mark_high, missing, invalid_title] {
        assert!(applied(threads.runtime(), &own, plan).await.is_err());
        let row = threads.listed(&own).unwrap();
        assert!(!row.marks.pinned);
        assert_eq!(row.config.title, None);
        assert_eq!(row.status.inbox, []);
    }
    threads.runtime().close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn sub_threads_cannot_apply_organizing_plans() {
    use crate::tools::Plan;
    let beak = Beak::new().unwrap();
    let threads = organizing_threads(&beak, Options::default());
    let own = threads.create(&person(Config::default()), "own").await.unwrap();
    for (index, kind) in [
        crate::threads::SubThreadKind::Explorer,
        crate::threads::SubThreadKind::GeneralPurpose,
    ]
    .into_iter()
    .enumerate()
    {
        let child = threads
            .create(
                &NewThread {
                    parent: Some(Parent {
                        thread: own.clone(),
                        call: "call".into(),
                        kind,
                    }),
                    ..person(Config::default())
                },
                &format!("child-{index}"),
            )
            .await
            .unwrap();
        let mut plan = Plan::default();
        plan.mark(
            &own,
            MarksChange {
                pinned: Some(true),
                ..MarksChange::default()
            },
        );
        assert!(
            applied(threads.runtime(), &child, plan)
                .await
                .unwrap_err()
                .contains("a sub-thread works on its task alone")
        );
        assert!(!threads.listed(&own).unwrap().marks.pinned);
    }
    threads.runtime().close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn marks_do_not_revive_or_stop_work_and_snoozes_follow_plan_order_and_expiry() {
    use crate::tools::Plan;
    let beak = Beak::new().unwrap();
    let threads = organizing_threads(&beak, Options::default());
    let own = threads.create(&person(Config::default()), "own").await.unwrap();
    let target = threads.create(&person(Config::default()), "target").await.unwrap();
    force_remove_main(threads.runtime(), &target).await;
    let until = threads.runtime().now_ms() + 60_000;
    let mut plan = Plan::default();
    let change = MarksChange {
        pinned: Some(true),
        archived: Some(true),
        snoozed_until: Some(until),
        ..MarksChange::default()
    };
    plan.mark(&own, change.clone());
    plan.mark(&target, change);
    applied(threads.runtime(), &own, plan.clone()).await.unwrap();
    applied(threads.runtime(), &own, plan).await.unwrap();
    assert!(
        threads
            .runtime()
            .snapshot()
            .unwrap()
            .record(&crate::threads::thread::main(&target))
            .unwrap()
            .is_none()
    );
    assert!(
        threads
            .runtime()
            .snapshot()
            .unwrap()
            .record(&crate::threads::thread::main(&own))
            .unwrap()
            .unwrap()
            .outcome
            .is_none()
    );
    assert_eq!(
        entries(&threads, &own, |_| true).await.len(),
        1,
        "marks add no conversation entries"
    );
    let mut clear = Plan::default();
    clear.mark(
        &target,
        MarksChange {
            snoozed_until: Some(0),
            ..MarksChange::default()
        },
    );
    clear.mark(
        &target,
        MarksChange {
            snoozed_until: Some(until),
            ..MarksChange::default()
        },
    );
    clear.mark(
        &target,
        MarksChange {
            snoozed_until: Some(0),
            ..MarksChange::default()
        },
    );
    applied(threads.runtime(), &own, clear).await.unwrap();
    assert_eq!(threads.listed(&target).unwrap().marks.snoozed_until, None);
    let mut expired = Plan::default();
    expired.mark(
        &own,
        MarksChange {
            pinned: Some(false),
            ..MarksChange::default()
        },
    );
    expired.mark(
        &own,
        MarksChange {
            snoozed_until: Some(threads.runtime().now_ms() + 5),
            ..MarksChange::default()
        },
    );
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(applied(threads.runtime(), &own, expired).await.unwrap_err().contains("future"));
    let own = threads.listed(&own).unwrap();
    assert!(own.marks.pinned);
    assert_eq!(own.marks.snoozed_until, Some(until));
    threads.runtime().close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_own_title_request_and_repeated_marks_apply_once_and_survive_reopening() {
    use crate::tools::Plan;
    let beak = Beak::new().unwrap();
    let storage = tempfile::tempdir().unwrap();
    let options = Options {
        directory: Some(storage.path().to_owned()),
        ..Options::default()
    };
    let first = organizing_threads(&beak, options.clone());
    let own = first.create(&person(Config::default()), "own").await.unwrap();
    let volume = crate::volume::Volume {
        runtime: first.runtime().clone(),
    };
    let until = first.runtime().now_ms() + 60_000;
    let mut plan = Plan::default();
    plan.rename(&volume, &own, "  Renamed  ").unwrap();
    plan.mark(
        &own,
        MarksChange {
            pinned: Some(true),
            archived: Some(true),
            snoozed_until: Some(until),
            ..MarksChange::default()
        },
    );
    applied(first.runtime(), &own, plan.clone()).await.unwrap();
    inbox_empty(&first, &own).await;
    assert_eq!(first.listed(&own).unwrap().config.title.as_deref(), Some("Renamed"));
    applied(first.runtime(), &own, plan).await.unwrap();
    inbox_empty(&first, &own).await;
    let log = entries(&first, &own, |_| true).await;
    assert_eq!(log.len(), 2, "the repeated title request adds no second configuration change");
    assert!(log.iter().all(|entry| matches!(entry, Entry::Config { .. })));
    first.runtime().close().await;
    drop(volume);
    drop(first);
    let reopened = organizing_threads(&beak, options);
    let row = reopened.listed(&own).unwrap();
    assert_eq!(row.config.title.as_deref(), Some("Renamed"));
    assert!(row.marks.pinned && row.marks.archived);
    assert_eq!(row.marks.snoozed_until, Some(until));
    reopened.runtime().close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn rename_and_deliver_revive_a_target_once_and_removing_targets_are_refused() {
    use crate::tools::{Plan, Report};
    let beak = Beak::new().unwrap();
    let threads = organizing_threads(&beak, Options::default());
    let own = threads.create(&person(Config::default()), "own").await.unwrap();
    let target = threads.create(&person(Config::default()), "target").await.unwrap();
    force_remove_main(threads.runtime(), &target).await;
    let volume = crate::volume::Volume {
        runtime: threads.runtime().clone(),
    };
    let mut plan = Plan::default();
    plan.rename(&volume, &target, "First").unwrap();
    plan.rename(&volume, &target, "Second").unwrap();
    plan.deliver(
        &volume,
        &target,
        vec![InputContent::text("Hello")],
        inbox::Joins::Step,
        Report::None,
    )
    .unwrap();
    applied(threads.runtime(), &own, plan).await.unwrap();
    let row = threads.listed(&target).unwrap();
    assert_eq!(
        row.status.inbox.len(),
        3,
        "the revived agent fiber is not registered in this test runtime"
    );
    let main = threads
        .runtime()
        .snapshot()
        .unwrap()
        .record(&crate::threads::thread::main(&target))
        .unwrap()
        .unwrap();
    let mut changes = siphon_runtime::Changes::new();
    changes.meta(
        crate::threads::thread::path(&target),
        vec![(Bytes::from_static(beak::layout::REMOVING_LABEL), Some(Bytes::from_static(b"true")))],
        Vec::new(),
    );
    threads.runtime().commit(changes).await.unwrap();
    for plan in [
        {
            let mut plan = Plan::default();
            plan.rename(&volume, &target, "Forbidden").unwrap();
            plan
        },
        {
            let mut plan = Plan::default();
            plan.mark(
                &target,
                MarksChange {
                    pinned: Some(true),
                    ..MarksChange::default()
                },
            );
            plan
        },
    ] {
        assert_eq!(
            applied(threads.runtime(), &own, plan).await.unwrap_err(),
            format!("no thread {target}")
        );
        assert_eq!(inbox::waiting(&threads.runtime().snapshot().unwrap(), &target).unwrap().len(), 3);
    }
    let after = threads
        .runtime()
        .snapshot()
        .unwrap()
        .record(&crate::threads::thread::main(&target))
        .unwrap()
        .unwrap();
    assert_eq!((after.created_seq, after.outcome), (main.created_seq, main.outcome));
    threads.runtime().close().await;
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
        _ => None,
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
        Err(crate::threads::Error::Idempotency(crate::threads::idempotency::Error::Reused))
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
            kind: crate::threads::SubThreadKind::GeneralPurpose,
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
                    kind: crate::threads::SubThreadKind::GeneralPurpose,
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
    assert!(matches!(threads.listed(&parent), Err(crate::threads::Error::NoSuchThread)));
    threads.runtime().close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn sub_thread_kinds_survive_configuration_changes_and_reopening() {
    let beak = Beak::new().unwrap();
    let storage = tempfile::tempdir().unwrap();
    let options = Options {
        directory: Some(storage.path().to_owned()),
        ..Options::default()
    };
    let first = Threads::<Taker>::new(Runtime::builder().fiber(Taker).start(&beak, options.clone()).unwrap());
    let mut children = Vec::new();
    for kind in [
        crate::threads::SubThreadKind::Explorer,
        crate::threads::SubThreadKind::GeneralPurpose,
    ] {
        let new = NewThread {
            parent: Some(Parent {
                thread: "parent".into(),
                call: "call".into(),
                kind,
            }),
            ..person(Config::default())
        };
        let id = first.create(&new, &format!("{kind:?}")).await.unwrap();
        first
            .configure(
                &id,
                "person",
                ConfigChange {
                    mode: Some(Some(Mode::FullAccess)),
                    title: Some(Some("changed".into())),
                    ..ConfigChange::default()
                },
            )
            .await
            .unwrap();
        entries(&first, &id, |entries| {
            entries.iter().filter(|entry| matches!(entry, Entry::Config { .. })).count() == 2
        })
        .await;
        assert_eq!(first.listed(&id).unwrap().config.mode, Some(Mode::FullAccess));
        assert_eq!(first.listed(&id).unwrap().parent.unwrap().kind, kind);
        children.push((id, kind));
    }
    first.runtime().close().await;
    drop(first);
    let reopened = Threads::<Taker>::new(Runtime::builder().fiber(Taker).start(&beak, options).unwrap());
    let volume = crate::volume::Volume {
        runtime: reopened.runtime().clone(),
    };
    for (id, kind) in children {
        let parent = crate::agent::parent_of(&volume, &id).unwrap().unwrap();
        assert_eq!(parent.kind, kind);
        assert_eq!(parent.thread, "parent");
        assert_eq!(reopened.listed(&id).unwrap().parent, Some(parent));
    }
    reopened.runtime().close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn legacy_sub_threads_are_general_purpose_and_invalid_kinds_are_refused() {
    let beak = Beak::new().unwrap();
    let threads = threads(&beak, Idle);
    let new = NewThread {
        parent: Some(Parent {
            thread: "parent".into(),
            call: "call".into(),
            kind: crate::threads::SubThreadKind::Explorer,
        }),
        ..person(Config::default())
    };
    let id = threads.create(&new, "child").await.unwrap();
    let annotation = Bytes::from_static(crate::threads::thread::SUB_THREAD_KIND_ANNOTATION.as_bytes());
    let mut changes = siphon_runtime::Changes::new();
    changes.meta(crate::threads::thread::path(&id), Vec::new(), vec![(annotation.clone(), None)]);
    threads.runtime().commit(changes).await.unwrap();
    assert_eq!(
        threads.listed(&id).unwrap().parent.unwrap().kind,
        crate::threads::SubThreadKind::GeneralPurpose
    );
    let mut changes = siphon_runtime::Changes::new();
    changes.meta(
        crate::threads::thread::path(&id),
        Vec::new(),
        vec![(annotation, Some(Bytes::from_static(b"\"invalid\"")))],
    );
    threads.runtime().commit(changes).await.unwrap();
    assert!(threads.listed(&id).is_err());
    let volume = crate::volume::Volume {
        runtime: threads.runtime().clone(),
    };
    assert!(crate::agent::parent_of(&volume, &id).is_err());
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
    let back = |before: Option<&str>, limit| {
        let (lines, complete) = log::read_before(&snapshot, &id, before, limit).expect("read");
        let ids: Vec<String> = lines
            .iter()
            .map(|line| log::entry_of(line).expect("an entry").id().to_owned())
            .collect();
        (ids, complete)
    };
    let ids: Vec<String> = whole.iter().map(|entry| entry.id().to_owned()).collect();
    assert_eq!(back(None, 5), (ids[16..].to_vec(), false), "the latest five");
    assert_eq!(back(Some(&ids[16]), 5), (ids[11..16].to_vec(), false), "the five before them");
    assert_eq!(
        back(Some(&ids[3]), 5),
        (ids[..3].to_vec(), true),
        "the first three, the log's start reached"
    );
    assert_eq!(back(None, 100), (ids.clone(), true), "all of it");
    threads.runtime().close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_thread_s_files_are_read_by_their_reference_as_its_conversation_names_them_and_by_no_other_thread() {
    use octoii_ai::host::Files as _;
    let beak = Beak::new().expect("Beak");
    let threads = threads(&beak, Taker);
    let id = threads.create(&person(Config::default()), "k1").await.expect("made");
    let other = threads.create(&person(Config::default()), "k2").await.expect("made");
    let runtime = threads.runtime();
    let mut changes = siphon_runtime::Changes::new();
    let reference = crate::threads::files::keep(
        &runtime.snapshot().expect("a snapshot"),
        &mut changes,
        &id,
        Bytes::from_static(b"%PDF"),
    )
    .expect("kept");
    runtime.commit(changes).await.expect("committed");
    let files = crate::threads::files::OfThread {
        runtime: runtime.clone(),
        thread: id,
    };
    assert_eq!(files.read(&reference).expect("read"), Bytes::from_static(b"%PDF"));
    assert!(files.read("sha256:00").is_err(), "a reference of no file kept");
    let elsewhere = crate::threads::files::OfThread {
        runtime: runtime.clone(),
        thread: other,
    };
    assert!(elsewhere.read(&reference).is_err(), "another thread's files are not its own");
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn history_pages_cross_segments_and_hold_the_snapshot_they_opened() {
    let beak = Beak::new().unwrap();
    let threads = threads(&beak, Idle);
    let id = threads.create(&person(Config::default()), "history").await.unwrap();
    let runtime = threads.runtime();
    let mut changes = siphon_runtime::Changes::new();
    changes.delete(log::path(&id));
    changes.mkdir(log::path(&id));
    let entries: Vec<Entry> = (0..150)
        .map(|at| Entry::Input {
            id: format!("{at:03}"),
            actor: "person".to_owned(),
            delivery: format!("delivery-{at}"),
            content: vec![InputContent::text(format!(
                "message {at} {}",
                "x".repeat(if at == 75 { 100_000 } else { 4096 })
            ))],
            reply_to: None,
        })
        .collect();
    for chunk in entries.chunks(40) {
        let mut bytes = Vec::new();
        for entry in chunk {
            bytes.extend(serde_json::to_vec(entry).unwrap());
            bytes.push(b'\n');
        }
        changes.write(log::segment(&id, &format!("{}.jsonl", chunk[0].id())), bytes);
    }
    runtime.commit(changes).await.unwrap();
    let volume = crate::volume::Volume { runtime: runtime.clone() };
    let history = crate::threads::History::open(&volume, &id).unwrap().unwrap();
    assert_eq!(
        history.reverse().unwrap().collect::<anyhow::Result<Vec<_>>>().unwrap(),
        entries.iter().rev().cloned().collect::<Vec<_>>()
    );
    assert_eq!(history.before(None, 60).unwrap(), entries[90..]);
    assert_eq!(history.before(Some("090"), 60).unwrap(), entries[30..90]);
    assert_eq!(history.before(Some("030"), 60).unwrap(), entries[..30]);
    assert_eq!(history.after(Some("029"), 60).unwrap(), entries[30..90]);
    assert_eq!(history.after(Some("089"), 60).unwrap(), entries[90..]);
    assert_eq!(history.after(None, 60).unwrap(), entries[..60]);
    assert_eq!(history.before(Some("000"), 60).unwrap(), Vec::<Entry>::new());
    assert_eq!(history.after(Some("149"), 60).unwrap(), Vec::<Entry>::new());
    assert_eq!(history.before(None, 0).unwrap(), Vec::<Entry>::new());
    assert_eq!(history.after(None, 0).unwrap(), Vec::<Entry>::new());
    assert!(crate::threads::History::open(&volume, "missing").unwrap().is_none());
    let mut changes = siphon_runtime::Changes::new();
    changes.delete(log::path(&id));
    runtime.commit(changes).await.unwrap();
    assert_eq!(history.before(None, 60).unwrap(), entries[90..]);
    assert_eq!(
        crate::threads::History::open(&volume, &id)
            .unwrap()
            .unwrap()
            .before(None, 60)
            .unwrap(),
        Vec::<Entry>::new()
    );
    runtime.close().await;
}

fn startup_request() -> crate::threads::StartRequest {
    crate::threads::StartRequest {
        owner: "test".into(),
        payload: serde_json::json!({"task": "prepare"}),
    }
}

fn startup_volume(beak: &Beak, options: Options) -> crate::volume::Volume {
    crate::volume::Volume {
        runtime: Runtime::builder().fiber(crate::agent::ThreadFiber).start(beak, options).unwrap(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn startup_claim_is_atomic_fenced_and_survives_reopening() {
    use crate::threads::{StartState, StartWorkspace};
    let beak = Beak::new().unwrap();
    let storage = tempfile::tempdir().unwrap();
    let options = Options {
        directory: Some(storage.path().to_owned()),
        ..Options::default()
    };
    let volume = startup_volume(&beak, options.clone());
    let new = person(Config::default());
    let inputs = vec![InputContent::text("keep this")];
    let (a, b) = tokio::join!(
        volume.prepare_thread(
            &new,
            inputs.iter().cloned().map(Into::into).collect(),
            startup_request(),
            "startup-race"
        ),
        volume.prepare_thread(
            &new,
            inputs.iter().cloned().map(Into::into).collect(),
            startup_request(),
            "startup-race"
        )
    );
    let (id, start) = a.unwrap();
    assert_eq!(b.unwrap(), (id.clone(), start.clone()));
    assert_eq!(crate::threads::list(&volume).unwrap().len(), 1);
    assert!(
        volume
            .prepare_thread(&new, vec![InputContent::text("changed").into()], startup_request(), "startup-race")
            .await
            .is_err()
    );
    assert!(
        volume
            .finish_thread_start(
                &id,
                &start.token,
                StartWorkspace {
                    workspace: "relative/path".into(),
                    project: None,
                }
            )
            .await
            .is_err()
    );
    assert_eq!(volume.thread_start(&id).unwrap(), Some(start.clone()));
    let failed = volume.fail_thread_start(&id, &start.token, "checkout failed".into()).await.unwrap();
    assert!(matches!(failed.state, StartState::Failed { .. }));
    volume.runtime.close().await;
    drop(volume);
    let volume = startup_volume(&beak, options);
    assert_eq!(volume.thread_start(&id).unwrap(), Some(failed));
    let listed = crate::threads::list(&volume).unwrap().pop().unwrap();
    assert_eq!(listed.status.inbox.len(), 1);
    assert!(matches!(&listed.status.inbox[0].1, inbox::Inbound::Input { content, .. } if content == &inputs));
    let (a, b) = tokio::join!(
        volume.retry_thread_start(&id, &start.token),
        volume.retry_thread_start(&id, &start.token)
    );
    let retried = a.unwrap();
    assert_eq!(b.unwrap(), retried);
    assert_ne!(retried.token, start.token);
    let directory = StartWorkspace {
        workspace: storage.path().to_string_lossy().into_owned(),
        project: None,
    };
    assert!(volume.finish_thread_start(&id, &start.token, directory.clone()).await.is_err());
    let (a, b) = tokio::join!(
        volume.finish_thread_start(&id, &retried.token, directory.clone()),
        volume.finish_thread_start(&id, &retried.token, directory.clone())
    );
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(volume.thread_config(&id).unwrap().unwrap().workspace, Some(directory.workspace));
    assert!(volume.retry_thread_start(&id, &retried.token).await.is_err());
    volume.runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_or_deleting_preparation_cannot_be_undone_by_a_late_result() {
    use crate::threads::{StartState, StartWorkspace};
    let beak = Beak::new().unwrap();
    let volume = startup_volume(&beak, Options::default());
    let threads = Threads::<crate::agent::ThreadFiber>::new(volume.runtime.clone());
    let new = person(Config::default());
    let (id, start) = volume
        .prepare_thread(&new, vec![InputContent::text("wait").into()], startup_request(), "startup-stop")
        .await
        .unwrap();
    threads.stop(&id, "person").await.unwrap();
    assert!(matches!(volume.thread_start(&id).unwrap().unwrap().state, StartState::Cancelled));
    let directory = StartWorkspace {
        workspace: std::env::temp_dir().to_string_lossy().into_owned(),
        project: None,
    };
    assert!(volume.finish_thread_start(&id, &start.token, directory.clone()).await.is_err());
    assert!(volume.retry_thread_start(&id, &start.token).await.is_err());
    let (tool_stopped, start) = volume
        .prepare_thread(
            &new,
            vec![InputContent::text("tool stop").into()],
            startup_request(),
            "startup-tool-stop",
        )
        .await
        .unwrap();
    let mut changes = siphon_runtime::Changes::new();
    crate::threads::thread::stop_turn(&volume.runtime.snapshot().unwrap(), &mut changes, &tool_stopped).unwrap();
    volume.runtime.commit(changes).await.unwrap();
    assert!(
        volume
            .finish_thread_start(&tool_stopped, &start.token, directory.clone())
            .await
            .is_err()
    );
    assert!(matches!(
        volume.thread_start(&tool_stopped).unwrap().unwrap().state,
        StartState::Cancelled
    ));
    let (deleted, start) = volume
        .prepare_thread(&new, vec![InputContent::text("delete").into()], startup_request(), "startup-delete")
        .await
        .unwrap();
    threads.delete(&deleted).await.unwrap();
    assert!(volume.finish_thread_start(&deleted, &start.token, directory).await.is_err());
    assert!(
        volume
            .prepare_thread(&new, vec![InputContent::text("delete").into()], startup_request(), "startup-delete")
            .await
            .is_err()
    );
    volume.runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn initial_attachments_commit_with_ordered_input_and_survive_reopening() {
    use crate::threads::{InputPart, files};
    let beak = Beak::new().unwrap();
    let storage = tempfile::tempdir().unwrap();
    let options = Options {
        directory: Some(storage.path().to_owned()),
        ..Options::default()
    };
    let volume = startup_volume(&beak, options.clone());
    let mut png = Vec::new();
    image::RgbImage::from_pixel(2048, 2048, image::Rgb([40, 100, 200]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let original = files::reference_of(&png);
    let attachment = InputPart::Attachment {
        bytes: png.into(),
        media_type: "image/png".into(),
        filename: Some("image.png".into()),
    };
    let parts = vec![InputContent::text("before").into(), attachment, InputContent::text("after").into()];
    let new = person(Config::default());
    let (a, b) = tokio::join!(
        volume.prepare_thread(&new, parts.clone(), startup_request(), "attachments"),
        volume.prepare_thread(&new, parts.clone(), startup_request(), "attachments")
    );
    let (id, start) = a.unwrap();
    assert_eq!(b.unwrap(), (id.clone(), start));
    assert_eq!(crate::threads::list(&volume).unwrap().len(), 1);
    let waiting = inbox::waiting(&volume.snapshot().unwrap(), &id).unwrap();
    let inbox::Inbound::Input { content, .. } = &waiting[0].inbound else {
        panic!("initial input")
    };
    assert_eq!(content.first(), Some(&InputContent::text("before")));
    assert_eq!(content.last(), Some(&InputContent::text("after")));
    let InputContent::InputImage { file, .. } = &content[1] else {
        panic!("fitted image")
    };
    assert_ne!(file.reference, original);
    let kept = files::read(&volume.snapshot().unwrap(), &id, &file.reference).unwrap().unwrap();
    assert_eq!(files::reference_of(&kept), file.reference);
    let reference = file.reference.clone();
    volume.runtime.close().await;
    drop(volume);
    let volume = startup_volume(&beak, options);
    assert_eq!(files::read(&volume.snapshot().unwrap(), &id, &reference).unwrap(), Some(kept));
    assert_eq!(
        volume
            .prepare_thread(&new, parts.clone(), startup_request(), "attachments")
            .await
            .unwrap()
            .0,
        id
    );
    let mut changed = parts;
    if let InputPart::Attachment { filename, .. } = &mut changed[1] {
        *filename = Some("changed.png".into());
    }
    assert!(
        volume
            .prepare_thread(&new, changed, startup_request(), "attachments")
            .await
            .is_err()
    );
    volume.runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_attachment_rolls_back_every_part_and_pure_attachments_are_accepted() {
    use crate::threads::{InputPart, files};
    let beak = Beak::new().unwrap();
    let volume = startup_volume(&beak, Options::default());
    let new = person(Config::default());
    let file = InputPart::Attachment {
        bytes: Bytes::from_static(b"saved"),
        media_type: "text/plain".into(),
        filename: Some("note.txt".into()),
    };
    let invalid = InputPart::Attachment {
        bytes: Bytes::from_static(b"not an image"),
        media_type: "image/png".into(),
        filename: None,
    };
    assert!(
        volume
            .prepare_thread(&new, vec![file.clone(), invalid], startup_request(), "files-only")
            .await
            .is_err()
    );
    assert_eq!(crate::threads::list(&volume).unwrap(), []);
    assert!(volume.snapshot().unwrap().stat(b"siphon.start/files-only").unwrap().is_none());
    let (id, _) = volume
        .prepare_thread(&new, vec![file], startup_request(), "files-only")
        .await
        .unwrap();
    assert_eq!(
        files::read(&volume.snapshot().unwrap(), &id, &files::reference_of(b"saved")).unwrap(),
        Some(Bytes::from_static(b"saved"))
    );
    let changed = InputPart::Attachment {
        bytes: Bytes::from_static(b"different"),
        media_type: "text/plain".into(),
        filename: Some("note.txt".into()),
    };
    assert!(
        volume
            .prepare_thread(&new, vec![changed], startup_request(), "files-only")
            .await
            .is_err()
    );
    volume.runtime.close().await;
}
