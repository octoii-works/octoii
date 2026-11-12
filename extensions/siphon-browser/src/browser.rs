//! The browser of the threads: one headless Chromium, started as a page is first opened, spoken to over its DevTools
//! pipe, its profile in the folder the host gives so what the person signed in to stays. Each page is a target of its
//! own, a thread's: made held at its start and set up before it loads anything, as a debugger starts a process
//! suspended; laid out at the size its views ask; painted by the browser as it changes, the newest frame kept for those
//! that look at it, as a VNC server keeps its framebuffer. A page a page opens is a page of the same thread; one that
//! closes itself is gone. One task owns the pages, telling whoever listens as a thread's pages come, change and go, and
//! as a page is painted anew; what reads and acts on a page speaks to its target itself, through the pipe.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::Instant;

use base64::Engine as _;
use beak::{Exec, Kernel, Spec, Stdio};
use bytes::Bytes;
use chromiumoxide_cdp::cdp::{CdpEvent, CdpEventMessage};
use serde_json::{Value, json};
use siphon::machine::Base;
use siphon::machine::{Host, Machine, Writes};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::cdp::{Cdp, CdpError, connect};

/// The JPEG quality pages are painted at.
const FRAME_QUALITY: u32 = 70;
/// How many messages of its console a page keeps, and how many characters of each.
const MESSAGES_MAX: usize = 50;
const MESSAGE_CHARS_MAX: usize = 2000;
const EVENTS_MAX: usize = 1024;
/// How many of a page's own events it keeps for a script to take, the oldest let go of past them.
const EVENTS_KEPT: usize = 500;

/// The world of its own a page's title is watched from, out of the page's reach, and the binding it tells the title
/// through — as a document loads, as its title changes, and as it comes back from the back-forward cache.
const WORLD: &str = "octoii";
const TITLE_BINDING: &str = "octoiiTitle";
const TITLE_WATCH: &str = "(() => {
  let told = null;
  const tell = () => { if (document.title !== told) { told = document.title; octoiiTitle(told); } };
  new MutationObserver(tell).observe(document, { subtree: true, childList: true, characterData: true });
  addEventListener('DOMContentLoaded', tell);
  addEventListener('pageshow', () => { told = null; tell(); });
  tell();
})()";

/// Where the browser is: the machine it runs on, the program, by its path — one of the usual places where none — and
/// the folder its profile is kept in, one that lasts as long as it runs where none.
#[derive(Debug, Clone)]
pub struct BrowserHost {
    pub kernel: Kernel,
    pub machine: Host,
    pub program: Option<PathBuf>,
    pub profile: Option<PathBuf>,
    /// The folder what a page downloads is kept in; none, nothing is downloaded.
    pub downloads: Option<PathBuf>,
}

/// A view's size: CSS pixels across and down, and how many device pixels each CSS pixel is drawn with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

/// A dialog that holds a page's scripts till it is answered: its kind, its message, and the text a prompt offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    pub kind: String,
    pub message: String,
    pub offered: String,
}

/// A message of a page's console, or an error its script threw: its level, its text, and its number among the page's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub level: String,
    pub text: String,
    pub number: u64,
}

/// A page as a list of a thread's shows it: whether it loads, and whether its history goes back and forward from where
/// it is.
#[derive(Debug, Clone, PartialEq)]
pub struct PageView {
    pub id: String,
    pub thread: String,
    pub url: String,
    pub title: String,
    pub size: Size,
    pub dialog: Option<Dialog>,
    pub loading: bool,
    pub back: bool,
    pub forward: bool,
}

/// A frame of a page as the browser painted it: JPEG, its size in device pixels, and its number among the page's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub jpeg: Bytes,
    pub width: u32,
    pub height: u32,
    pub number: u64,
}

/// A download a page began: the browser's id of it, its address, the name it suggested, how far it got and where it
/// was kept once it was.
#[derive(Debug, Clone, PartialEq)]
pub struct Download {
    pub id: String,
    pub url: String,
    pub name: String,
    pub received_bytes: u64,
    pub total_bytes: u64,
    pub state: String,
    pub path: Option<String>,
}

/// How busy a page's network is: requests still in flight that began in the last 1.5 s — older ones taken as long
/// polls, not loading — and how long ago a request last began or ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Network {
    pub in_flight: usize,
    pub quiet_ms: u64,
}

/// What the browser tells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// The thread's pages changed: one came, moved, took a title, held a dialog, or went.
    Pages { thread: String },
    /// The page was painted anew.
    Painted { page: String, number: u64 },
}

/// Why a page could not be opened or reached.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BrowserError {
    #[error("no such page")]
    NoSuchPage,
    #[error("no browser on the machine")]
    NoBrowser,
    #[error("the browser is gone")]
    Gone,
    #[error(transparent)]
    Cdp(#[from] CdpError),
    #[error(transparent)]
    Beak(#[from] beak::Error),
    #[error("the browser's own folders: {0}")]
    Folders(String),
}

/// A page's target as what reads and acts on it speaks to it: the pipe, and the target's session.
#[derive(Debug, Clone)]
pub struct Attached {
    pub cdp: Cdp,
    pub session: String,
}

type Reply<T> = oneshot::Sender<Result<T, BrowserError>>;

#[derive(Debug)]
enum Ask {
    Open {
        thread: String,
        url: Option<String>,
        size: Size,
        reply: Reply<String>,
    },
    Close {
        page: String,
    },
    CloseThread {
        thread: String,
    },
    Pages {
        thread: String,
        reply: oneshot::Sender<Vec<PageView>>,
    },
    Attached {
        page: String,
        reply: Reply<Attached>,
    },
    Frame {
        page: String,
        reply: oneshot::Sender<Option<Frame>>,
    },
    Messages {
        page: String,
        after: u64,
        reply: oneshot::Sender<Vec<Message>>,
    },
    Resize {
        page: String,
        size: Size,
        reply: Reply<()>,
    },
    View {
        page: String,
        reply: oneshot::Sender<Option<PageView>>,
    },
    Drain {
        page: String,
        reply: oneshot::Sender<Vec<Value>>,
    },
    /// The session of the page's frame of another site's process whose address holds `part`.
    FrameSession {
        page: String,
        part: String,
        reply: oneshot::Sender<Option<String>>,
    },
    /// The refs the page's last reading named, and those this one names in their place.
    Refs {
        page: String,
        read: Vec<i64>,
        reply: oneshot::Sender<HashSet<i64>>,
    },
    Current {
        thread: String,
        reply: oneshot::Sender<Option<String>>,
    },
    Switch {
        thread: String,
        page: String,
        reply: Reply<()>,
    },
    Network {
        page: String,
        reply: oneshot::Sender<Network>,
    },
    Downloads {
        page: String,
        reply: oneshot::Sender<Vec<Download>>,
    },
}

/// The browser, reached through the task that owns its pages.
#[derive(Debug, Clone)]
pub struct Browser {
    asks: mpsc::UnboundedSender<Ask>,
    events: broadcast::Sender<Change>,
}

impl Browser {
    /// The browser `host` gives, started as a page is first opened, owned by a task on the current runtime till every
    /// handle is let go of.
    #[must_use]
    pub fn start(host: BrowserHost) -> Self {
        let (asks, inbox) = mpsc::unbounded_channel();
        let (events, _) = broadcast::channel(EVENTS_MAX);
        tokio::spawn(Owner::new(host, events.clone()).serve(inbox));
        Self { asks, events }
    }

    /// What the browser tells, from now on.
    #[must_use]
    pub fn events(&self) -> broadcast::Receiver<Change> {
        self.events.subscribe()
    }

    async fn ask<T>(&self, ask: impl FnOnce(Reply<T>) -> Ask) -> Result<T, BrowserError> {
        let (reply, answer) = oneshot::channel();
        self.asks.send(ask(reply)).map_err(|_| BrowserError::Gone)?;
        answer.await.map_err(|_| BrowserError::Gone)?
    }

    async fn get<T: Default>(&self, ask: impl FnOnce(oneshot::Sender<T>) -> Ask) -> T {
        let (reply, answer) = oneshot::channel();
        if self.asks.send(ask(reply)).is_err() {
            return T::default();
        }
        answer.await.unwrap_or_default()
    }

    /// Opens a page for `thread`, laid out at `size`, at `url` or `about:blank`: its id.
    ///
    /// # Errors
    /// `NoBrowser` where the machine has none; the browser's refusal.
    pub async fn open(&self, thread: &str, url: Option<&str>, size: Size) -> Result<String, BrowserError> {
        let (thread, url) = (thread.to_owned(), url.map(str::to_owned));
        self.ask(|reply| Ask::Open { thread, url, size, reply }).await
    }

    /// Closes the page `page`.
    pub fn close(&self, page: &str) {
        let _ = self.asks.send(Ask::Close { page: page.to_owned() });
    }

    /// Closes every page of `thread`, as it goes.
    pub fn close_thread(&self, thread: &str) {
        let _ = self.asks.send(Ask::CloseThread { thread: thread.to_owned() });
    }

    /// The pages of `thread`, in the order they were opened.
    pub async fn pages(&self, thread: &str) -> Vec<PageView> {
        let thread = thread.to_owned();
        self.get(|reply| Ask::Pages { thread, reply }).await
    }

    /// The page's target, to read and act on it — made again, at its address, where the browser that held it ended.
    ///
    /// # Errors
    /// `NoSuchPage`; the browser's refusal.
    pub async fn attached(&self, page: &str) -> Result<Attached, BrowserError> {
        let page = page.to_owned();
        self.ask(|reply| Ask::Attached { page, reply }).await
    }

    /// The page's newest frame, where it was painted.
    pub async fn frame(&self, page: &str) -> Option<Frame> {
        let page = page.to_owned();
        self.get(|reply| Ask::Frame { page, reply }).await
    }

    /// The messages of the page's console numbered past `after`, oldest first.
    pub async fn messages(&self, page: &str, after: u64) -> Vec<Message> {
        let page = page.to_owned();
        self.get(|reply| Ask::Messages { page, after, reply }).await
    }

    /// The page `page`, as a list shows it; none where there is no such page.
    pub async fn view(&self, page: &str) -> Option<PageView> {
        let page = page.to_owned();
        self.get(|reply| Ask::View { page, reply }).await
    }

    /// The page's own events since they were last taken, as they came, oldest first.
    pub async fn drain_events(&self, page: &str) -> Vec<Value> {
        let page = page.to_owned();
        self.get(|reply| Ask::Drain { page, reply }).await
    }

    /// The session of the page's frame of another site's process whose address holds `part`, where it has one.
    pub(crate) async fn frame_session(&self, page: &str, part: &str) -> Option<String> {
        let (page, part) = (page.to_owned(), part.to_owned());
        self.get(|reply| Ask::FrameSession { page, part, reply }).await
    }

    /// Keeps `read`, the refs a reading of the page named: those its reading before named.
    pub(crate) async fn swap_refs(&self, page: &str, read: Vec<i64>) -> HashSet<i64> {
        let page = page.to_owned();
        self.get(|reply| Ask::Refs { page, read, reply }).await
    }

    /// The page of `thread`'s the agent works, as a person works one tab at a time: the one it opened or switched to
    /// last, else the thread's newest.
    pub async fn current(&self, thread: &str) -> Option<String> {
        let thread = thread.to_owned();
        self.get(|reply| Ask::Current { thread, reply }).await
    }

    /// Has the agent of `thread` work the page `page` from now on.
    ///
    /// # Errors
    /// `NoSuchPage` for no page of the thread's.
    pub async fn switch(&self, thread: &str, page: &str) -> Result<(), BrowserError> {
        let (thread, page) = (thread.to_owned(), page.to_owned());
        self.ask(|reply| Ask::Switch { thread, page, reply }).await
    }

    /// How busy the page's network is.
    pub async fn network(&self, page: &str) -> Network {
        let page = page.to_owned();
        self.get(|reply| Ask::Network { page, reply }).await
    }

    /// What the page downloaded, or downloads, in the order it began.
    pub async fn downloads(&self, page: &str) -> Vec<Download> {
        let page = page.to_owned();
        self.get(|reply| Ask::Downloads { page, reply }).await
    }

    /// Lays the page out at `size`.
    ///
    /// # Errors
    /// `NoSuchPage`; the browser's refusal.
    pub async fn resize(&self, page: &str, size: Size) -> Result<(), BrowserError> {
        let page = page.to_owned();
        self.ask(|reply| Ask::Resize { page, size, reply }).await
    }
}

/// A page as its owner keeps it.
#[derive(Debug)]
struct Page {
    id: String,
    thread: String,
    url: String,
    title: String,
    size: Size,
    target: Option<String>,
    session: Option<String>,
    /// Where it is to go once its target is set up.
    going: Option<String>,
    dialog: Option<Dialog>,
    /// Whether its main frame loads, as the browser tells of it.
    loading: bool,
    /// Whether its history goes back and forward from where it is, as the browser told of it last.
    back: bool,
    forward: bool,
    frame: Option<Frame>,
    messages: VecDeque<Message>,
    heard: u64,
    /// The requests in flight, by the browser's id, with when each began; and when one last began or ended.
    in_flight: HashMap<String, Instant>,
    network_at: Instant,
    downloads: Vec<Download>,
    /// The newest events of its target, as they came, for a script to take.
    events: VecDeque<Value>,
    /// The refs its last reading named, so the next marks those new.
    refs: HashSet<i64>,
    /// Its frames of other sites' processes, as the browser attached to them: each target, its session and address.
    frames: Vec<FrameTarget>,
}

/// A frame of another site's process, attached to as the browser made it.
#[derive(Debug, Clone)]
struct FrameTarget {
    target: String,
    session: String,
    url: String,
}

/// The browser as it runs: its pipe, and what it tells.
struct Running {
    cdp: Cdp,
    events: mpsc::UnboundedReceiver<Value>,
    /// The machine it was started in, kept as long as it runs.
    _machine: Machine,
}

struct Owner {
    host: BrowserHost,
    running: Option<Running>,
    pages: Vec<Page>,
    /// The page each thread's agent works, by the thread.
    current: HashMap<String, String>,
    events: broadcast::Sender<Change>,
}

impl Owner {
    fn new(host: BrowserHost, events: broadcast::Sender<Change>) -> Self {
        Self {
            host,
            running: None,
            pages: Vec::new(),
            current: HashMap::new(),
            events,
        }
    }

    async fn serve(mut self, mut inbox: mpsc::UnboundedReceiver<Ask>) {
        loop {
            let event = async {
                match self.running.as_mut() {
                    Some(running) => running.events.recv().await,
                    None => std::future::pending().await,
                }
            };
            // What the browser told is heard before what is asked, so an answer is at least as new as what the
            // asker saw of the page.
            tokio::select! {
                biased;
                event = event => match event {
                    Some(event) => self.heard(event).await,
                    None => self.ended(),
                },
                ask = inbox.recv() => match ask {
                    Some(ask) => self.handle(ask).await,
                    None => return,
                },
            }
        }
    }

    async fn handle(&mut self, ask: Ask) {
        match ask {
            Ask::Open { thread, url, size, reply } => {
                let _ = reply.send(self.open(thread, url, size).await);
            }
            Ask::Close { page } => self.close(&page).await,
            Ask::CloseThread { thread } => {
                let ids: Vec<String> = self
                    .pages
                    .iter()
                    .filter(|page| page.thread == thread)
                    .map(|page| page.id.clone())
                    .collect();
                for id in ids {
                    self.close(&id).await;
                }
            }
            Ask::Pages { thread, reply } => {
                let _ = reply.send(self.pages.iter().filter(|page| page.thread == thread).map(view).collect());
            }
            Ask::Attached { page, reply } => {
                let _ = reply.send(self.attached(&page).await);
            }
            Ask::Frame { page, reply } => {
                let _ = reply.send(self.page(&page).and_then(|page| page.frame.clone()));
            }
            Ask::Messages { page, after, reply } => {
                let found = self
                    .page(&page)
                    .map(|page| page.messages.iter().filter(|message| message.number > after).cloned().collect());
                let _ = reply.send(found.unwrap_or_default());
            }
            Ask::Resize { page, size, reply } => {
                let _ = reply.send(self.resize(&page, size).await);
            }
            Ask::View { page, reply } => {
                let _ = reply.send(self.page(&page).map(view));
            }
            Ask::FrameSession { page, part, reply } => {
                let session = self.page(&page).and_then(|page| {
                    page.frames
                        .iter()
                        .find(|frame| frame.url.contains(&part))
                        .map(|frame| frame.session.clone())
                });
                let _ = reply.send(session);
            }
            Ask::Refs { page, read, reply } => {
                let before = self
                    .page_mut(&page)
                    .map(|page| std::mem::replace(&mut page.refs, read.into_iter().collect()));
                let _ = reply.send(before.unwrap_or_default());
            }
            Ask::Drain { page, reply } => {
                let drained = self.page_mut(&page).map(|page| page.events.drain(..).collect());
                let _ = reply.send(drained.unwrap_or_default());
            }
            Ask::Current { thread, reply } => {
                let current = self
                    .current
                    .get(&thread)
                    .filter(|page| self.page(page).is_some())
                    .cloned()
                    .or_else(|| {
                        self.pages
                            .iter()
                            .rev()
                            .find(|page| page.thread == thread)
                            .map(|page| page.id.clone())
                    });
                let _ = reply.send(current);
            }
            Ask::Switch { thread, page, reply } => {
                let theirs = self.page(&page).is_some_and(|found| found.thread == thread);
                if theirs {
                    self.current.insert(thread, page);
                }
                let _ = reply.send(if theirs { Ok(()) } else { Err(BrowserError::NoSuchPage) });
            }
            Ask::Network { page, reply } => {
                let busy = self.page(&page).map(|page| Network {
                    in_flight: page.in_flight.values().filter(|began| began.elapsed().as_millis() < 1500).count(),
                    quiet_ms: u64::try_from(page.network_at.elapsed().as_millis()).unwrap_or(u64::MAX),
                });
                let _ = reply.send(busy.unwrap_or_default());
            }
            Ask::Downloads { page, reply } => {
                let _ = reply.send(self.page(&page).map(|page| page.downloads.clone()).unwrap_or_default());
            }
        }
    }

    fn page(&self, id: &str) -> Option<&Page> {
        self.pages.iter().find(|page| page.id == id)
    }

    fn page_mut(&mut self, id: &str) -> Option<&mut Page> {
        self.pages.iter_mut().find(|page| page.id == id)
    }

    /// Asks the browser where the history of the page `id` goes from where it is — back where it has entries before, forward
    /// where after — kept and told where it changed.
    async fn history(&mut self, id: &str) {
        let (Some(running), Some(session)) = (&self.running, self.page(id).and_then(|page| page.session.clone())) else {
            return;
        };
        let Ok(history) = running.cdp.call(Some(&session), "Page.getNavigationHistory", json!({})).await else {
            return;
        };
        let at = history["currentIndex"].as_i64().unwrap_or_default();
        let entries = history["entries"].as_array().map_or(0, Vec::len);
        let (back, forward) = (at > 0, usize::try_from(at + 1).is_ok_and(|next| next < entries));
        let Some(page) = self.page_mut(id) else { return };
        if (page.back, page.forward) == (back, forward) {
            return;
        }
        (page.back, page.forward) = (back, forward);
        let thread = page.thread.clone();
        self.told(&thread);
    }

    fn told(&self, thread: &str) {
        let _ = self.events.send(Change::Pages { thread: thread.to_owned() });
    }

    /// The browser's pipe, the browser started where it is not running.
    async fn cdp(&mut self) -> Result<Cdp, BrowserError> {
        if let Some(running) = &self.running {
            return Ok(running.cdp.clone());
        }
        let running = launch(&self.host)?;
        let cdp = running.cdp.clone();
        cdp.call(None, "Target.setDiscoverTargets", json!({ "discover": true })).await?;
        let attach = json!({ "autoAttach": true, "waitForDebuggerOnStart": true, "flatten": true });
        cdp.call(None, "Target.setAutoAttach", attach).await?;
        let downloads = match &self.host.downloads {
            Some(folder) => json!({ "behavior": "allowAndName", "downloadPath": folder, "eventsEnabled": true }),
            None => json!({ "behavior": "deny", "eventsEnabled": true }),
        };
        cdp.call(None, "Browser.setDownloadBehavior", downloads).await?;
        self.running = Some(running);
        Ok(cdp)
    }

    async fn open(&mut self, thread: String, url: Option<String>, size: Size) -> Result<String, BrowserError> {
        let cdp = self.cdp().await?;
        let id = self.host.kernel.new_id(now_ms())?;
        self.pages.push(Page {
            id: id.clone(),
            thread: thread.clone(),
            url: url.clone().unwrap_or_else(|| "about:blank".into()),
            title: String::new(),
            size,
            target: None,
            session: None,
            going: url,
            dialog: None,
            loading: false,
            back: false,
            forward: false,
            frame: None,
            messages: VecDeque::new(),
            heard: 0,
            in_flight: HashMap::new(),
            network_at: Instant::now(),
            downloads: Vec::new(),
            events: VecDeque::new(),
            refs: HashSet::new(),
            frames: Vec::new(),
        });
        self.create(&cdp, &id).await?;
        self.current.insert(thread.clone(), id.clone());
        self.told(&thread);
        Ok(id)
    }

    /// Asks the browser for a target for the page `id`, held at its start till it is set up as it attaches.
    async fn create(&mut self, cdp: &Cdp, id: &str) -> Result<(), BrowserError> {
        let made = cdp.call(None, "Target.createTarget", json!({ "url": "about:blank" })).await?;
        let target = made.get("targetId").and_then(Value::as_str).unwrap_or_default().to_owned();
        if let Some(page) = self.page_mut(id) {
            page.target = Some(target);
        }
        Ok(())
    }

    async fn attached(&mut self, id: &str) -> Result<Attached, BrowserError> {
        let cdp = self.cdp().await?;
        let page = self.page(id).ok_or(BrowserError::NoSuchPage)?;
        if page.target.is_none() {
            // Made again where it was, as the browser that held it ended.
            let url = page.url.clone();
            if let Some(page) = self.page_mut(id) {
                page.going = Some(url).filter(|url| url != "about:blank");
            }
            self.create(&cdp, id).await?;
        }
        // The target is set up as the browser tells it attached; what came meanwhile is heard first.
        for _ in 0..200 {
            if let Some(session) = self.page(id).and_then(|page| page.session.clone()) {
                return Ok(Attached { cdp, session });
            }
            let Some(running) = self.running.as_mut() else {
                return Err(BrowserError::Gone);
            };
            match tokio::time::timeout(std::time::Duration::from_millis(50), running.events.recv()).await {
                Ok(Some(event)) => self.heard(event).await,
                Ok(None) => {
                    self.ended();
                    return Err(BrowserError::Gone);
                }
                Err(_) => {}
            }
        }
        Err(BrowserError::Gone)
    }

    async fn resize(&mut self, id: &str, size: Size) -> Result<(), BrowserError> {
        let page = self.page_mut(id).ok_or(BrowserError::NoSuchPage)?;
        page.size = size;
        if let (Some(session), Some(running)) = (page.session.clone(), &self.running) {
            lay_out(&running.cdp, &session, size).await?;
        }
        Ok(())
    }

    async fn close(&mut self, id: &str) {
        let Some(at) = self.pages.iter().position(|page| page.id == id) else {
            return;
        };
        let page = self.pages.remove(at);
        if let (Some(target), Some(running)) = (&page.target, &self.running) {
            let _ = running.cdp.call(None, "Target.closeTarget", json!({ "targetId": target })).await;
        }
        self.told(&page.thread);
    }

    /// The browser ended: every page is left without its target, made again as it is next reached.
    fn ended(&mut self) {
        self.running = None;
        for page in &mut self.pages {
            page.target = None;
            page.session = None;
            page.dialog = None;
            page.in_flight.clear();
        }
    }

    async fn heard(&mut self, raw: Value) {
        let session = raw["sessionId"].as_str().map(str::to_owned);
        if let Some(session) = &session
            && let Some(page) = self.pages.iter_mut().find(|page| page.session.as_deref() == Some(session.as_str()))
        {
            page.events.push_back(raw.clone());
            while page.events.len() > EVENTS_KEPT {
                page.events.pop_front();
            }
        }
        let Ok(message) = serde_json::from_value::<CdpEventMessage>(raw) else {
            return;
        };
        match message.params {
            CdpEvent::TargetAttachedToTarget(event) => match message.session_id.as_deref() {
                Some(parent) if event.target_info.r#type == "iframe" => self.attach_frame(parent, &event).await,
                _ => self.attach(&event).await,
            },
            CdpEvent::TargetTargetInfoChanged(event) => {
                let info = &event.target_info;
                for frame in self.pages.iter_mut().flat_map(|page| page.frames.iter_mut()) {
                    if frame.target == info.target_id.as_ref() {
                        frame.url.clone_from(&info.url);
                    }
                }
                let Some(page) = self
                    .pages
                    .iter_mut()
                    .find(|page| page.target.as_deref() == Some(info.target_id.as_ref()))
                else {
                    return;
                };
                // Its address as the browser tells it; its title as its own watch does; where its history goes, asked anew.
                page.url.clone_from(&info.url);
                let (id, thread) = (page.id.clone(), page.thread.clone());
                self.told(&thread);
                self.history(&id).await;
            }
            CdpEvent::TargetDetachedFromTarget(event) => {
                for page in &mut self.pages {
                    page.frames.retain(|frame| frame.session != event.session_id.as_ref());
                }
            }
            CdpEvent::TargetTargetDestroyed(event) => {
                for page in &mut self.pages {
                    page.frames.retain(|frame| frame.target != event.target_id.as_ref());
                }
                let gone = self
                    .pages
                    .iter()
                    .find(|page| page.target.as_deref() == Some(event.target_id.as_ref()))
                    .map(|page| page.id.clone());
                if let Some(id) = gone {
                    let thread = self.page(&id).map(|page| page.thread.clone()).unwrap_or_default();
                    self.pages.retain(|page| page.id != id);
                    self.told(&thread);
                }
            }
            event @ (CdpEvent::BrowserDownloadWillBegin(_) | CdpEvent::BrowserDownloadProgress(_)) => self.heard_download(event),
            other => {
                if let Some(session) = session {
                    self.heard_of_page(&session, other).await;
                }
            }
        }
    }

    /// Hears of a download: one a page began, by its main frame, which has its target's id; how far one got.
    fn heard_download(&mut self, event: CdpEvent) {
        match event {
            CdpEvent::BrowserDownloadWillBegin(began) => {
                // A page's main frame has its target's id.
                let Some(page) = self
                    .pages
                    .iter_mut()
                    .find(|page| page.target.as_deref() == Some(began.frame_id.as_ref()))
                else {
                    return;
                };
                page.downloads.push(Download {
                    id: began.guid.clone(),
                    url: began.url.clone(),
                    name: began.suggested_filename.clone(),
                    received_bytes: 0,
                    total_bytes: 0,
                    state: "inProgress".into(),
                    path: None,
                });
                let thread = page.thread.clone();
                self.told(&thread);
            }
            CdpEvent::BrowserDownloadProgress(progress) => {
                let Some(page) = self
                    .pages
                    .iter_mut()
                    .find(|page| page.downloads.iter().any(|download| download.id == progress.guid))
                else {
                    return;
                };
                let Some(download) = page.downloads.iter_mut().find(|download| download.id == progress.guid) else {
                    return;
                };
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (received, total) = (progress.received_bytes as u64, progress.total_bytes as u64);
                (download.received_bytes, download.total_bytes) = (received, total);
                let state = progress.state.as_ref().to_owned();
                let ended = download.state != state;
                download.state = state;
                download.path.clone_from(&progress.file_path);
                if ended {
                    let thread = page.thread.clone();
                    self.told(&thread);
                }
            }
            _ => {}
        }
    }

    /// Hears an event of a page's target: a frame painted, a dialog opened or closed, a message of its console.
    async fn heard_of_page(&mut self, session: &str, event: CdpEvent) {
        let Some(page) = self.pages.iter_mut().find(|page| page.session.as_deref() == Some(session)) else {
            return;
        };
        match event {
            CdpEvent::PageScreencastFrame(frame) => {
                let number = painted(page, &frame);
                let id = page.id.clone();
                if let Some(running) = &self.running {
                    let _ = running
                        .cdp
                        .call(Some(session), "Page.screencastFrameAck", json!({ "sessionId": frame.session_id }))
                        .await;
                }
                let _ = self.events.send(Change::Painted { page: id, number });
            }
            CdpEvent::PageJavascriptDialogOpening(dialog) => {
                page.dialog = Some(Dialog {
                    kind: serde_json::to_value(&dialog.r#type)
                        .ok()
                        .and_then(|kind| kind.as_str().map(str::to_owned))
                        .unwrap_or_default(),
                    message: dialog.message.clone(),
                    offered: dialog.default_prompt.clone().unwrap_or_default(),
                });
                let thread = page.thread.clone();
                self.told(&thread);
            }
            CdpEvent::NetworkRequestWillBeSent(sent) => {
                let streams = sent
                    .r#type
                    .as_ref()
                    .is_some_and(|kind| matches!(kind.as_ref(), "WebSocket" | "EventSource"));
                if !streams {
                    page.in_flight.insert(sent.request_id.as_ref().to_owned(), Instant::now());
                }
                page.network_at = Instant::now();
            }
            CdpEvent::NetworkLoadingFinished(done) => {
                page.in_flight.remove(done.request_id.as_ref());
                page.network_at = Instant::now();
            }
            CdpEvent::NetworkLoadingFailed(failed) => {
                page.in_flight.remove(failed.request_id.as_ref());
                page.network_at = Instant::now();
            }
            CdpEvent::RuntimeBindingCalled(called) if called.name == TITLE_BINDING => {
                page.title.clone_from(&called.payload);
                let thread = page.thread.clone();
                self.told(&thread);
            }
            CdpEvent::PageFrameStartedLoading(started) if page.target.as_deref() == Some(started.frame_id.as_ref()) => {
                page.loading = true;
                let thread = page.thread.clone();
                self.told(&thread);
            }
            CdpEvent::PageFrameStoppedLoading(stopped) if page.target.as_deref() == Some(stopped.frame_id.as_ref()) => {
                page.loading = false;
                let thread = page.thread.clone();
                self.told(&thread);
            }
            CdpEvent::PageJavascriptDialogClosed(_) => {
                page.dialog = None;
                let thread = page.thread.clone();
                self.told(&thread);
            }
            CdpEvent::RuntimeConsoleApiCalled(called) => {
                let level = serde_json::to_value(&called.r#type)
                    .ok()
                    .and_then(|kind| kind.as_str().map(str::to_owned))
                    .unwrap_or_default();
                let words: Vec<String> = called
                    .args
                    .iter()
                    .map(|arg| {
                        arg.value.as_ref().map_or_else(
                            || arg.description.clone().unwrap_or_default(),
                            |value| value.as_str().map_or_else(|| value.to_string(), str::to_owned),
                        )
                    })
                    .collect();
                keep(page, level, &words.join(" "));
            }
            CdpEvent::RuntimeExceptionThrown(thrown) => {
                let details = &thrown.exception_details;
                let text = details
                    .exception
                    .as_ref()
                    .and_then(|exception| exception.description.clone())
                    .unwrap_or_else(|| details.text.clone());
                keep(page, "error".into(), &text);
            }
            _ => {}
        }
    }

    /// Sets up a target the browser attached to, held at its start: the page that asked for it, or one a page of a
    /// thread's opened, made a page of that thread; any other let run and let be.
    /// Sets up a frame of another site's process a page's target attached to, held at its start: its scripts heard,
    /// its own frames attached to in turn, then let run — kept by the page whose session it came through.
    async fn attach_frame(&mut self, parent: &str, event: &chromiumoxide_cdp::cdp::browser_protocol::target::EventAttachedToTarget) {
        let Some(running) = &self.running else { return };
        let cdp = running.cdp.clone();
        let session = event.session_id.as_ref().to_owned();
        let owner = self
            .pages
            .iter_mut()
            .find(|page| page.session.as_deref() == Some(parent) || page.frames.iter().any(|frame| frame.session == parent));
        if let Some(page) = owner {
            page.frames.push(FrameTarget {
                target: event.target_info.target_id.as_ref().to_owned(),
                session: session.clone(),
                url: event.target_info.url.clone(),
            });
        }
        let _ = cdp.call(Some(&session), "Runtime.enable", json!({})).await;
        let _ = cdp
            .call(
                Some(&session),
                "Target.setAutoAttach",
                json!({ "autoAttach": true, "waitForDebuggerOnStart": true, "flatten": true }),
            )
            .await;
        let _ = cdp.call(Some(&session), "Runtime.runIfWaitingForDebugger", json!({})).await;
    }

    async fn attach(&mut self, event: &chromiumoxide_cdp::cdp::browser_protocol::target::EventAttachedToTarget) {
        let Some(running) = &self.running else { return };
        let cdp = running.cdp.clone();
        let info = &event.target_info;
        let session = event.session_id.as_ref().to_owned();
        let target = info.target_id.as_ref().to_owned();
        let opened_by = info
            .opener_id
            .as_ref()
            .and_then(|opener| self.pages.iter().find(|page| page.target.as_deref() == Some(opener.as_ref())))
            .map(|page| (page.thread.clone(), page.size));
        if info.r#type == "page"
            && !self.pages.iter().any(|page| page.target.as_deref() == Some(target.as_str()))
            && let Some((thread, size)) = opened_by
        {
            let Ok(id) = self.host.kernel.new_id(now_ms()) else { return };
            self.pages.push(Page {
                id,
                thread,
                url: info.url.clone(),
                title: info.title.clone(),
                size,
                target: Some(target.clone()),
                session: None,
                going: None,
                dialog: None,
                loading: false,
                back: false,
                forward: false,
                frame: None,
                messages: VecDeque::new(),
                heard: 0,
                in_flight: HashMap::new(),
                network_at: Instant::now(),
                downloads: Vec::new(),
                events: VecDeque::new(),
                refs: HashSet::new(),
                frames: Vec::new(),
            });
        }
        let ours = self.pages.iter().position(|page| page.target.as_deref() == Some(target.as_str()));
        if let Some(at) = ours {
            let (size, going) = (self.pages[at].size, self.pages[at].going.take());
            if set_up(&cdp, &session, size).await.is_ok() {
                self.pages[at].session = Some(session.clone());
            }
            let _ = cdp.call(Some(&session), "Runtime.runIfWaitingForDebugger", json!({})).await;
            if let Some(url) = going {
                let _ = cdp.call(Some(&session), "Page.navigate", json!({ "url": url })).await;
            }
            let thread = self.pages[at].thread.clone();
            self.told(&thread);
        } else if event.waiting_for_debugger {
            let _ = cdp.call(Some(&session), "Runtime.runIfWaitingForDebugger", json!({})).await;
        }
    }
}

fn keep(page: &mut Page, level: String, text: &str) {
    page.heard += 1;
    page.messages.push_back(Message {
        level,
        text: text.chars().take(MESSAGE_CHARS_MAX).collect(),
        number: page.heard,
    });
    while page.messages.len() > MESSAGES_MAX {
        page.messages.pop_front();
    }
}

/// Keeps `frame` as the page's newest, as the browser painted it: its number among the page's.
fn painted(page: &mut Page, frame: &chromiumoxide_cdp::cdp::browser_protocol::page::EventScreencastFrame) -> u64 {
    let jpeg = base64::engine::general_purpose::STANDARD
        .decode(AsRef::<str>::as_ref(&frame.data))
        .unwrap_or_default();
    let number = page.frame.as_ref().map_or(1, |frame| frame.number + 1);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (width, height) = (frame.metadata.device_width as u32, frame.metadata.device_height as u32);
    page.frame = Some(Frame {
        jpeg: jpeg.into(),
        width,
        height,
        number,
    });
    number
}

fn view(page: &Page) -> PageView {
    PageView {
        id: page.id.clone(),
        thread: page.thread.clone(),
        url: page.url.clone(),
        title: page.title.clone(),
        size: page.size,
        dialog: page.dialog.clone(),
        loading: page.loading,
        back: page.back,
        forward: page.forward,
    }
}

/// Sets up a page's target before it runs: its page, its scripts, its accessibility and its network heard, its title watched, laid
/// out at `size`, and painted as it changes.
async fn set_up(cdp: &Cdp, session: &str, size: Size) -> Result<(), CdpError> {
    for domain in [
        "Page.enable",
        "Runtime.enable",
        "DOM.enable",
        "Accessibility.enable",
        "Network.enable",
    ] {
        cdp.call(Some(session), domain, json!({})).await?;
    }
    cdp.call(
        Some(session),
        "Runtime.addBinding",
        json!({ "name": TITLE_BINDING, "executionContextName": WORLD }),
    )
    .await?;
    let watch = json!({ "source": TITLE_WATCH, "worldName": WORLD, "runImmediately": true });
    cdp.call(Some(session), "Page.addScriptToEvaluateOnNewDocument", watch).await?;
    lay_out(cdp, session, size).await?;
    // Its frames of other sites' processes attached to as they are made, held at their start, as the page itself was.
    let frames = json!({ "autoAttach": true, "waitForDebuggerOnStart": true, "flatten": true });
    cdp.call(Some(session), "Target.setAutoAttach", frames).await?;
    let screencast = json!({ "format": "jpeg", "quality": FRAME_QUALITY, "everyNthFrame": 1 });
    cdp.call(Some(session), "Page.startScreencast", screencast).await?;
    Ok(())
}

async fn lay_out(cdp: &Cdp, session: &str, size: Size) -> Result<(), CdpError> {
    let metrics = json!({ "width": size.width, "height": size.height, "deviceScaleFactor": size.scale, "mobile": false });
    cdp.call(Some(session), "Emulation.setDeviceMetricsOverride", metrics).await?;
    Ok(())
}

/// The places a browser is looked for where the host names none.
const PROGRAMS: [&str; 5] = [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
];

/// Starts the browser `host` gives, headless, its DevTools on the pipe of descriptors 3 and 4.
fn launch(host: &BrowserHost) -> Result<Running, BrowserError> {
    let program = host
        .program
        .clone()
        .or_else(|| PROGRAMS.iter().map(PathBuf::from).find(|path| path.exists()))
        .ok_or(BrowserError::NoBrowser)?;
    let mut argv = vec![
        program.to_string_lossy().into_owned(),
        "--headless=new".into(),
        "--remote-debugging-pipe".into(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--allow-pre-commit-input".into(),
        "--disable-background-timer-throttling".into(),
        "--disable-backgrounding-occluded-windows".into(),
        "--disable-renderer-backgrounding".into(),
        "--use-mock-keychain".into(),
        "about:blank".into(),
    ];
    if let Some(profile) = &host.profile {
        argv.insert(1, format!("--user-data-dir={}", profile.display()));
    }
    let home = host.machine.home.clone().unwrap_or_else(|| PathBuf::from("/"));
    let base = Base {
        workspace: home.clone(),
        home: Some(home.clone()),
    };
    // The browser keeps the pages it runs in a sandbox of its own, a process for each site, which cannot start inside
    // another — macOS lets no sandbox nest — so it runs on the machine as it is, nothing of the host's hidden from it.
    let open = Host {
        hidden: Vec::new(),
        ..host.machine.clone()
    };
    let machine = Machine::open(&host.kernel, &open, base, &Writes::Anywhere, None)?;
    for own in host.profile.iter().chain(&host.downloads) {
        std::fs::create_dir_all(own).map_err(|error| BrowserError::Folders(error.to_string()))?;
    }
    let spec = Spec {
        stdio: Stdio::Channel,
        ..Spec::new(Exec::Argv(argv), home.to_string_lossy())
    };
    let (command, told) = host.kernel.spawn(now_ms(), b"", machine.namespace(), &spec)?;
    let (cdp, events) = connect(command, told);
    Ok(Running {
        cdp,
        events,
        _machine: machine,
    })
}

fn now_ms() -> i64 {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
}
