//! The operations on threads, each checked against the volume as it stands and committed as one, durable as it
//! returns; one that makes or adds to a thread done once for its idempotency key.

use std::marker::PhantomData;

use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use octoii_siphon_lib::{Batch, Changes, Fiber, RuntimeHandle};
use octoii_siphon_lib::{Query, Range};
use serde::{Deserialize, Serialize};

use crate::protocol::siphon::request_id;
use crate::threads::config::{Config, ConfigChange};
use crate::threads::idempotency::{self, Done};
use crate::threads::inbox::{self, Inbound, Joins};
use crate::threads::log::{self, Entry, Writer};
use crate::threads::marks::{MarksChange, SNOOZE_NAME};
use crate::threads::status::{self, Listed, Parent, TURN_ANNOTATION, Turn};
use crate::threads::thread::{
    self, ARCHIVED_LABEL, KIND, MAIN_KEY, PARENT_CALL_ANNOTATION, PARENT_LABEL, PINNED_LABEL, PROJECT_LABEL, SUB_THREAD_KIND_ANNOTATION,
};

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
    pub fn runtime(&self) -> &RuntimeHandle {
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
        if let Some(done) = idempotency::done_before(&*self.runtime.snapshot()?, key, "threads.create", None)? {
            return Ok(done.result);
        }
        let id = self.runtime.new_id()?;
        let mut changes = Changes::new();
        Self::made(&mut changes, &id, &self.runtime.new_id()?, new)?;
        let done = Done {
            method: "threads.create".into(),
            thread: None,
            result: id.clone(),
        };
        idempotency::record(&mut changes, key, &done, self.runtime.now_ms()?)?;
        self.runtime.commit(changes).await?;
        Ok(id)
    }

    /// Atomically accepts a thread and its first input, without permitting execution.
    /// The immutable claim has no time-based expiry; deleting its thread never recreates it on replay.
    ///
    /// # Errors
    /// A reused key with different input, a missing deleted thread, or a storage failure.
    pub async fn prepare(
        &self,
        new: &NewThread,
        content: Vec<super::InputPart>,
        request: super::StartRequest,
        key: &str,
    ) -> Result<(String, super::Start), Error> {
        idempotency::done_before(&*self.runtime.snapshot()?, key, "threads.prepare", None)?;
        if request.owner.trim().is_empty() || content.is_empty() {
            return Err(Error::Start("preparation requires an owner and initial input".into()));
        }
        let path: Bytes = [b"siphon.start/", key.as_bytes()].concat().into();
        let claim = StartClaim {
            fingerprint: super::input::fingerprint(new, &content, &request)?,
            thread: self.runtime.new_id()?,
        };
        if let Some(bytes) = self.runtime.snapshot()?.read_range(&path, 0, u64::MAX)? {
            return self.claimed(&bytes, &claim);
        }
        let start = super::Start {
            request,
            token: self.runtime.new_id()?,
            previous: None,
            state: super::StartState::Pending,
        };
        let mut changes = Changes::new();
        Self::made(&mut changes, &claim.thread, &self.runtime.new_id()?, new)?;
        super::start::made(&mut changes, &claim.thread, &start)?;
        let content = super::input::keep(&*self.runtime.snapshot()?, &mut changes, &claim.thread, content)?;
        inbox::put(
            &mut changes,
            &claim.thread,
            &self.runtime.new_id()?,
            &Inbound::Input {
                actor: new.actor.clone(),
                content,
                reply_to: None,
                joins: Joins::Step,
            },
        )?;
        changes
            .mkdir(Bytes::from_static(b"siphon.start"))
            .write(path.clone(), serde_json::to_vec(&claim)?)
            .seal(path.clone());
        if let Err(error) = self.runtime.commit(changes).await {
            if let Some(bytes) = self.runtime.snapshot()?.read_range(&path, 0, u64::MAX)? {
                return self.claimed(&bytes, &claim);
            }
            return Err(error.into());
        }
        Ok((claim.thread, start))
    }

    fn claimed(&self, bytes: &[u8], asked: &StartClaim) -> Result<(String, super::Start), Error> {
        let kept: StartClaim = serde_json::from_slice(bytes)?;
        if kept.fingerprint != asked.fingerprint {
            return Err(Error::Start("startup key was used with different input".into()));
        }
        let start = self
            .start_state(&kept.thread)?
            .ok_or_else(|| Error::Start("missing startup state".into()))?;
        Ok((kept.thread, start))
    }

    /// Reads startup state without exposing its storage paths.
    ///
    /// # Errors
    /// A missing thread or invalid stored state.
    pub fn start_state(&self, id: &str) -> Result<Option<super::Start>, Error> {
        Ok(super::Start::of(&self.directory(id)?)?)
    }

    pub(crate) async fn start_command(&self, id: &str, token: &str, action: super::start::Action) -> Result<super::Start, Error> {
        self.start_state(id)?
            .ok_or_else(|| Error::Start("thread has no startup gate".into()))?;
        let key = self.runtime.new_id()?;
        let command = super::start::Command {
            token: token.into(),
            action,
        };
        let mut changes = Changes::new();
        changes.write(super::start::command_path(id, &key), serde_json::to_vec(&command)?);
        Self::revive(&*self.runtime.snapshot()?, &mut changes, id)?;
        let mut committed = self.runtime.committed()?;
        self.runtime.commit(changes).await?;
        loop {
            let snapshot = self.runtime.snapshot()?;
            if let Some(bytes) = snapshot.read_range(&super::start::result_path(id, &key), 0, u64::MAX)? {
                let result: Result<super::Start, String> = serde_json::from_slice(&bytes)?;
                return result.map_err(Error::Start);
            }
            thread::directory(&snapshot, id)?.ok_or(Error::NoSuchThread)?;
            if snapshot.fiber(&thread::main(id))?.is_some_and(|record| record.outcome.is_some()) {
                return Err(Error::Start("thread process ended before startup completed".into()));
            }
            committed
                .next()
                .await
                .ok_or_else(|| Error::Start("runtime closed before startup completed".into()))?;
        }
    }

    /// Adds to `changes` what makes the thread `id` as `new` says: its directory, labelled and annotated, its inbox, its
    /// log begun with how it runs, by the entry `entry`, and its main process started — committed with whatever else
    /// `changes` holds, as a process that delegates makes its sub-thread with its own commit. The log has no writer
    /// before this commit.
    ///
    /// # Errors
    /// What cannot be written as JSON.
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
            .mkdir(path.clone())
            .meta(path, labels, annotations)
            .mkdir(inbox::path(id))
            .mkdir(log::path(id));
        let entry = Entry::Config {
            id: entry.to_owned(),
            actor: new.actor.clone(),
            changed: made,
        };
        Writer::new_log(id).append(changes, &entry)?;
        changes.start::<M>(thread::path(id), MAIN_KEY, &MainInput { thread: id.to_owned() })?;
        Ok(())
    }

    /// The thread `id`'s directory, as it stands; none for one being removed, gone as its processes end.
    fn directory(&self, id: &str) -> Result<octoii_siphon_lib::Directory, Error> {
        thread::directory(&*self.runtime.snapshot()?, id)?.ok_or(Error::NoSuchThread)
    }

    /// Adds to `changes` `inbound` delivered to the thread `id`, its main process started again where it had ended: its
    /// id in the inbox.
    fn put(&self, id: &str, inbound: &Inbound, changes: &mut Batch<'_>) -> Result<String, Error> {
        let delivery = self.runtime.new_id()?;
        inbox::put(changes, id, &delivery, inbound)?;
        Self::revive(&*self.runtime.snapshot()?, changes, id)?;
        Ok(delivery)
    }

    /// Adds to `changes` the main process of the thread `id` started again where it ended — failed past its retries,
    /// or killed — as socket activation starts a service a message comes for; one that lives is left as it is.
    ///
    /// # Errors
    /// The volume's failure; what cannot be written as JSON.
    pub fn revive(snapshot: &octoii_siphon_lib::Snapshot, changes: &mut Batch<'_>, id: &str) -> Result<(), Error> {
        let main = thread::main(id);
        match snapshot.fiber(&main)? {
            Some(record) if !crate::volume::reapable(&record) => return Ok(()),
            Some(_) => {
                changes.reap(main);
            }
            None => {}
        }
        changes.start::<M>(thread::path(id), MAIN_KEY, &MainInput { thread: id.to_owned() })?;
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
        self.directory(id)?;
        if let Some(done) = idempotency::done_before(&*self.runtime.snapshot()?, key, "threads.deliver", Some(id))? {
            return Ok(done.result);
        }
        let mut changes = Changes::new();
        let inbound = Inbound::Input {
            actor: actor.to_owned(),
            content,
            reply_to: None,
            joins,
        };
        let delivery = self.put(id, &inbound, &mut changes)?;
        let done = Done {
            method: "threads.deliver".into(),
            thread: Some(id.to_owned()),
            result: delivery.clone(),
        };
        idempotency::record(&mut changes, key, &done, self.runtime.now_ms()?)?;
        self.runtime.commit(changes).await?;
        Ok(delivery)
    }

    /// Changes how the thread `id` runs, as `actor` asks: kept in its inbox, then logged and set where its main process
    /// takes it, so the change stands in the log where it took effect. Its id in the inbox.
    ///
    /// # Errors
    /// `NoSuchThread`; the commit's failure.
    pub async fn configure(&self, id: &str, actor: &str, change: ConfigChange) -> Result<String, Error> {
        if change.workspace.is_some() && self.start_state(id)?.is_some_and(|start| start.blocks_execution()) {
            return Err(Error::Start("workspace is bound when preparation completes".into()));
        }
        self.directory(id)?;
        let mut changes = Changes::new();
        let inbound = Inbound::Config {
            actor: actor.to_owned(),
            change,
        };
        let delivery = self.put(id, &inbound, &mut changes)?;
        self.runtime.commit(changes).await?;
        Ok(delivery)
    }

    /// Marks the thread `id` as the person keeps it, in one commit.
    ///
    /// # Errors
    /// `NoSuchThread`; the commit's failure.
    pub async fn mark(&self, id: &str, change: &MarksChange) -> Result<(), Error> {
        self.directory(id)?;
        let snoozed = self.runtime.snapshot()?.stat(&thread::within(id, SNOOZE_NAME))?.is_some();
        let mut changes = Changes::new();
        change.add(&mut changes, id, snoozed, self.runtime.now_ms()?);
        if !changes.is_empty() {
            self.runtime.commit(changes).await?;
        }
        Ok(())
    }

    /// Takes back what was delivered to the thread `id` as `delivery`, while it waits in its inbox — who waits on an
    /// input told it was — whether it did.
    ///
    /// # Errors
    /// The commit's failure.
    pub async fn withdraw(&self, id: &str, delivery: &str) -> Result<bool, Error> {
        let snapshot = self.runtime.snapshot()?;
        let file = inbox::file(id, delivery);
        let Some(bytes) = snapshot.read_range(&file, 0, u64::MAX)? else {
            return Ok(false);
        };
        let mut changes = Changes::new();
        if let Inbound::Input {
            reply_to: Some(reply_to), ..
        } = serde_json::from_slice(&bytes)?
        {
            inbox::tell(&snapshot, &mut changes, &reply_to, &inbox::Ended::Withdrawn)?;
        }
        changes.delete(file);
        self.runtime.commit(changes).await?;
        Ok(true)
    }

    /// Stops the thread `id` for `actor`: the inputs of theirs that wait withdrawn, and its turn asked to stop — none of
    /// what runs beside it, as an interrupt reaches the foreground alone — in one commit. Pending preparation is
    /// cancelled and acknowledged first, so a delayed preparation result cannot release its input.
    ///
    /// # Errors
    /// `NoSuchThread`; the commit's failure.
    pub async fn stop(&self, id: &str, actor: &str) -> Result<(), Error> {
        if let Some(start) = self.start_state(id)?
            && start.blocks_execution()
            && let Err(error) = self.start_command(id, &start.token, super::start::Action::Cancel).await
            && self.start_state(id)?.is_some_and(|current| current.blocks_execution())
        {
            return Err(error);
        }
        self.directory(id)?;
        let snapshot = self.runtime.snapshot()?;
        let mut changes = Changes::new();
        for waiting in inbox::waiting(&snapshot, id)? {
            if matches!(&waiting.inbound, Inbound::Input { actor: theirs, .. } if theirs == actor) {
                changes.delete(inbox::file(id, &waiting.id));
            }
        }
        thread::stop_turn(&snapshot, &mut changes, id)?;
        if !changes.is_empty() {
            self.runtime.commit(changes).await?;
        }
        Ok(())
    }

    /// Deletes the thread `id` and every thread under it, in one commit, as `rm -rf` takes a tree: each gone with its
    /// inbox, its log and its processes, killed as they go. The ids gone, `id` first.
    ///
    /// # Errors
    /// `NoSuchThread`; the commit's failure.
    pub async fn delete(&self, id: &str) -> Result<Vec<String>, Error> {
        self.directory(id)?;
        let snapshot = self.runtime.snapshot()?;
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
        let mut changes = Changes::new();
        for each in &gone {
            changes.force_remove(thread::path(each), Some(Bytes::from_static(b"deleted")));
        }
        self.runtime.commit(changes).await?;
        Ok(gone)
    }

    /// Gives a person's answer to the exact request of a process in this thread.
    ///
    /// # Errors
    /// The thread or request is missing, the request belongs to another thread, or Siphon rejects the answer.
    pub async fn answer(&self, id: &str, token: &str, answer: &serde_json::Value) -> Result<(), Error> {
        self.directory(id)?;
        let request = request_id(token)
            .filter(|request| thread::id_of(&request.process) == Some(id))
            .ok_or_else(|| {
                Error::NotGiven(octoii_siphon_lib::Error::new(
                    octoii_siphon_lib::Status::NotFound,
                    "no such request waits",
                ))
            })?;
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
        if thread::removing(directory) {
            continue;
        }
        out.push(status::listed(runtime, snapshot, id, directory)?);
    }
    Ok(out)
}

#[derive(Debug, Serialize, Deserialize)]
struct StartClaim {
    fingerprint: [u8; 32],
    thread: String,
}
