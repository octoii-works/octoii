//! The operations on threads, each checked against the volume as it stands and committed as one.
//! A request that makes or adds to a thread is done once for its idempotency key.

use std::marker::PhantomData;

use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use octoii_siphon_lib::{Batch, Fiber, RequestId, RuntimeHandle, Signal};
use octoii_siphon_lib::{Query, Range, Snapshot, Status};
use serde::{Deserialize, Serialize};

use crate::threads::config::{Config, ConfigChange};
use crate::threads::idempotency::{self, Done};
use crate::threads::inbox::{self, Inbound, Joins};
use crate::threads::log::{self, Entry, Writer};
use crate::threads::marks::{MarksChange, SNOOZE_NAME};
use crate::threads::start;
use crate::threads::status::{self, Listed, Parent, TURN_ANNOTATION, Turn};
use crate::threads::thread::{
    self, ARCHIVED_LABEL, KIND, PARENT_CALL_ANNOTATION, PARENT_LABEL, PINNED_LABEL, PROJECT_LABEL, SUB_THREAD_KIND_ANNOTATION,
};
use crate::volume::commit_checked;

/// What a thread's main process is started with: the thread it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct MainInput {
    pub thread: String,
}

/// A thread to make: who makes it, how it runs, its project and the thread that delegates it, where it has them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct NewThread {
    pub actor: String,
    pub config: Config,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<Parent>,
}

/// Why an operation on a thread was not done.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no such thread")]
    NoSuchThread,
    #[error("a sub-thread must record its kind")]
    MissingSubThreadKind,
    #[error("a thread's workspace is fixed at startup")]
    WorkspaceFixed,
    #[error("{0}")]
    Start(String),
    #[error(transparent)]
    Idempotency(#[from] idempotency::Error),
    #[error(transparent)]
    NotGiven(octoii_siphon_lib::Error),
    #[error(transparent)]
    Log(#[from] log::Error),
    #[error(transparent)]
    Beak(#[from] octoii_siphon_lib::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    File(#[from] super::files::Refused),
}

/// The threads, on a runtime whose fiber `M` runs each thread's main process.
#[derive(Debug, Clone)]
pub struct Threads<M> {
    runtime: RuntimeHandle,
    main: PhantomData<fn(M)>,
}

impl<M> Threads<M> {
    #[must_use]
    pub fn new(runtime: RuntimeHandle) -> Self {
        Self {
            runtime,
            main: PhantomData,
        }
    }

    #[must_use]
    pub(crate) fn runtime(&self) -> &RuntimeHandle {
        &self.runtime
    }
}

impl<M: Fiber<Input = MainInput>> Threads<M> {
    /// Makes a thread as `new` says, its main process started: its id. Made again under the same `key` within a day,
    /// it is the thread made then.
    ///
    /// # Errors
    /// As the key says; the commit's failure.
    pub async fn create(&self, new: &NewThread, key: &str) -> Result<String, Error> {
        commit_checked(&self.runtime, |snapshot, changes| {
            if let Some(done) = idempotency::done_before(snapshot, key, "threads.create", None)? {
                return Ok(done.result);
            }
            let id = self.runtime.new_id()?;
            Self::made(changes, &id, &self.runtime.new_id()?, new)?;
            let done = Done {
                method: "threads.create".into(),
                thread: None,
                result: id.clone(),
            };
            idempotency::record(changes, key, &done, self.runtime.now_ms()?)?;
            Ok(id)
        })
        .await
    }

    /// Adds to `changes` what makes the thread `id` as `new` says: its directory, labelled and annotated, its inbox, its
    /// log begun with how it runs, by the entry `entry`, and its main process started — committed with whatever else
    /// `changes` holds, as a process that delegates makes its sub-thread with its own commit. The log has no writer
    /// before this commit.
    ///
    /// # Errors
    /// The volume's failure or what cannot be written as JSON.
    pub fn made(changes: &mut Batch<'_>, id: &str, entry: &str, new: &NewThread) -> Result<(), Error> {
        let path = thread::path(id);
        let named = |name: &'static str, value: Option<&str>| {
            (
                Bytes::from_static(name.as_bytes()),
                Some(Bytes::copy_from_slice(value.unwrap_or_default().as_bytes())),
            )
        };
        let labels = vec![
            named(PROJECT_LABEL, new.project.as_deref()),
            named(PARENT_LABEL, new.parent.as_ref().map(|parent| parent.thread.as_str())),
            (Bytes::from_static(PINNED_LABEL.as_bytes()), Some(thread::flag(false))),
            (Bytes::from_static(ARCHIVED_LABEL.as_bytes()), Some(thread::flag(false))),
        ];
        let made = new.config.as_made();
        let mut annotations = made.annotations();
        annotations.push((
            Bytes::from_static(TURN_ANNOTATION.as_bytes()),
            Some(serde_json::to_vec(&Turn::Created)?.into()),
        ));
        if let Some(parent) = &new.parent {
            annotations.push(named(PARENT_CALL_ANNOTATION, Some(&parent.call)));
            annotations.push((
                Bytes::from_static(SUB_THREAD_KIND_ANNOTATION.as_bytes()),
                Some(serde_json::to_vec(&parent.kind)?.into()),
            ));
        }
        changes
            .mkdir(&path)?
            .meta(path, labels, annotations)?
            .mkdir(inbox::path(id))?
            .mkdir(log::path(id))?;
        let entry = Entry::Config {
            id: entry.to_owned(),
            actor: new.actor.clone(),
            changed: made,
        };
        Writer::new_log(id).append(changes, &entry)?;
        changes.start::<M>(None, thread::main_key(id), &MainInput { thread: id.to_owned() })?;
        Ok(())
    }

    /// Adds to `changes` `inbound` delivered to the thread `id`, its main process started again where it had ended: its
    /// id in the inbox.
    fn put(&self, snapshot: &Snapshot, id: &str, inbound: &Inbound, changes: &mut Batch<'_>) -> Result<String, Error> {
        let delivery = self.runtime.new_id()?;
        inbox::put(changes, id, &delivery, inbound)?;
        Self::revive(snapshot, changes, id)?;
        Ok(delivery)
    }

    /// Adds to `changes` the main process of the thread `id` started again where it ended — failed past its retries,
    /// or killed — as socket activation starts a service a message comes for; one that lives is left as it is.
    ///
    /// # Errors
    /// The volume's failure; what cannot be written as JSON.
    pub fn revive(snapshot: &octoii_siphon_lib::Snapshot, changes: &mut Batch<'_>, id: &str) -> Result<(), Error> {
        match snapshot.fiber(&thread::main(id))? {
            Some(record) if !record.reapable => return Ok(()),
            Some(record) => {
                changes.reap(&record.id)?;
            }
            None => {}
        }
        changes.start::<M>(None, thread::main_key(id), &MainInput { thread: id.to_owned() })?;
        Ok(())
    }

    /// Delivers `content`, `actor`'s input, to the thread `id`, kept in its inbox till its main process takes it — at
    /// the next step of the turn that works, steering it, or as the next turn: its id there. Delivered again under the
    /// same `key` within a day, it is the input delivered then.
    ///
    /// # Errors
    /// `NoSuchThread`; as the key says; the commit's failure.
    pub async fn deliver(&self, id: &str, actor: &str, content: Vec<InputContent>, key: &str) -> Result<String, Error> {
        self.put_input(id, actor, content, Joins::Step, key).await
    }

    /// Delivers `content`, `actor`'s input, to the thread `id` as `deliver` does, to wait for the turn that works to end
    /// and begin the next: its id in the inbox.
    ///
    /// # Errors
    /// As `deliver`'s.
    pub async fn queue(&self, id: &str, actor: &str, content: Vec<InputContent>, key: &str) -> Result<String, Error> {
        self.put_input(id, actor, content, Joins::Turn, key).await
    }

    /// Delivers `content`, `actor`'s input, to the thread `id`, to join its work as `joins` says, once for `key`.
    async fn put_input(&self, id: &str, actor: &str, content: Vec<InputContent>, joins: Joins, key: &str) -> Result<String, Error> {
        let inbound = Inbound::Input {
            actor: actor.to_owned(),
            content,
            reply_to: None,
            joins,
        };
        commit_checked(&self.runtime, |snapshot, changes| {
            thread::directory(snapshot, id)?.ok_or(Error::NoSuchThread)?;
            if let Some(done) = idempotency::done_before(snapshot, key, "threads.deliver", Some(id))? {
                return Ok(done.result);
            }
            let delivery = self.put(snapshot, id, &inbound, changes)?;
            let done = Done {
                method: "threads.deliver".into(),
                thread: Some(id.to_owned()),
                result: delivery.clone(),
            };
            idempotency::record(changes, key, &done, self.runtime.now_ms()?)?;
            Ok(delivery)
        })
        .await
    }

    /// Changes how the thread `id` runs, as `actor` asks: kept in its inbox, then logged and set where its main process
    /// takes it, so the change stands in the log where it took effect. Its id in the inbox.
    ///
    /// # Errors
    /// `NoSuchThread`, `WorkspaceFixed`, or the commit's failure.
    pub async fn configure(&self, id: &str, actor: &str, change: ConfigChange) -> Result<String, Error> {
        if change.workspace.is_some() {
            return Err(Error::WorkspaceFixed);
        }
        let inbound = Inbound::Config {
            actor: actor.to_owned(),
            change,
        };
        commit_checked(&self.runtime, |snapshot, changes| {
            thread::directory(snapshot, id)?.ok_or(Error::NoSuchThread)?;
            self.put(snapshot, id, &inbound, changes)
        })
        .await
    }

    /// Marks the thread `id` as the person keeps it, in one commit.
    ///
    /// # Errors
    /// `NoSuchThread`; the commit's failure.
    pub async fn mark(&self, id: &str, change: &MarksChange) -> Result<(), Error> {
        commit_checked(&self.runtime, |snapshot, changes| {
            thread::directory(snapshot, id)?.ok_or(Error::NoSuchThread)?;
            let snoozed = snapshot.stat(&thread::within(id, SNOOZE_NAME))?.is_some();
            change.add(changes, id, snoozed, self.runtime.now_ms()?)?;
            Ok(())
        })
        .await
    }

    /// Takes back what was delivered to the thread `id` as `delivery`, while it waits in its inbox — who waits on an
    /// input told it was — whether it did.
    ///
    /// # Errors
    /// The commit's failure.
    pub async fn withdraw(&self, id: &str, delivery: &str) -> Result<bool, Error> {
        let file = inbox::file(id, delivery);
        commit_checked(&self.runtime, |snapshot, changes| {
            let Some(bytes) = snapshot.read(&file)? else {
                return Ok(false);
            };
            if let Inbound::Input {
                reply_to: Some(reply_to), ..
            } = serde_json::from_slice(&bytes)?
            {
                inbox::tell(snapshot, changes, &reply_to, &inbox::Ended::Withdrawn)?;
            }
            changes.delete(&file)?;
            Ok(true)
        })
        .await
    }

    /// Stops the thread `id` for `actor`: the inputs of theirs that wait withdrawn, and its turn asked to stop — none of
    /// what runs beside it, as an interrupt reaches the foreground alone — in one commit. Pending preparation is
    /// cancelled and acknowledged first, so a delayed preparation result cannot release its input.
    ///
    /// # Errors
    /// `NoSuchThread`; the commit's failure.
    pub async fn stop(&self, id: &str, actor: &str) -> Result<(), Error> {
        self.cancel_start(id).await?;
        commit_checked(&self.runtime, |snapshot, changes| {
            thread::directory(snapshot, id)?.ok_or(Error::NoSuchThread)?;
            for waiting in inbox::waiting(snapshot, id)? {
                if matches!(&waiting.inbound, Inbound::Input { actor: theirs, .. } if theirs == actor) {
                    changes.delete(inbox::file(id, &waiting.id))?;
                }
            }
            stop_turn(snapshot, changes, id)
        })
        .await
    }

    /// Deletes the thread `id` and every thread under it, in one commit, as `rm -rf` takes a tree: each gone with its
    /// inbox, its log and its processes, killed as they go. The ids gone, `id` first.
    ///
    /// # Errors
    /// `NoSuchThread`; the commit's failure.
    pub async fn delete(&self, id: &str) -> Result<Vec<String>, Error> {
        commit_checked(&self.runtime, |snapshot, changes| {
            thread::directory(snapshot, id)?.ok_or(Error::NoSuchThread)?;
            let mut gone = vec![id.to_owned()];
            let mut next = 0;
            while next < gone.len() {
                let query = Query {
                    kind: KIND.as_bytes(),
                    name: PARENT_LABEL.as_bytes(),
                    value: Some(gone[next].as_bytes()),
                    under: b"",
                    after: None,
                };
                for found in snapshot.find(&query, usize::MAX)? {
                    gone.extend(thread::id_of(&found.path).map(str::to_owned));
                }
                next += 1;
            }
            for each in &gone {
                changes.delete(thread::path(each))?;
                if let Some(main) = snapshot.fiber(&thread::main(each))? {
                    changes.force_remove(&main.id, Some(Bytes::from_static(b"deleted")))?;
                }
            }
            Ok(gone)
        })
        .await
    }

    /// Gives a person's answer to the exact request of a process in this thread.
    ///
    /// # Errors
    /// The thread or request is missing, the request belongs to another thread, or Siphon rejects the answer.
    pub async fn answer(&self, id: &str, token: &str, answer: &serde_json::Value) -> Result<(), Error> {
        let snapshot = self.runtime.snapshot()?;
        thread::directory(&snapshot, id)?.ok_or(Error::NoSuchThread)?;
        let main = thread::main(id);
        let missing = || Error::NotGiven(octoii_siphon_lib::Error::new(Status::NotFound, "no such request waits"));
        let request = RequestId::from_token(token)
            .filter(|request| {
                request.path() == main.as_ref()
                    || request
                        .path()
                        .strip_prefix(main.as_ref())
                        .is_some_and(|suffix| suffix.starts_with(b"/"))
            })
            .ok_or_else(missing)?;
        if snapshot
            .fiber(request.path())?
            .is_none_or(|record| record.outcome.is_some() || &record.id != request.fiber())
        {
            return Err(missing());
        }
        self.runtime.answer(&request, answer).await.map_err(Error::NotGiven)?;
        Ok(())
    }

    /// The thread `id`, as a list shows it.
    ///
    /// # Errors
    /// `NoSuchThread`; the volume's failure.
    pub fn listed(&self, id: &str) -> Result<Listed, Error> {
        self.listed_at(&*self.runtime.snapshot()?, id)
    }

    pub(crate) fn listed_at(&self, snapshot: &octoii_siphon_lib::Snapshot, id: &str) -> Result<Listed, Error> {
        let directory = thread::directory(snapshot, id)?.ok_or(Error::NoSuchThread)?;
        status::listed(&self.runtime, snapshot, id, &directory)
    }

    /// Every thread, as a list shows it, in the order they were made.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn list(&self) -> Result<Vec<Listed>, Error> {
        self.list_at(&*self.runtime.snapshot()?)
    }

    pub(crate) fn list_at(&self, snapshot: &octoii_siphon_lib::Snapshot) -> Result<Vec<Listed>, Error> {
        listed_all(&self.runtime, snapshot)
    }
}

/// Stages a cancellation of pending preparation and asks the turn of `id` to stop, where one runs.
pub(crate) fn stop_turn(snapshot: &Snapshot, changes: &mut Batch<'_>, id: &str) -> Result<(), Error> {
    start::stage_cancel(snapshot, changes, id)?;
    for fiber in snapshot.fibers(&thread::main(id), thread::TURN_KIND.as_bytes())? {
        changes.signal(&fiber, Signal::Terminate, Some(Bytes::from_static(b"stopped")))?;
    }
    Ok(())
}

/// Every thread, as a list shows it, in the order they were made — as the person's thread list shows them, for an
/// extension's own list of them.
///
/// # Errors
/// The volume's failure; a directory not as Octoii writes it.
pub fn list(volume: &crate::volume::Volume) -> anyhow::Result<Vec<Listed>> {
    Ok(listed_all(&volume.runtime, &*volume.snapshot()?)?)
}

/// Every thread the snapshot holds, as a list shows it, in the order they were made.
fn listed_all(runtime: &RuntimeHandle, snapshot: &octoii_siphon_lib::Snapshot) -> Result<Vec<Listed>, Error> {
    let prefix = format!("{KIND}:");
    let range = Range {
        prefix: prefix.as_bytes(),
        limit: usize::MAX,
        ..Range::default()
    };
    let mut out = Vec::new();
    for entry in snapshot.list(b"", &range)?.unwrap_or_default() {
        let (Some(id), Some(directory)) = (thread::id_of(&entry.name), entry.directory.as_ref()) else {
            continue;
        };
        out.push(status::listed(runtime, snapshot, id, directory)?);
    }
    Ok(out)
}
