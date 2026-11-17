//! What streams of each request while its handler answers it — a model's reply as it is written, a command's output —
//! published on a topic, a thread, so whoever shows the topic follows it. A task of its own keeps each stream from its
//! start, by offset, as a log is read on from an offset, till its handler ends and the answer is in its process's
//! hands; a stream past its bound keeps no more, so what is kept never skips an event. A follower is given what each
//! stream of its topic kept, then what is published after, as it comes.

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::{broadcast, mpsc, oneshot};

/// How many bytes of one stream are kept at most.
pub const STREAM_BYTES_MAX: usize = 8 << 20;

/// How many events a follower may fall behind by before it reads what is kept again.
const LIVE_EVENTS_MAX: usize = 4096;

/// An event published on a topic for a request, by its offset in the request's stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub topic: Arc<str>,
    pub key: Arc<str>,
    pub offset: u64,
    pub data: Bytes,
}

/// The streams, reached through the task that keeps them.
#[derive(Debug, Clone)]
pub struct Streams(mpsc::UnboundedSender<Message>);

#[derive(Debug)]
enum Message {
    Publish {
        topic: Arc<str>,
        key: Arc<str>,
        data: Bytes,
    },
    End {
        key: Arc<str>,
    },
    Follow {
        reply: oneshot::Sender<(Vec<Published>, broadcast::Receiver<Published>)>,
        topic: Arc<str>,
    },
}

/// A stream: its topic, what was kept of it from offset 0, how many events were published, kept or not, the bytes
/// kept, and whether it passed its bound.
#[derive(Debug)]
struct Stream {
    topic: Arc<str>,
    kept: Vec<Bytes>,
    count: u64,
    bytes: usize,
    full: bool,
}

impl Streams {
    /// The streams, kept by a task of their own on the current runtime till every handle is let go of.
    #[must_use]
    pub fn start() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        tokio::spawn(keep(receiver));
        Self(sender)
    }

    /// Publishes `data` on `topic` to the stream of the request `key`.
    pub fn publish(&self, topic: &str, key: &str, data: Bytes) {
        let _ = self.0.send(Message::Publish {
            topic: topic.into(),
            key: key.into(),
            data,
        });
    }

    /// Lets go of the stream of `key`, once what was published on it before is kept.
    pub fn end(&self, key: &str) {
        let _ = self.0.send(Message::End { key: key.into() });
    }

    /// Follows `topic`: what each of its streams kept, then what is published after.
    pub async fn follow(&self, topic: &str) -> Following {
        let topic: Arc<str> = topic.into();
        let (kept, live) = self.followed(&topic).await;
        Following {
            streams: self.clone(),
            topic,
            kept: kept.into(),
            live,
            seen: HashMap::new(),
        }
    }

    async fn followed(&self, topic: &Arc<str>) -> (Vec<Published>, Option<broadcast::Receiver<Published>>) {
        let (reply, answer) = oneshot::channel();
        if self
            .0
            .send(Message::Follow {
                reply,
                topic: Arc::clone(topic),
            })
            .is_err()
        {
            return (Vec::new(), None);
        }
        answer.await.map_or((Vec::new(), None), |(kept, live)| (kept, Some(live)))
    }
}

/// Keeps the streams as the messages come, in the order they were sent, till every sender is gone.
async fn keep(mut messages: mpsc::UnboundedReceiver<Message>) {
    let (live, _) = broadcast::channel(LIVE_EVENTS_MAX);
    let mut streams: HashMap<Arc<str>, Stream> = HashMap::new();
    while let Some(message) = messages.recv().await {
        match message {
            Message::Publish { topic, key, data } => {
                let stream = streams.entry(Arc::clone(&key)).or_insert_with(|| Stream {
                    topic: Arc::clone(&topic),
                    kept: Vec::new(),
                    count: 0,
                    bytes: 0,
                    full: false,
                });
                let offset = stream.count;
                stream.count += 1;
                if !stream.full && stream.bytes + data.len() <= STREAM_BYTES_MAX {
                    stream.bytes += data.len();
                    stream.kept.push(data.clone());
                } else {
                    stream.full = true;
                }
                let _ = live.send(Published { topic, key, offset, data });
            }
            Message::End { key } => {
                streams.remove(&key);
            }
            Message::Follow { reply, topic } => {
                let kept = streams
                    .iter()
                    .filter(|(_, stream)| stream.topic == topic)
                    .flat_map(|(key, stream)| {
                        stream.kept.iter().zip(0..).map(|(data, offset)| Published {
                            topic: Arc::clone(&topic),
                            key: Arc::clone(key),
                            offset,
                            data: data.clone(),
                        })
                    })
                    .collect();
                let _ = reply.send((kept, live.subscribe()));
            }
        }
    }
}

/// A topic followed: each event once, by its stream's offset — what was kept first, then what comes.
#[derive(Debug)]
pub struct Following {
    streams: Streams,
    topic: Arc<str>,
    kept: std::collections::VecDeque<Published>,
    live: Option<broadcast::Receiver<Published>>,
    /// The next offset of each stream, so an event given once is not given again.
    seen: HashMap<Arc<str>, u64>,
}

impl Following {
    /// The next event of the topic; `None` once the streams are gone. A follower fallen too far behind reads what is
    /// kept again, so it misses only what a stream past its bound no longer keeps.
    pub async fn next(&mut self) -> Option<Published> {
        loop {
            let event = match self.kept.pop_front() {
                Some(event) => event,
                None => match self.live.as_mut()?.recv().await {
                    Ok(event) => event,
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let (kept, live) = self.streams.followed(&self.topic).await;
                        (self.kept, self.live) = (kept.into(), live);
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                },
            };
            if event.topic != self.topic {
                continue;
            }
            let next = self.seen.entry(Arc::clone(&event.key)).or_insert(0);
            if event.offset < *next {
                continue;
            }
            *next = event.offset + 1;
            return Some(event);
        }
    }
}
