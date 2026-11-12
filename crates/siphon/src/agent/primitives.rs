//! What a call's plan does to threads, done by its caller in the commit that ends the call, held to the rules a kernel
//! holds its processes to: a sub-thread works on its task alone, reaching no other thread; no thread is sent a message
//! by one whose mode reaches less far, which would reach further through it; a thread neither messages nor stops itself.
//! A thread made is made like the caller's thread; a delivery's end is told to the call, to a process beside the
//! caller's main process that delivers it to the caller's inbox, or to no one.

use std::collections::HashSet;

use crate::agent::thread::ThreadProgram;
use crate::agent::waiting::{DelegatedProgram, Waiting, directory_of, label};
use crate::threads::inbox::{self, Inbound, ReplyTo};
use crate::threads::{Config, Mode, NewThread, Parent, Threads, thread};
use crate::tools::{Awaited, Plan, Primitives, Report, Spawned, Step};
use beak::layout;
use siphon_runtime::{Changes, Run};

/// Delivers `content` from `actor` to the inbox of the thread `thread` at once, outside any call — as a terminal tells
/// its thread that the command it ran ended: it joins the thread's work at its next step, and no one is told of it.
///
/// # Errors
/// The volume's failure.
pub async fn deliver(
    volume: &crate::volume::Volume,
    thread: &str,
    actor: &str,
    content: Vec<octoii_ai::vocabulary::InputContent>,
) -> anyhow::Result<()> {
    let inbound = Inbound::Input {
        actor: actor.to_owned(),
        content,
        reply_to: None,
        joins: crate::threads::inbox::Joins::Step,
    };
    let mut changes = Changes::new();
    inbox::put(&mut changes, thread, &volume.new_id()?, &inbound)?;
    volume.runtime.commit(changes).await?;
    Ok(())
}

/// The thread that made the thread `thread`, as a sub-thread of one of its calls, on `runtime`; none for a thread of the
/// person's, or one that is not there.
///
/// # Errors
/// The volume's failure.
pub fn parent_of(volume: &crate::volume::Volume, thread: &str) -> anyhow::Result<Option<String>> {
    let directory = volume.snapshot()?.stat(&thread::path(thread))?.and_then(|entry| entry.directory);
    Ok(directory.and_then(|directory| label(&directory, thread::PARENT_LABEL)))
}

/// The primitives on threads, as Siphon does them.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadPrimitives;

impl Primitives for ThreadPrimitives {
    fn apply(&self, run: &mut Run<'_>, own: &str, call: &str, plan: &Plan) -> anyhow::Result<Result<(), String>> {
        let Some(directory) = directory_of(run, own)? else {
            return Ok(Err("the thread is gone".into()));
        };
        if label(&directory, thread::PARENT_LABEL).is_some() {
            return Ok(Err(
                "a sub-thread works on its task alone and reaches no other thread: do it yourself".into(),
            ));
        }
        let mut made = HashSet::new();
        let mut changes = Changes::new();
        for step in &plan.steps {
            let refused = match step {
                Step::Spawn {
                    thread,
                    entry,
                    title,
                    spawned,
                } => {
                    spawn(&mut changes, own, call, &directory, thread, entry, title.clone(), *spawned)?;
                    made.insert(thread.clone());
                    None
                }
                Step::Deliver {
                    thread,
                    delivery,
                    content,
                    joins,
                    report,
                } => {
                    let refused = if made.contains(thread) {
                        None
                    } else {
                        refused(run, own, &directory, thread)?
                    };
                    if refused.is_none() {
                        if !made.contains(thread) {
                            Threads::<ThreadProgram>::revive(run.snapshot(), &mut changes, thread)?;
                        }
                        let reply_to = reply_to(run, &mut changes, own, thread, delivery, *report)?;
                        let inbound = Inbound::Input {
                            actor: own.to_owned(),
                            content: content.clone(),
                            reply_to,
                            joins: *joins,
                        };
                        inbox::put(&mut changes, thread, delivery, &inbound)?;
                    }
                    refused
                }
                Step::Stop { thread } => {
                    let refused = if thread == own {
                        Some("a thread stops not itself: end your reply".to_owned())
                    } else if directory_of(run, thread)?.is_none() {
                        Some(format!("no thread {thread}"))
                    } else {
                        None
                    };
                    if refused.is_none() {
                        thread::stop_turn(run.snapshot(), &mut changes, thread)?;
                    }
                    refused
                }
            };
            if let Some(refused) = refused {
                return Ok(Err(refused));
            }
        }
        run.changes.extend(changes);
        Ok(Ok(()))
    }

    fn withdraw(&self, run: &mut Run<'_>, awaited: &Awaited) -> anyhow::Result<()> {
        let waiting = inbox::file(&awaited.thread, &awaited.delivery);
        if run.snapshot().stat(&waiting)?.is_some() {
            run.changes.delete(waiting);
        } else {
            let mut stopping = Changes::new();
            thread::stop_turn(run.snapshot(), &mut stopping, &awaited.thread)?;
            run.changes.extend(stopping);
        }
        Ok(())
    }
}

/// Makes the thread `id` like the thread `own`, whose directory is `directory` — its folder, its model, its mode and its
/// effort — titled `title`, with `changes`: a sub-thread of the call `call`, or a thread of the person's.
#[expect(clippy::too_many_arguments, reason = "a step's fields and the thread it is made like")]
fn spawn(
    changes: &mut Changes,
    own: &str,
    call: &str,
    directory: &beak::Directory,
    id: &str,
    entry: &str,
    title: Option<String>,
    spawned: Spawned,
) -> anyhow::Result<()> {
    let config = Config::of(directory)?;
    let new = NewThread {
        actor: own.to_owned(),
        config: Config { title, ..config },
        project: label(directory, thread::PROJECT_LABEL),
        parent: (spawned == Spawned::SubThread).then(|| Parent {
            thread: own.to_owned(),
            call: call.to_owned(),
        }),
    };
    Threads::<ThreadProgram>::made(changes, id, entry, &new)?;
    Ok(())
}

/// Why the thread `own`, whose directory is `directory`, may not deliver to `target`: it is itself, it is not there,
/// or its mode reaches further than `own`'s; none where it may.
fn refused(run: &Run<'_>, own: &str, directory: &beak::Directory, target: &str) -> anyhow::Result<Option<String>> {
    if target == own {
        return Ok(Some("a thread sends itself no message".into()));
    }
    let Some(theirs) = directory_of(run, target)? else {
        return Ok(Some(format!("no thread {target}")));
    };
    let reach = |config: &Config| match config.mode.unwrap_or_default() {
        Mode::Default => 0,
        Mode::Auto => 1,
        Mode::FullAccess => 2,
    };
    if reach(&Config::of(&theirs)?) > reach(&Config::of(directory)?) {
        return Ok(Some(format!(
            "the thread {target} runs in a mode that reaches further than yours: the person must message it"
        )));
    }
    Ok(None)
}

/// Who is told how the turn of `target` that takes `delivery` ends, as `report` says, with `changes`: the call of `run`,
/// by a mail keyed by the delivery; a process beside the main process of `own`, started, which delivers it to `own`'s
/// inbox; or no one.
fn reply_to(
    run: &Run<'_>,
    changes: &mut Changes,
    own: &str,
    target: &str,
    delivery: &str,
    report: Report,
) -> anyhow::Result<Option<ReplyTo>> {
    Ok(match report {
        Report::None => None,
        Report::Call => Some(ReplyTo {
            process: String::from_utf8_lossy(run.path()).into_owned(),
            started: run.snapshot().record(run.path())?.map(|record| record.created_seq),
            key: delivery.to_owned(),
        }),
        Report::Inbox => {
            let waiter = format!("told-{delivery}");
            let waiting = Waiting {
                thread: own.to_owned(),
                child: target.to_owned(),
                task: delivery.to_owned(),
            };
            changes.start::<DelegatedProgram>(thread::main(own), &waiter, &waiting)?;
            Some(ReplyTo {
                process: String::from_utf8_lossy(&layout::process(&thread::main(own), waiter.as_bytes())).into_owned(),
                started: None,
                key: delivery.to_owned(),
            })
        }
    })
}
