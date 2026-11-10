//! The operations on threads, each checked against the volume as it stands and committed as one, durable as it
//! returns; one that makes or adds to a thread done once for its idempotency key.

use std::marker::PhantomData;

use beak::{Query, Range, layout};
use bytes::Bytes;
use octoii_ai::vocabulary::InputContent;
use serde::{Deserialize, Serialize};
use siphon_runtime::{Changes, NotGiven, Program, Runtime};

use crate::config::{Config, ConfigChange};
use crate::idempotency::{self, Done};
use crate::inbox::{self, Inbound};
use crate::log::{self, Entry, Writer};
use crate::marks::{MarksChange, SNOOZE_NAME};
use crate::status::{self, Listed, Parent, TURN_ANNOTATION, Turn};
use crate::thread::{self, ARCHIVED_LABEL, KIND, MAIN_KEY, PARENT_CALL_ANNOTATION, PARENT_LABEL, PINNED_LABEL, PROJECT_LABEL};

/// What a thread's main process is started with: the thread it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MainInput {
    pub thread: String,
}

/// A thread to make: who makes it, how it runs, its project and the thread that delegates it, where it has them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    #[error(transparent)]
    Idempotency(#[from] idempotency::Error),
    #[error(transparent)]
    NotGiven(#[from] NotGiven),
    #[error(transparent)]
    Log(#[from] log::Error),
    #[error(transparent)]
    Beak(#[from] beak::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// The threads, on a runtime whose program `M` runs each thread's main process.
#[derive(Debug, Clone)]
pub struct Threads<M> {
    runtime: Runtime,
    main: PhantomData<fn(M)>,
}

impl<M> Threads<M> {
    #[must_use]
    pub fn new(runtime: Runtime) -> Self {
        Self {
            runtime,
            main: PhantomData,
        }
    }

    #[must_use]
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }
}

impl<M: Program<Input = MainInput>> Threads<M> {
    /// Makes a thread as `new` says, its main process started: its id. Made again under the same `key` within a day,
    /// it is the thread made then.
    ///
    /// # Errors
    /// As the key says; the commit's failure.
    pub async fn create(&self, new: &NewThread, key: &str) -> Result<String, Error> {
        if let Some(done) = idempotency::done_before(&self.runtime.snapshot()?, key, "threads.create", None)? {
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
        idempotency::record(&mut changes, key, &done, self.runtime.now_ms())?;
        self.runtime.commit(changes).await?;
        Ok(id)
    }

    /// Adds to `changes` what makes the thread `id` as `new` says: its directory, labelled and annotated, its inbox, its
    /// log begun with how it runs, by the entry `entry`, and its main process started — committed with whatever else
    /// `changes` holds, as a process that delegates makes its sub-thread with its own commit. The log has no writer
    /// before this commit.
    ///
    /// # Errors
    /// What cannot be written as JSON.
    pub fn made(changes: &mut Changes, id: &str, entry: &str, new: &NewThread) -> Result<(), Error> {
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
    fn directory(&self, id: &str) -> Result<beak::Directory, Error> {
        self.runtime
            .snapshot()?
            .stat(&thread::path(id))?
            .and_then(|entry| entry.directory)
            .filter(|directory| !removing(directory))
            .ok_or(Error::NoSuchThread)
    }

    /// Adds to `changes` `inbound` delivered to the thread `id`, its main process started again where it had ended: its
    /// id in the inbox.
    fn put(&self, id: &str, inbound: &Inbound, changes: &mut Changes) -> Result<String, Error> {
        let delivery = self.runtime.new_id()?;
        inbox::put(changes, id, &delivery, inbound)?;
        Self::revive(&self.runtime.snapshot()?, changes, id)?;
        Ok(delivery)
    }

    /// Adds to `changes` the main process of the thread `id` started again where it ended — failed past its retries,
    /// or killed — as socket activation starts a service a message comes for; one that lives is left as it is.
    ///
    /// # Errors
    /// The volume's failure; what cannot be written as JSON.
    pub fn revive(snapshot: &beak::Snapshot, changes: &mut Changes, id: &str) -> Result<(), Error> {
        let main = thread::main(id);
        match snapshot.record(&main)? {
            Some(record) if record.state != beak::ProcessState::Zombie => return Ok(()),
            Some(_) => {
                changes.reap(main);
            }
            None => {}
        }
        changes.start::<M>(thread::path(id), MAIN_KEY, &MainInput { thread: id.to_owned() })?;
        Ok(())
    }

    /// Delivers `content`, `actor`'s input, to the thread `id`, kept in its inbox till its main process takes it: its
    /// id there. Delivered again under the same `key` within a day, it is the input delivered then.
    ///
    /// # Errors
    /// `NoSuchThread`; as the key says; the commit's failure.
    pub async fn deliver(&self, id: &str, actor: &str, content: Vec<InputContent>, key: &str) -> Result<String, Error> {
        self.directory(id)?;
        if let Some(done) = idempotency::done_before(&self.runtime.snapshot()?, key, "threads.deliver", Some(id))? {
            return Ok(done.result);
        }
        let mut changes = Changes::new();
        let inbound = Inbound::Input {
            actor: actor.to_owned(),
            content,
            reply_to: None,
        };
        let delivery = self.put(id, &inbound, &mut changes)?;
        let done = Done {
            method: "threads.deliver".into(),
            thread: Some(id.to_owned()),
            result: delivery.clone(),
        };
        idempotency::record(&mut changes, key, &done, self.runtime.now_ms())?;
        self.runtime.commit(changes).await?;
        Ok(delivery)
    }

    /// Changes how the thread `id` runs, as `actor` asks: kept in its inbox, then logged and set where its main process
    /// takes it, so the change stands in the log where it took effect. Its id in the inbox.
    ///
    /// # Errors
    /// `NoSuchThread`; the commit's failure.
    pub async fn configure(&self, id: &str, actor: &str, change: ConfigChange) -> Result<String, Error> {
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
        change.add(&mut changes, id, snoozed, self.runtime.now_ms());
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
        let Some(bytes) = snapshot.read(&file, 0, u64::MAX)? else {
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
    /// what runs beside it, as an interrupt reaches the foreground alone — in one commit.
    ///
    /// # Errors
    /// `NoSuchThread`; the commit's failure.
    pub async fn stop(&self, id: &str, actor: &str) -> Result<(), Error> {
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

    /// Gives the person's `answer` to the request of `key` a process of the thread `id` waits on.
    ///
    /// # Errors
    /// `NoSuchThread`; as the runtime gives it.
    pub async fn answer(&self, id: &str, key: &str, answer: &serde_json::Value) -> Result<(), Error> {
        self.directory(id)?;
        let under = thread::path(id);
        let query = Query {
            kind: layout::REQUEST_KIND,
            name: layout::REQUEST_LABEL,
            value: None,
            under: &under,
            after: None,
        };
        for found in self.runtime.snapshot()?.find(&query, usize::MAX)? {
            if let Some((process, asked)) = status::split_request(&found.path)
                && asked == key.as_bytes()
            {
                self.runtime.answer(process, asked, answer).await?;
                return Ok(());
            }
        }
        Err(NotGiven::NoSuchRequest.into())
    }

    /// The thread `id`, as a list shows it.
    ///
    /// # Errors
    /// `NoSuchThread`; the volume's failure.
    pub fn listed(&self, id: &str) -> Result<Listed, Error> {
        let directory = self.directory(id)?;
        Ok(status::listed(&self.runtime, &self.runtime.snapshot()?, id, &directory)?)
    }

    /// Every thread, as a list shows it, in the order they were made.
    ///
    /// # Errors
    /// The volume's failure.
    pub fn list(&self) -> Result<Vec<Listed>, Error> {
        let snapshot = self.runtime.snapshot()?;
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
            if removing(directory) {
                continue;
            }
            out.push(status::listed(&self.runtime, &snapshot, id, directory)?);
        }
        Ok(out)
    }
}

/// Whether `directory` is being removed, to go once the processes under it end.
fn removing(directory: &beak::Directory) -> bool {
    directory.label(layout::REMOVING_LABEL).is_some_and(|value| value == "true")
}
