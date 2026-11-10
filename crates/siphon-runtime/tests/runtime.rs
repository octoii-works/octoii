//! The runtime on a kernel of its own: processes run a phase at a time, the requests they make answered once — by
//! their handler, for a handler that failed, at once under a stop, interrupted as the volume opens again, by a person
//! whose answer fits — what streams followed by topic, waits on a deadline and on children, and the volume collected.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use beak::{Beak, Outcome, layout};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use siphon_runtime::{
    Answer, Asked, AskedOfPerson, Builder, Changes, Clock, Collection, Ending, NotGiven, Options, Program, Request, Run, Runtime,
    Unanswered, Wait,
};

/// Asks for its input in capitals.
#[derive(Debug, Serialize, Deserialize)]
struct Shout {
    text: String,
}

impl Request for Shout {
    const NAME: &'static str = "test.shout";
    type Answer = String;
}

/// What an asker ended with: its answer as it came.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Said {
    Given(String),
    Failed(String),
    Interrupted,
}

impl From<Answer<String>> for Said {
    fn from(answer: Answer<String>) -> Self {
        match answer {
            Answer::Given(given) => Self::Given(given),
            Answer::Failed(why) => Self::Failed(why),
            Answer::Interrupted => Self::Interrupted,
        }
    }
}

/// Asks `Shout` its input, keyed `k1`, and ends with the answer; its stop handler waits for the answer too, and ends
/// stopped with it.
struct Asker;

impl Program for Asker {
    const KIND: &'static str = "test.asker";
    const STOP_HANDLER: bool = true;
    type Input = String;
    type Checkpoint = ();
    type Output = Said;

    fn run(&self, run: &mut Run<'_>, input: String, checkpoint: Option<()>) -> anyhow::Result<Ending<(), Said>> {
        if checkpoint.is_none() && !run.is_stop_handler() {
            run.request("k1", &Shout { text: input })?;
            return Ok(Ending::Wait((), Wait::new().mail("k1")));
        }
        let Some(answer) = run.answer::<Shout>("k1")? else {
            return Ok(Ending::Wait((), Wait::new().mail("k1")));
        };
        run.take_mail("k1");
        let said = Said::from(answer);
        Ok(if run.is_stop_handler() {
            Ending::Stopped(Some(said))
        } else {
            Ending::Done(said)
        })
    }
}

fn beak() -> Beak {
    Beak::new().expect("Beak")
}

fn memory() -> Options {
    Options::default()
}

/// Starts a process of `P` at the root keyed `key`: its path.
async fn start<P: Program>(runtime: &Runtime, key: &str, input: &P::Input) -> Bytes {
    let mut changes = Changes::new();
    changes.start::<P>(Bytes::new(), key, input).expect("an input");
    runtime.commit(changes).await.expect("started");
    layout::process(b"", key.as_bytes())
}

/// How the process at `path` ended, once it has.
async fn ended(runtime: &Runtime, path: &[u8]) -> Outcome {
    let mut committed = runtime.committed();
    let waiting = async {
        loop {
            let record = runtime.snapshot().expect("a snapshot").record(path).expect("a record");
            if let Some(outcome) = record.and_then(|record| record.outcome) {
                return outcome;
            }
            committed.changed().await.expect("the runtime runs");
        }
    };
    tokio::time::timeout(Duration::from_secs(10), waiting)
        .await
        .expect("it ends in time")
}

fn said(outcome: &Outcome) -> Said {
    match outcome {
        Outcome::Completed(Some(result)) | Outcome::Stopped { result: Some(result), .. } => {
            serde_json::from_slice(result).expect("what it said")
        }
        other => panic!("it ended {other:?}"),
    }
}

fn shouting(builder: Builder) -> Builder {
    builder
        .program(Asker)
        .handle(|asked: Asked<Shout>| async move { Ok(asked.payload.text.to_uppercase()) })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_is_answered_by_its_handler_and_its_process_goes_on_from_the_answer() {
    let beak = beak();
    let runtime = shouting(Runtime::builder()).start(&beak, memory()).expect("started");
    let path = start::<Asker>(&runtime, "a1", &"hello".into()).await;
    assert_eq!(said(&ended(&runtime, &path).await), Said::Given("HELLO".into()));
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_handler_that_fails_or_panics_is_answered_for() {
    let beak = beak();
    let runtime = Runtime::builder()
        .program(Asker)
        .handle(|asked: Asked<Shout>| async move {
            assert!(asked.payload.text != "panic", "the handler panics");
            Err::<String, _>(Unanswered::Failed("broken".into()))
        })
        .start(&beak, memory())
        .expect("started");
    let failing = start::<Asker>(&runtime, "a1", &"hello".into()).await;
    assert_eq!(said(&ended(&runtime, &failing).await), Said::Failed("broken".into()));
    let panicking = start::<Asker>(&runtime, "a2", &"panic".into()).await;
    let Said::Failed(why) = said(&ended(&runtime, &panicking).await) else {
        panic!("answered as failed")
    };
    assert!(why.starts_with("the handler panicked"), "{why}");
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stop_cuts_a_request_off_and_its_handler_answers_with_what_has_come() {
    let beak = beak();
    let began = Arc::new(tokio::sync::Notify::new());
    let told = Arc::clone(&began);
    let runtime = Runtime::builder()
        .program(Asker)
        .handle(move |asked: Asked<Shout>| {
            let told = Arc::clone(&told);
            async move {
                told.notify_one();
                asked.cancelled.cancelled().await;
                Ok("partial".into())
            }
        })
        .start(&beak, memory())
        .expect("started");
    let path = start::<Asker>(&runtime, "a1", &"hello".into()).await;
    began.notified().await;
    let mut changes = Changes::new();
    changes.signal(path.clone(), beak::Signal::Terminate, None);
    runtime.commit(changes).await.expect("signalled");
    let outcome = ended(&runtime, &path).await;
    assert!(matches!(outcome, Outcome::Stopped { .. }), "{outcome:?}");
    assert_eq!(said(&outcome), Said::Given("partial".into()));
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_left_unanswered_as_the_runtime_closed_comes_back_interrupted_as_it_opens_again() {
    let directory = tempfile::tempdir().expect("a directory");
    let options = Options {
        directory: Some(directory.path().to_owned()),
        ..Options::default()
    };
    let beak = beak();
    let began = Arc::new(tokio::sync::Notify::new());
    let told = Arc::clone(&began);
    let runtime = Runtime::builder()
        .program(Asker)
        .handle(move |_: Asked<Shout>| {
            let told = Arc::clone(&told);
            async move {
                told.notify_one();
                std::future::pending::<Result<String, Unanswered>>().await
            }
        })
        .start(&beak, options.clone())
        .expect("started");
    let path = start::<Asker>(&runtime, "a1", &"hello".into()).await;
    began.notified().await;
    runtime.close().await;
    drop(runtime);
    let asked_again = Arc::new(AtomicBool::new(false));
    let noted = Arc::clone(&asked_again);
    let runtime = Runtime::builder()
        .program(Asker)
        .handle(move |_: Asked<Shout>| {
            noted.store(true, Ordering::SeqCst);
            async move { Ok("again".into()) }
        })
        .start(&beak, options)
        .expect("opened again");
    assert_eq!(said(&ended(&runtime, &path).await), Said::Interrupted);
    assert!(!asked_again.load(Ordering::SeqCst), "nothing is acted on twice");
    runtime.close().await;
}

/// Asks a person to choose one of its options.
#[derive(Debug, Serialize, Deserialize)]
struct Choice {
    options: Vec<String>,
}

impl Request for Choice {
    const NAME: &'static str = "test.choice";
    type Answer = String;
}

impl AskedOfPerson for Choice {
    fn answer_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "string", "enum": self.options })
    }
}

/// Asks a person to choose, and ends with the choice.
struct Chooser;

impl Program for Chooser {
    const KIND: &'static str = "test.chooser";
    type Input = Vec<String>;
    type Checkpoint = ();
    type Output = Said;

    fn run(&self, run: &mut Run<'_>, options: Vec<String>, checkpoint: Option<()>) -> anyhow::Result<Ending<(), Said>> {
        if checkpoint.is_none() {
            run.request("c1", &Choice { options })?;
            return Ok(Ending::Wait((), Wait::new().mail("c1")));
        }
        Ok(match run.answer::<Choice>("c1")? {
            Some(answer) => Ending::Done(Said::from(answer)),
            None => Ending::Wait((), Wait::new().mail("c1")),
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_answers_with_what_fits_what_the_request_asks() {
    let beak = beak();
    let runtime = Runtime::builder()
        .program(Chooser)
        .ask_person::<Choice>()
        .start(&beak, memory())
        .expect("started");
    let path = start::<Chooser>(&runtime, "c", &vec!["a".into(), "b".into()]).await;
    let waiting = async {
        let mut committed = runtime.committed();
        loop {
            match runtime.answer_schema(&path, b"c1") {
                Ok(schema) => return schema,
                Err(NotGiven::NoSuchRequest) => committed.changed().await.expect("the runtime runs"),
                Err(error) => panic!("{error}"),
            }
        }
    };
    let schema = tokio::time::timeout(Duration::from_secs(10), waiting).await.expect("asked in time");
    assert_eq!(schema, serde_json::json!({ "type": "string", "enum": ["a", "b"] }));
    let unfit = runtime.answer(&path, b"c1", &serde_json::json!("c")).await;
    assert!(matches!(unfit, Err(NotGiven::Unfit(_))), "{unfit:?}");
    runtime.answer(&path, b"c1", &serde_json::json!("b")).await.expect("given");
    assert_eq!(said(&ended(&runtime, &path).await), Said::Given("b".into()));
    assert!(matches!(
        runtime.answer(&path, b"c1", &serde_json::json!("a")).await,
        Err(NotGiven::NoSuchRequest)
    ));
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn what_streams_while_a_request_is_answered_is_followed_by_its_topic_each_event_once() {
    let beak = beak();
    let release = Arc::new(tokio::sync::Notify::new());
    let released = Arc::clone(&release);
    let runtime = Runtime::builder()
        .program(Asker)
        .handle(move |asked: Asked<Shout>| {
            let released = Arc::clone(&released);
            async move {
                asked.publish("t1", Bytes::from_static(b"a"));
                asked.publish("other", Bytes::from_static(b"x"));
                asked.publish("t1", Bytes::from_static(b"b"));
                released.notified().await;
                asked.publish("t1", Bytes::from_static(b"c"));
                Ok("done".into())
            }
        })
        .start(&beak, memory())
        .expect("started");
    let path = start::<Asker>(&runtime, "a1", &"hello".into()).await;
    let mut following = runtime.follow("t1").await;
    let mut seen = Vec::new();
    while seen.len() < 2 {
        let event = tokio::time::timeout(Duration::from_secs(10), following.next())
            .await
            .expect("in time")
            .expect("an event");
        seen.push((event.offset, event.data));
    }
    // A follower that comes later is given what was kept, from the stream's start.
    let mut late = runtime.follow("t1").await;
    let first = late.next().await.expect("an event");
    assert_eq!((first.offset, &first.data[..], &*first.key), (0, &b"a"[..], "k1"));
    release.notify_one();
    let event = tokio::time::timeout(Duration::from_secs(10), following.next())
        .await
        .expect("in time")
        .expect("an event");
    seen.push((event.offset, event.data));
    assert_eq!(
        seen,
        [
            (0, Bytes::from_static(b"a")),
            (2, Bytes::from_static(b"b")),
            (3, Bytes::from_static(b"c"))
        ]
    );
    assert_eq!(said(&ended(&runtime, &path).await), Said::Given("done".into()));
    runtime.close().await;
}

/// A clock the test sets.
#[derive(Debug, Default)]
struct Set(AtomicI64);

impl Clock for Set {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Sleeps till a second after it first ran, then ends with the time it woke at.
struct Sleeper;

impl Program for Sleeper {
    const KIND: &'static str = "test.sleeper";
    type Input = ();
    type Checkpoint = i64;
    type Output = i64;

    fn run(&self, run: &mut Run<'_>, (): (), checkpoint: Option<i64>) -> anyhow::Result<Ending<i64, i64>> {
        Ok(match checkpoint {
            None => {
                let at = run.time_ms() + 1_000;
                Ending::Wait(at, Wait::new().deadline(at))
            }
            Some(_) => Ending::Done(run.time_ms()),
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_process_waiting_for_a_deadline_runs_once_the_clock_reaches_it() {
    let beak = beak();
    let clock = Arc::new(Set(AtomicI64::new(1_000_000)));
    let options = Options {
        clock: Arc::clone(&clock) as Arc<dyn Clock>,
        ..Options::default()
    };
    let runtime = Runtime::builder().program(Sleeper).start(&beak, options).expect("started");
    let path = start::<Sleeper>(&runtime, "s", &()).await;
    let mut committed = runtime.committed();
    let asleep = async {
        loop {
            let record = runtime
                .snapshot()
                .expect("a snapshot")
                .record(&path)
                .expect("a record")
                .expect("there");
            if record.waits_until.is_some() {
                return;
            }
            committed.changed().await.expect("the runtime runs");
        }
    };
    tokio::time::timeout(Duration::from_secs(10), asleep).await.expect("asleep in time");
    clock.0.store(1_001_000, Ordering::SeqCst);
    runtime.wake();
    assert_eq!(
        ended(&runtime, &path).await,
        Outcome::Completed(Some(Bytes::from_static(b"1001000")))
    );
    runtime.close().await;
}

/// Ends with its input doubled.
struct Doubler;

impl Program for Doubler {
    const KIND: &'static str = "test.doubler";
    type Input = u32;
    type Checkpoint = ();
    type Output = u32;

    fn run(&self, _: &mut Run<'_>, input: u32, _: Option<()>) -> anyhow::Result<Ending<(), u32>> {
        Ok(Ending::Done(input * 2))
    }
}

/// Starts two doublers and ends with the sum of what they ended with, once both have.
struct Parent;

impl Program for Parent {
    const KIND: &'static str = "test.parent";
    type Input = ();
    type Checkpoint = ();
    type Output = u32;

    fn run(&self, run: &mut Run<'_>, (): (), checkpoint: Option<()>) -> anyhow::Result<Ending<(), u32>> {
        if checkpoint.is_none() {
            run.start::<Doubler>("d1", &1)?;
            run.start::<Doubler>("d2", &20)?;
            return Ok(Ending::Wait((), Wait::new().child("d1").child("d2")));
        }
        let mut sum = 0;
        for key in ["d1", "d2"] {
            let child = layout::process(run.path(), key.as_bytes());
            let record = run.snapshot().record(&child)?.expect("a child");
            let Some(Outcome::Completed(Some(result))) = record.outcome else {
                anyhow::bail!("{key} did not complete")
            };
            sum += serde_json::from_slice::<u32>(&result)?;
        }
        Ok(Ending::Done(sum))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_process_waiting_for_its_children_runs_once_every_one_ended() {
    let beak = beak();
    let runtime = Runtime::builder()
        .program(Parent)
        .program(Doubler)
        .start(&beak, memory())
        .expect("started");
    let path = start::<Parent>(&runtime, "p", &()).await;
    assert_eq!(ended(&runtime, &path).await, Outcome::Completed(Some(Bytes::from_static(b"42"))));
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_volume_grown_past_its_base_is_collected() {
    let beak = beak();
    let options = Options {
        collection: Collection {
            min_bytes: 4 << 10,
            growth_percent: 100,
        },
        ..Options::default()
    };
    let runtime = Runtime::builder().start(&beak, options).expect("started");
    // A file written again and again leaves what it held before for a collection to give back.
    let written = 64;
    for round in 0..written {
        let mut changes = Changes::new();
        changes.write(Bytes::from_static(b"notes"), format!("{round:0>1024}").into_bytes());
        runtime.commit(changes).await.expect("written");
    }
    let collected = async {
        loop {
            if runtime.kernel().usage().expect("usage") < written * 1024 / 2 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), collected)
        .await
        .expect("collected in time");
    runtime.close().await;
}

#[test]
fn a_volume_is_due_for_collection_once_grown_past_its_base_by_the_share_given_and_past_the_floor() {
    let collection = Collection {
        min_bytes: 100,
        growth_percent: 100,
    };
    assert!(!collection.is_due(80, 99));
    assert!(!collection.is_due(80, 159));
    assert!(collection.is_due(80, 160));
    assert!(!collection.is_due(0, 99));
    assert!(collection.is_due(0, 100));
}
