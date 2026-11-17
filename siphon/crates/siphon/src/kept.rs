//! What is kept running by its key for as long as it is used, as a connection pool keeps its connections: started as it
//! is first asked for, those who ask while it starts waiting on the one start; given to each who asks after; started
//! anew where it ended; let go of once nothing asked for it for its idle time, and all of it as the last handle goes.

use std::collections::HashMap;
use std::hash::Hash;
use std::pin::Pin;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tokio::task::{Id, JoinSet};
use tokio::time::Instant;

/// What is kept, as the keeper asks it whether it ended — a server whose connection closed, a program that exited —
/// to be started anew.
pub trait Ends {
    fn ended(&self) -> bool;
}

/// A start of what is kept, as it is given.
type Start<V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + Send>>;

/// What is kept running by its key, as a handle to the task that keeps it.
#[derive(Debug)]
pub struct Kept<K, V, E> {
    orders: mpsc::UnboundedSender<Order<K, V, E>>,
}

impl<K, V, E> Clone for Kept<K, V, E> {
    fn clone(&self) -> Self {
        Self {
            orders: self.orders.clone(),
        }
    }
}

/// What the task that keeps is asked: what is kept by a key, started by the start given where it is not; or what of
/// the keys given is kept running now, starting nothing.
enum Order<K, V, E> {
    Get(K, Start<V, E>, oneshot::Sender<Result<V, E>>),
    Held(Vec<K>, oneshot::Sender<Vec<Option<V>>>),
}

impl<K, V, E> std::fmt::Debug for Order<K, V, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Get(..) => "Get",
            Self::Held(..) => "Held",
        })
    }
}

impl<K, V, E> Kept<K, V, E>
where
    K: Eq + Hash + Clone + Send + 'static,
    V: Ends + Clone + Send + 'static,
    E: Clone + Send + 'static,
{
    /// A keeper that lets go of what nothing asked for over `idle`.
    #[must_use]
    pub fn new(idle: Duration) -> Self {
        let (orders, taken) = mpsc::unbounded_channel();
        tokio::spawn(keep(idle, taken));
        Self { orders }
    }

    /// What is kept by `key`, started by `start` where nothing is, or what was ended.
    ///
    /// # Errors
    /// Why `start` could not start it, as each who waited on it is told.
    ///
    /// # Panics
    /// Where `start` panicked, or the runtime the keeper ran on is gone.
    pub async fn get(&self, key: K, start: impl Future<Output = Result<V, E>> + Send + 'static) -> Result<V, E> {
        let (reply, got) = oneshot::channel();
        self.orders
            .send(Order::Get(key, Box::pin(start), reply))
            .expect("the keeper keeps till its last handle goes");
        got.await.expect("what was kept panicked as it started")
    }

    /// What of `keys` is kept running now, each by its place; none started.
    ///
    /// # Panics
    /// Where the runtime the keeper ran on is gone.
    pub async fn held(&self, keys: Vec<K>) -> Vec<Option<V>> {
        let (reply, held) = oneshot::channel();
        self.orders
            .send(Order::Held(keys, reply))
            .expect("the keeper keeps till its last handle goes");
        held.await.expect("the keeper answers each it is asked")
    }
}

/// Where a key stands: starting, those who asked waiting; or running, and when it was last asked for.
enum Slot<V, E> {
    Starting(Vec<oneshot::Sender<Result<V, E>>>),
    Running(V, Instant),
}

/// Keeps what is asked for, by its key, till the last handle goes.
async fn keep<K, V, E>(idle: Duration, mut orders: mpsc::UnboundedReceiver<Order<K, V, E>>)
where
    K: Eq + Hash + Clone + Send + 'static,
    V: Ends + Clone + Send + 'static,
    E: Clone + Send + 'static,
{
    let mut slots: HashMap<K, Slot<V, E>> = HashMap::new();
    let mut starting: JoinSet<(K, Result<V, E>)> = JoinSet::new();
    let mut started: HashMap<Id, K> = HashMap::new();
    let mut ticks = tokio::time::interval(idle / 10);
    loop {
        tokio::select! {
            order = orders.recv() => match order {
                None => break,
                Some(Order::Held(keys, reply)) => {
                    let held = keys
                        .iter()
                        .map(|key| match slots.get(key) {
                            Some(Slot::Running(value, _)) if !value.ended() => Some(value.clone()),
                            _ => None,
                        })
                        .collect();
                    let _ = reply.send(held);
                }
                Some(Order::Get(key, start, reply)) => match slots.get_mut(&key) {
                    Some(Slot::Running(value, used)) if !value.ended() => {
                        *used = Instant::now();
                        let _ = reply.send(Ok(value.clone()));
                    }
                    Some(Slot::Starting(waiting)) => waiting.push(reply),
                    _ => {
                        slots.insert(key.clone(), Slot::Starting(vec![reply]));
                        let begun = key.clone();
                        let task = starting.spawn(async move { (begun, start.await) });
                        started.insert(task.id(), key);
                    }
                },
            },
            Some(ended) = starting.join_next_with_id() => {
                let (id, outcome) = match ended {
                    Ok((id, outcome)) => (id, Some(outcome)),
                    Err(panicked) => (panicked.id(), None),
                };
                let Some(key) = started.remove(&id) else { continue };
                let Some(Slot::Starting(waiting)) = slots.remove(&key) else { continue };
                let Some((_, result)) = outcome else { continue };
                for reply in waiting {
                    let _ = reply.send(result.clone());
                }
                if let Ok(value) = result {
                    slots.insert(key, Slot::Running(value, Instant::now()));
                }
            }
            _ = ticks.tick() => {
                slots.retain(|_, slot| !matches!(slot, Slot::Running(_, used) if used.elapsed() >= idle));
            }
        }
    }
}
