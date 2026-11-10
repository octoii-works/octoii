//! The queue every object is bound to, and the task that routes what it tells: each completion, taken from Beak's
//! queue as its descriptor reads ready — or, where the system has none, as a thread waits on it — sent to the object
//! whose tag it carries.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use beak_sys as sys;
use bytes::Bytes;
use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::ffi::{check, copied, object};
use crate::{Exit, Result, Signal};

/// What happened, as the binding holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Told {
    Committed(u64),
    Request {
        process: Bytes,
        key: Bytes,
        name: Bytes,
        payload: Bytes,
        started: u64,
        retold: bool,
    },
    Interrupted {
        process: Bytes,
        key: Bytes,
    },
    Limit {
        process: Bytes,
        name: Bytes,
    },
    Signalled {
        process: Bytes,
        signal: Signal,
    },
    Output(Bytes),
    Title(String),
    Exit(Exit),
}

impl Told {
    /// What a completion tells, copied out of it.
    fn of(completion: &sys::beak_completion) -> Self {
        use sys::beak_event_Tag as Tag;
        let event = &completion.event;
        // SAFETY: the tag says which of the union's members was written.
        unsafe {
            let body = &event.__bindgen_anon_1;
            match event.tag {
                Tag::BEAK_EVENT_COMMITTED => Self::Committed(body.COMMITTED.seq),
                Tag::BEAK_EVENT_REQUEST => {
                    let request = &body.REQUEST;
                    Self::Request {
                        process: copied(&request.process),
                        key: copied(&request.key),
                        name: copied(&request.name),
                        payload: copied(&request.payload),
                        started: request.started,
                        retold: request.retold,
                    }
                }
                Tag::BEAK_EVENT_INTERRUPTED => Self::Interrupted {
                    process: copied(&body.INTERRUPTED.process),
                    key: copied(&body.INTERRUPTED.key),
                },
                Tag::BEAK_EVENT_LIMIT => Self::Limit {
                    process: copied(&body.LIMIT.process),
                    name: copied(&body.LIMIT.name),
                },
                Tag::BEAK_EVENT_SIGNALLED => Self::Signalled {
                    process: copied(&body.SIGNALLED.process),
                    signal: Signal::of(body.SIGNALLED.signal),
                },
                Tag::BEAK_EVENT_OUTPUT => Self::Output(copied(&body.OUTPUT.bytes)),
                Tag::BEAK_EVENT_TITLE => Self::Title(String::from_utf8_lossy(&copied(&body.TITLE.title)).into_owned()),
                Tag::BEAK_EVENT_EXIT => Self::Exit(Exit::of(body.EXIT.exit)),
            }
        }
    }
}

/// Beak's queue, let go of as the last holder goes.
pub(crate) struct Raw(pub(crate) *mut sys::beak_queue);

// SAFETY: a queue is used from any thread at once, behind its own lock.
unsafe impl Send for Raw {}
// SAFETY: as above.
unsafe impl Sync for Raw {}

impl Drop for Raw {
    fn drop(&mut self) {
        // SAFETY: the queue Beak gave, let go of once.
        unsafe { sys::beak_queue_free(self.0) };
    }
}

/// The queue, and where what it tells goes, by tag.
pub(crate) struct Router {
    pub(crate) raw: Raw,
    routes: Mutex<HashMap<u64, mpsc::UnboundedSender<Told>>>,
    next_tag: AtomicU64,
}

/// How many completions one take moves at most.
const TAKE_MAX: usize = 64;

impl Router {
    /// Opens a queue and starts routing what it tells, on the current Tokio runtime.
    pub(crate) fn start() -> Result<Arc<Self>> {
        // SAFETY: the call is given where to put the queue.
        let raw = Raw(object(|out| unsafe { sys::beak_queue_open(out) })?);
        let router = Arc::new(Self {
            raw,
            routes: Mutex::new(HashMap::new()),
            next_tag: AtomicU64::new(1),
        });
        route(Arc::downgrade(&router));
        Ok(router)
    }

    /// A new tag, and what it tells.
    pub(crate) fn bind(&self) -> (u64, mpsc::UnboundedReceiver<Told>) {
        let tag = self.next_tag.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::unbounded_channel();
        self.routes.lock().insert(tag, sender);
        (tag, receiver)
    }

    /// Takes what waits and sends each where its tag goes: whether a take came back full, more waiting.
    fn take(&self) -> Result<bool> {
        let mut taken: Vec<std::mem::MaybeUninit<sys::beak_completion>> = (0..TAKE_MAX).map(|_| std::mem::MaybeUninit::uninit()).collect();
        let mut count = 0;
        // SAFETY: room for `TAKE_MAX`.
        check(unsafe { sys::beak_queue_take(self.raw.0, taken.as_mut_ptr().cast(), TAKE_MAX, &raw mut count) })?;
        let mut routes = self.routes.lock();
        for each in taken.iter_mut().take(count) {
            let completion = each.as_mut_ptr();
            // SAFETY: the first `count` were written; each is let go of once, after it is read.
            let (tag, told) = unsafe { ((*completion).tag, Told::of(&*completion)) };
            // SAFETY: as above.
            unsafe { sys::beak_completion_free(completion) };
            let ends = matches!(told, Told::Exit(_));
            if let Some(route) = routes.get(&tag)
                && (route.send(told).is_err() || ends)
            {
                routes.remove(&tag);
            }
        }
        Ok(count == TAKE_MAX)
    }
}

/// Routes what the queue tells till the router goes.
#[cfg(unix)]
fn route(router: std::sync::Weak<Router>) {
    use std::os::fd::{AsRawFd, RawFd};
    struct Ready(RawFd);
    impl AsRawFd for Ready {
        fn as_raw_fd(&self) -> RawFd {
            self.0
        }
    }
    let Some(held) = router.upgrade() else { return };
    // SAFETY: the queue is open while the router is.
    let fd = unsafe { sys::beak_queue_fd(held.raw.0) };
    drop(held);
    tokio::spawn(async move {
        // SAFETY: the descriptor is the queue's, open while the router is, and this task ends as the router goes.
        let Ok(ready) = (unsafe { tokio::io::unix::AsyncFd::register_with_interest(Ready(fd), tokio::io::Interest::READABLE) }) else {
            return;
        };
        loop {
            let Ok(mut guard) = ready.readable().await else { return };
            let Some(router) = router.upgrade() else { return };
            while let Ok(true) = router.take() {}
            guard.clear_ready();
        }
    });
}

/// Routes what the queue tells till the router goes, a thread waiting on it.
#[cfg(not(unix))]
fn route(router: std::sync::Weak<Router>) {
    std::thread::spawn(move || {
        loop {
            let Some(router) = router.upgrade() else { return };
            let mut ready = false;
            // SAFETY: the queue is open while the router is.
            if check(unsafe { sys::beak_queue_wait(router.raw.0, 100, &raw mut ready) }).is_err() {
                return;
            }
            while let Ok(true) = router.take() {}
        }
    });
}
