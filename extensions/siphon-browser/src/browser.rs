//! One task owns the threads' pages. A page keeps the DevTools target of the renderer that opened it: the local
//! desktop or the node's headless Chromium. Tools, input and screenshots all use that target. The owner publishes
//! a thread's pages and keeps their newest frame for clients that view them over the network.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::Instant;

use base64::Engine as _;
use bytes::Bytes;
use chromiumoxide_cdp::cdp::browser_protocol::page::EventScreencastFrame;
use chromiumoxide_cdp::cdp::{CdpEvent, CdpEventMessage};
use octoii::machine::Base;
use octoii::machine::{Commands, Host, Machine, Writes};
use octoii::machine::{Exec, Spec, Stdio};
use octoii::threads::ThreadData;
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinSet;

use crate::cdp::{Cdp, CdpError, connect, connect_desktop};
use crate::files::Upload;
use crate::protocol::{
    DevToolsState, PageFileChooser, PageHost as Runtime, PageLoad, PagePlacement, PagePresentation, PageReference, ViewportMode,
};
use url::Url;

mod navigation;

use navigation::DocumentChange;

/// The JPEG quality pages are painted at.
const FRAME_QUALITY: u32 = 92;
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
    pub commands: Commands,
    pub machine: Host,
    pub program: Option<PathBuf>,
    pub profile: Option<PathBuf>,
    /// The folder what a page downloads is kept in; none, nothing is downloaded.
    pub downloads: Option<PathBuf>,
    /// Where a thread remembers the addresses of its pages.
    pub references: Option<ThreadData>,
}

/// A view's size: CSS pixels across and down, and how many device pixels each CSS pixel is drawn with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

impl Size {
    fn zoomed(self, percent: u16, host: Runtime) -> Self {
        if percent == 100 {
            return self;
        }
        let percent = u32::from(percent);
        let css = |dimension: u32| (dimension.saturating_mul(100).saturating_add(percent / 2) / percent).max(1);
        Self {
            width: css(self.width),
            height: css(self.height),
            scale: match host {
                // The guest's compositor paints at the display's density. A larger capture at an overridden
                // density repeats the surface when CDP reads past its bounds.
                Runtime::Desktop => self.scale,
                Runtime::Headless => (self.scale * f64::from(percent) / 100.0).max(1.0),
            },
        }
    }
}

/// A dialog that holds a page's scripts till it is answered: its kind, its message, and the text a prompt offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    pub kind: String,
    pub message: String,
    pub offered: String,
}

/// A message of a page's console, or an error its script threw: its level, its text, and its number among the page's.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
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
    pub viewport_size: Size,
    pub viewport: ViewportMode,
    pub zoom: u16,
    pub host: Runtime,
    pub dev_tools: DevToolsState,
    pub downloads: Vec<crate::protocol::PageDownload>,
    pub chooser: Option<PageFileChooser>,
    pub dialog: Option<Dialog>,
    pub load: PageLoad,
    pub back: bool,
    pub forward: bool,
    pub(crate) controller: Option<String>,
}

/// A frame's JPEG, the viewport in CSS pixels, and the frame number. The JPEG carries its own raster dimensions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub jpeg: Bytes,
    pub width: u32,
    pub height: u32,
    pub number: u64,
}

/// A download a page began: the browser's id of it, its address, the name it suggested, how far it got and where it
/// was kept once it was.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Download {
    pub id: String,
    pub url: String,
    pub name: String,
    pub received_bytes: u64,
    pub total_bytes: u64,
    pub state: String,
    pub path: Option<String>,
}

#[derive(serde::Deserialize)]
struct DesktopDownload {
    page: String,
    #[serde(flatten)]
    download: Download,
}

#[derive(Debug)]
pub(crate) struct SavedDownload {
    pub(crate) thread: String,
    pub(crate) name: String,
    pub(crate) path: PathBuf,
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
    #[error("there is already a desktop browser connected")]
    DesktopAttached,
    #[error("this page is controlled by someone else")]
    PageControlled,
    #[error("an action on this page is still in progress")]
    PageBusy,
    #[error("the page must be open in your browser before you can take control")]
    PageNotViewed,
    #[error("change this viewport in the desktop that renders the page")]
    DesktopViewport,
    #[error("viewport dimensions must be between 1 and 4096 pixels")]
    InvalidViewport,
    #[error("page zoom must be between 50 and 200 percent")]
    InvalidZoom,
    #[error("the file chooser changed or closed")]
    ChooserChanged,
    #[error("the page disconnected. Retry it to open a new browser target")]
    Disconnected,
    #[error("reopen this page on the desktop that hosts it")]
    NeedsDesktop,
    #[error("browser file transfer: {0}")]
    FileTransfer(String),
    #[error(transparent)]
    Cdp(#[from] CdpError),
    #[error(transparent)]
    Beak(#[from] octoii::volume::Error),
    #[error("the browser's own folders: {0}")]
    Folders(String),
}

/// A page's target as what reads and acts on it speaks to it: the pipe, and the target's session.
#[derive(Debug, Clone)]
pub struct Attached {
    pub cdp: Cdp,
    pub session: String,
}

/// DevTools messages exchanged with the local desktop through the node's private control socket.
#[derive(Debug)]
pub struct DesktopLink {
    pub commands: mpsc::UnboundedReceiver<Value>,
    pub messages: mpsc::UnboundedSender<Value>,
}

type Reply<T> = oneshot::Sender<Result<T, BrowserError>>;

#[derive(Debug)]
enum LayoutChange {
    Resize {
        size: Size,
        viewport: Option<ViewportMode>,
        measured_by: Option<String>,
    },
    Zoom(u16),
}

#[derive(Debug)]
enum FileChange {
    Choose { id: String, upload: Option<Upload> },
    Upload { node: i64, upload: Upload },
}

#[derive(Debug)]
enum Ask {
    Read(Read),
    Desktop {
        running: Running,
        reply: Reply<()>,
    },
    Open {
        thread: String,
        url: Option<String>,
        size: Size,
        viewer: Option<String>,
        placement: PagePlacement,
        reply: Reply<String>,
    },
    Close {
        page: String,
    },
    CloseThread {
        thread: String,
    },
    Retry {
        page: String,
        viewer: String,
        reply: Reply<()>,
    },
    Attached {
        page: String,
        reply: Reply<Attached>,
    },
    Layout {
        page: String,
        change: LayoutChange,
        presentation: PagePresentation,
        reply: Reply<()>,
    },
    Files {
        page: String,
        change: FileChange,
        reply: Reply<()>,
    },
    Switch {
        thread: String,
        page: String,
        reply: Reply<()>,
    },
    Watch {
        page: String,
        session: String,
        presentation: PagePresentation,
        asks: mpsc::UnboundedSender<Ask>,
        reply: Reply<Viewing>,
    },
    Unwatch {
        page: String,
        session: String,
        presentation: PagePresentation,
    },
    Control {
        page: String,
        session: String,
        action: crate::protocol::ControlAction,
        reply: Reply<()>,
    },
    Drive {
        page: String,
        session: Option<String>,
        asks: mpsc::UnboundedSender<Ask>,
        reply: Reply<Driving>,
    },
    Drove {
        page: String,
    },
}

#[derive(Debug)]
enum Read {
    Pages {
        thread: String,
        reply: oneshot::Sender<Vec<PageView>>,
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
    Network {
        page: String,
        reply: oneshot::Sender<Network>,
    },
    Downloads {
        page: String,
        reply: oneshot::Sender<Vec<Download>>,
    },
    SavedDownload {
        page: String,
        download: String,
        reply: oneshot::Sender<Option<SavedDownload>>,
    },
}

/// Holds control stable through one operation. Cancellation also releases the operation.
#[derive(Debug)]
pub(crate) struct Driving {
    asks: mpsc::UnboundedSender<Ask>,
    page: String,
}

impl Drop for Driving {
    fn drop(&mut self) {
        let _ = self.asks.send(Ask::Drove {
            page: std::mem::take(&mut self.page),
        });
    }
}

#[derive(Debug)]
pub(crate) struct Viewing {
    asks: mpsc::UnboundedSender<Ask>,
    page: String,
    session: String,
    presentation: PagePresentation,
}

impl Drop for Viewing {
    fn drop(&mut self) {
        let _ = self.asks.send(Ask::Unwatch {
            page: std::mem::take(&mut self.page),
            session: std::mem::take(&mut self.session),
            presentation: self.presentation,
        });
    }
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

    /// Registers the renderer for new desktop pages until its control connection closes.
    ///
    /// # Errors
    /// The browser is gone or another desktop is connected.
    pub async fn desktop(&self) -> Result<DesktopLink, BrowserError> {
        let (outgoing, commands) = mpsc::unbounded_channel();
        let (messages, incoming) = mpsc::unbounded_channel();
        let (cdp, events) = connect_desktop(outgoing, incoming);
        self.ask(|reply| Ask::Desktop {
            running: Running {
                cdp,
                events,
                _machine: None,
            },
            reply,
        })
        .await?;
        Ok(DesktopLink { commands, messages })
    }

    async fn ask<T>(&self, ask: impl FnOnce(Reply<T>) -> Ask) -> Result<T, BrowserError> {
        let (reply, answer) = oneshot::channel();
        self.asks.send(ask(reply)).map_err(|_| BrowserError::Gone)?;
        answer.await.map_err(|_| BrowserError::Gone)?
    }

    async fn get<T: Default>(&self, ask: impl FnOnce(oneshot::Sender<T>) -> Read) -> T {
        let (reply, answer) = oneshot::channel();
        if self.asks.send(Ask::Read(ask(reply))).is_err() {
            return T::default();
        }
        answer.await.unwrap_or_default()
    }

    /// Opens a page for `thread`, laid out at `size`, at `url` or `about:blank`: its id.
    ///
    /// # Errors
    /// `NoBrowser` where the machine has none; the browser's refusal.
    pub async fn open(&self, thread: &str, url: Option<&str>, size: Size) -> Result<String, BrowserError> {
        self.open_in(thread, url, size, PagePlacement::Workspace).await
    }

    pub(crate) async fn open_in(
        &self,
        thread: &str,
        url: Option<&str>,
        size: Size,
        placement: PagePlacement,
    ) -> Result<String, BrowserError> {
        self.open_for(thread, url, size, None, placement).await
    }

    pub(crate) async fn open_for(
        &self,
        thread: &str,
        url: Option<&str>,
        size: Size,
        viewer: Option<String>,
        placement: PagePlacement,
    ) -> Result<String, BrowserError> {
        let (thread, url) = (thread.to_owned(), url.map(str::to_owned));
        self.ask(|reply| Ask::Open {
            thread,
            url,
            size,
            viewer,
            placement,
            reply,
        })
        .await
    }

    /// Closes the page `page`.
    pub fn close(&self, page: &str) {
        let _ = self.asks.send(Ask::Close { page: page.to_owned() });
    }

    /// Closes every page of `thread`, as it goes.
    pub fn close_thread(&self, thread: &str) {
        let _ = self.asks.send(Ask::CloseThread { thread: thread.to_owned() });
    }

    pub(crate) async fn retry(&self, page: &str, viewer: &str) -> Result<(), BrowserError> {
        self.ask(|reply| Ask::Retry {
            page: page.to_owned(),
            viewer: viewer.to_owned(),
            reply,
        })
        .await
    }

    /// The pages of `thread`, in the order they were opened.
    pub async fn pages(&self, thread: &str) -> Vec<PageView> {
        let thread = thread.to_owned();
        self.get(|reply| Read::Pages { thread, reply }).await
    }

    /// The page's connected target, to read and act on it.
    ///
    /// # Errors
    /// `NoSuchPage`, `Disconnected`, or the browser's refusal.
    pub async fn attached(&self, page: &str) -> Result<Attached, BrowserError> {
        let page = page.to_owned();
        self.ask(|reply| Ask::Attached { page, reply }).await
    }

    /// The page's newest frame, where it was painted.
    pub async fn frame(&self, page: &str) -> Option<Frame> {
        let page = page.to_owned();
        self.get(|reply| Read::Frame { page, reply }).await
    }

    /// The messages of the page's console numbered past `after`, oldest first.
    pub async fn messages(&self, page: &str, after: u64) -> Vec<Message> {
        let page = page.to_owned();
        self.get(|reply| Read::Messages { page, after, reply }).await
    }

    /// The page `page`, as a list shows it; none where there is no such page.
    pub async fn view(&self, page: &str) -> Option<PageView> {
        let page = page.to_owned();
        self.get(|reply| Read::View { page, reply }).await
    }

    /// The page's own events since they were last taken, as they came, oldest first.
    pub async fn drain_events(&self, page: &str) -> Vec<Value> {
        let page = page.to_owned();
        self.get(|reply| Read::Drain { page, reply }).await
    }

    /// The session of the page's frame of another site's process whose address holds `part`, where it has one.
    pub(crate) async fn frame_session(&self, page: &str, part: &str) -> Option<String> {
        let (page, part) = (page.to_owned(), part.to_owned());
        self.get(|reply| Read::FrameSession { page, part, reply }).await
    }

    /// Keeps `read`, the refs a reading of the page named: those its reading before named.
    pub(crate) async fn swap_refs(&self, page: &str, read: Vec<i64>) -> HashSet<i64> {
        let page = page.to_owned();
        self.get(|reply| Read::Refs { page, read, reply }).await
    }

    /// The page of `thread`'s the agent works, as a person works one tab at a time: the one it opened or switched to
    /// last, else the thread's newest.
    pub async fn current(&self, thread: &str) -> Option<String> {
        let thread = thread.to_owned();
        self.get(|reply| Read::Current { thread, reply }).await
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
        self.get(|reply| Read::Network { page, reply }).await
    }

    /// What the page downloaded, or downloads, in the order it began.
    pub async fn downloads(&self, page: &str) -> Vec<Download> {
        let page = page.to_owned();
        self.get(|reply| Read::Downloads { page, reply }).await
    }

    pub(crate) async fn saved_download(&self, page: &str, download: &str) -> Option<SavedDownload> {
        self.get(|reply| Read::SavedDownload {
            page: page.to_owned(),
            download: download.to_owned(),
            reply,
        })
        .await
    }

    pub(crate) async fn choose_files(&self, page: &str, id: String, upload: Option<Upload>) -> Result<(), BrowserError> {
        self.change_files(page, FileChange::Choose { id, upload }).await
    }

    pub(crate) async fn upload(&self, page: &str, node: i64, upload: Upload) -> Result<(), BrowserError> {
        self.change_files(page, FileChange::Upload { node, upload }).await
    }

    async fn change_files(&self, page: &str, change: FileChange) -> Result<(), BrowserError> {
        self.ask(|reply| Ask::Files {
            page: page.to_owned(),
            change,
            reply,
        })
        .await
    }

    /// Lays the page out at `size`.
    ///
    /// # Errors
    /// `NoSuchPage`; the browser's refusal.
    pub async fn resize(&self, page: &str, size: Size) -> Result<(), BrowserError> {
        self.change_layout(
            page,
            PagePresentation::Native,
            LayoutChange::Resize {
                size,
                viewport: None,
                measured_by: None,
            },
        )
        .await
    }

    pub(crate) async fn resize_view(
        &self,
        page: &str,
        size: Size,
        presentation: PagePresentation,
        viewport: Option<ViewportMode>,
        session: &str,
    ) -> Result<(), BrowserError> {
        let measured_by = viewport.is_none().then(|| session.to_owned());
        self.change_layout(
            page,
            presentation,
            LayoutChange::Resize {
                size,
                viewport,
                measured_by,
            },
        )
        .await
    }

    async fn change_layout(&self, page: &str, presentation: PagePresentation, change: LayoutChange) -> Result<(), BrowserError> {
        self.ask(|reply| Ask::Layout {
            page: page.to_owned(),
            change,
            presentation,
            reply,
        })
        .await
    }

    pub(crate) async fn zoom(&self, page: &str, percent: u16, presentation: PagePresentation) -> Result<(), BrowserError> {
        self.change_layout(page, presentation, LayoutChange::Zoom(percent)).await
    }

    pub(crate) async fn watch(&self, page: &str, session: &str, presentation: PagePresentation) -> Result<Viewing, BrowserError> {
        self.ask(|reply| Ask::Watch {
            page: page.to_owned(),
            session: session.to_owned(),
            presentation,
            asks: self.asks.clone(),
            reply,
        })
        .await
    }

    pub(crate) async fn control(&self, page: &str, session: &str, action: crate::protocol::ControlAction) -> Result<(), BrowserError> {
        self.ask(|reply| Ask::Control {
            page: page.to_owned(),
            session: session.to_owned(),
            action,
            reply,
        })
        .await
    }

    pub(crate) async fn drive(&self, page: &str, session: Option<&str>) -> Result<Driving, BrowserError> {
        self.ask(|reply| Ask::Drive {
            page: page.to_owned(),
            session: session.map(str::to_owned),
            asks: self.asks.clone(),
            reply,
        })
        .await
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
    viewport: ViewportMode,
    zoom: u16,
    runtime: Runtime,
    dev_tools: DevToolsState,
    control: Control,
    target: Option<String>,
    session: Option<String>,
    /// Where it is to go once its target is set up.
    going: Option<String>,
    dialog: Option<Dialog>,
    load: PageLoad,
    document_request: Option<navigation::DocumentRequest>,
    /// Whether its history goes back and forward from where it is, as the browser told of it last.
    back: bool,
    forward: bool,
    frame: Option<Frame>,
    frame_number: u64,
    paint: Paint,
    messages: VecDeque<Message>,
    heard: u64,
    /// The requests in flight, by the browser's id, with when each began; and when one last began or ended.
    in_flight: HashMap<String, Instant>,
    network_at: Instant,
    downloads: Vec<Download>,
    chooser: Option<FileChooser>,
    uploads: Vec<Upload>,
    /// The newest events of its target, as they came, for a script to take.
    events: VecDeque<Value>,
    /// The refs its last reading named, so the next marks those new.
    refs: HashSet<i64>,
    /// Its frames of other sites' processes, as the browser attached to them: each target, its session and address.
    frames: Vec<FrameTarget>,
}

#[derive(Debug)]
struct FileChooser {
    shown: PageFileChooser,
    frame: String,
    session: String,
    node: i64,
}

#[derive(Debug, Default)]
struct Control {
    waiting_for_view: Option<String>,
    controller: Option<String>,
    viewers: HashMap<String, usize>,
    actions: usize,
    streams: usize,
}

impl Control {
    fn watch(&mut self, session: String) {
        if self.waiting_for_view.as_deref() == Some(&session) {
            self.waiting_for_view.take();
            if self.actions == 0 && self.controller.is_none() {
                self.controller = Some(session.clone());
            }
        }
        *self.viewers.entry(session).or_default() += 1;
    }

    fn unwatch(&mut self, session: &str) {
        if let Some(count) = self.viewers.get_mut(session) {
            *count -= 1;
            if *count == 0 {
                self.viewers.remove(session);
            }
        }
        self.release_if_unwatched();
    }

    fn release_if_unwatched(&mut self) {
        if self.actions == 0 && self.controller.as_ref().is_some_and(|person| !self.viewers.contains_key(person)) {
            self.controller = None;
        }
    }

    fn change(&mut self, action: crate::protocol::ControlAction, session: String) -> Result<(), BrowserError> {
        use crate::protocol::ControlAction::{Release, Take};
        match action {
            Take if self.controller.as_deref() == Some(&session) => Ok(()),
            Release if self.controller.is_none() => Ok(()),
            _ if self.controller.as_ref().is_some_and(|person| *person != session) => Err(BrowserError::PageControlled),
            _ if self.actions != 0 => Err(BrowserError::PageBusy),
            Take if !self.viewers.contains_key(&session) => Err(BrowserError::PageNotViewed),
            Take => {
                self.controller = Some(session);
                self.waiting_for_view = None;
                Ok(())
            }
            Release => {
                self.controller = None;
                Ok(())
            }
        }
    }

    fn admit(&mut self, session: Option<&str>) -> Result<(), BrowserError> {
        if self.controller.as_deref() != session {
            return Err(BrowserError::PageControlled);
        }
        self.actions += 1;
        Ok(())
    }
}

/// A frame of another site's process, attached to as the browser made it.
#[derive(Debug, Clone)]
struct FrameTarget {
    target: String,
    session: String,
    url: String,
}

/// Coalesces invalidations while a high-DPI screenshot is in flight.
#[derive(Debug, Clone, Copy, Default)]
struct Paint {
    capturing: bool,
    dirty: bool,
}

struct Captured {
    page: String,
    session: String,
    target: String,
    size: Size,
    jpeg: Option<Bytes>,
}

impl Page {
    fn new(id: String, thread: String, size: Size, url: Option<String>, runtime: Runtime) -> Self {
        let load = if url.is_some() { PageLoad::Loading } else { PageLoad::Ready };
        Self {
            id,
            thread,
            url: url.clone().unwrap_or_else(|| "about:blank".into()),
            title: String::new(),
            size,
            viewport: ViewportMode::Fill,
            zoom: 100,
            runtime,
            dev_tools: DevToolsState::Closed,
            control: Control::default(),
            target: None,
            session: None,
            going: url,
            dialog: None,
            load,
            document_request: None,
            back: false,
            forward: false,
            frame: None,
            frame_number: 0,
            paint: Paint::default(),
            messages: VecDeque::new(),
            heard: 0,
            in_flight: HashMap::new(),
            network_at: Instant::now(),
            downloads: Vec::new(),
            chooser: None,
            uploads: Vec::new(),
            events: VecDeque::new(),
            refs: HashSet::new(),
            frames: Vec::new(),
        }
    }

    fn owns_session(&self, session: &str) -> bool {
        self.session.as_deref() == Some(session) || self.frames.iter().any(|frame| frame.session == session)
    }

    fn disconnect(&mut self) {
        self.target = None;
        self.session = None;
        self.load = PageLoad::Disconnected;
        self.document_request = None;
        self.dev_tools = DevToolsState::Closed;
        self.control.controller = None;
        self.control.waiting_for_view = None;
        self.dialog = None;
        self.back = false;
        self.forward = false;
        self.frame = None;
        self.in_flight.clear();
        self.paint = Paint::default();
        self.frames.clear();
        self.chooser = None;
        self.uploads.clear();
        self.refs.clear();
        for download in &mut self.downloads {
            if download.state == "inProgress" {
                download.state = "canceled".into();
            }
        }
    }

    fn reference(&self) -> Option<PageReference> {
        let url = Url::parse(&self.url).ok()?;
        if !matches!(url.scheme(), "http" | "https") || !url.username().is_empty() || url.password().is_some() || url.as_str().len() > 8192
        {
            return None;
        }
        Some(PageReference {
            url: url.into(),
            title: self.title.chars().take(240).collect(),
            host: self.runtime,
        })
    }

    fn casts(&self) -> bool {
        self.runtime == Runtime::Headless || self.control.streams > 0
    }

    fn view_size(&self) -> Size {
        self.size.zoomed(self.zoom, self.runtime)
    }

    fn admit(&mut self, session: Option<&str>) -> Result<(), BrowserError> {
        if session.is_none() && self.dev_tools == DevToolsState::Open {
            return Err(BrowserError::PageControlled);
        }
        self.control.admit(session)
    }
}

/// The browser as it runs: its pipe, and what it tells.
#[derive(Debug)]
struct Running {
    cdp: Cdp,
    events: mpsc::UnboundedReceiver<Value>,
    /// The machine it was started in, kept as long as it runs.
    _machine: Option<Machine>,
}

struct Owner {
    host: BrowserHost,
    headless: Option<Running>,
    desktop: Option<Running>,
    desktop_thread: Option<String>,
    desktop_session: Option<String>,
    pages: Vec<Page>,
    /// The page each thread's agent works, by the thread.
    current: HashMap<String, String>,
    events: broadcast::Sender<Change>,
    captures: JoinSet<Captured>,
}

impl Owner {
    fn new(host: BrowserHost, events: broadcast::Sender<Change>) -> Self {
        Self {
            host,
            headless: None,
            desktop: None,
            desktop_thread: None,
            desktop_session: None,
            pages: Vec::new(),
            current: HashMap::new(),
            events,
            captures: JoinSet::new(),
        }
    }

    async fn serve(mut self, mut inbox: mpsc::UnboundedReceiver<Ask>) {
        loop {
            // What the browser told is heard before what is asked, so an answer is at least as new as what the
            // asker saw of the page.
            tokio::select! {
                biased;
                capture = self.captures.join_next(), if !self.captures.is_empty() => {
                    if let Some(Ok(capture)) = capture {
                        self.finished_capture(capture);
                    }
                }
                event = next_event(&mut self.headless) => match event {
                    Some(event) => self.heard(Runtime::Headless, event).await,
                    None => self.ended(Runtime::Headless),
                },
                event = next_event(&mut self.desktop) => match event {
                    Some(event) => self.heard(Runtime::Desktop, event).await,
                    None => self.ended(Runtime::Desktop),
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
            Ask::Read(read) => self.read(read),
            Ask::Desktop { running, reply } => {
                if self.desktop.is_some() {
                    let _ = reply.send(Err(BrowserError::DesktopAttached));
                } else {
                    self.desktop = Some(running);
                    let _ = reply.send(Ok(()));
                }
            }
            Ask::Open {
                thread,
                url,
                size,
                viewer,
                placement,
                reply,
            } => {
                let _ = reply.send(self.open(thread, url, size, viewer, placement).await);
            }
            Ask::Close { page } => self.close(&page).await,
            Ask::CloseThread { thread } => self.close_thread(&thread).await,
            Ask::Retry { page, viewer, reply } => {
                let _ = reply.send(self.retry(&page, &viewer).await);
            }
            Ask::Attached { page, reply } => {
                let _ = reply.send(self.attached(&page).await);
            }
            Ask::Layout {
                page,
                change,
                presentation,
                reply,
            } => {
                let _ = reply.send(self.adjust_layout(&page, change, presentation).await);
            }
            Ask::Files { page, change, reply } => {
                let _ = reply.send(self.change_files(&page, change).await);
            }
            Ask::Switch { thread, page, reply } => {
                let _ = reply.send(self.switch(thread, page));
            }
            Ask::Watch {
                page,
                session,
                presentation,
                asks,
                reply,
            } => {
                let viewing = self.watch_page(&page, &session, presentation).await.map(|()| Viewing {
                    asks,
                    page,
                    session,
                    presentation,
                });
                let _ = reply.send(viewing);
            }
            Ask::Unwatch {
                page,
                session,
                presentation,
            } => {
                self.unwatch_page(&page, &session, presentation).await;
            }
            Ask::Control {
                page,
                session,
                action,
                reply,
            } => {
                let result = self
                    .control_changed(&page, |control| control.change(action, session))
                    .await
                    .unwrap_or(Err(BrowserError::NoSuchPage));
                let _ = reply.send(result);
            }
            Ask::Drive {
                page,
                session,
                asks,
                reply,
            } => {
                let result = self
                    .page_mut(&page)
                    .ok_or(BrowserError::NoSuchPage)
                    .and_then(|page| page.admit(session.as_deref()));
                let _ = reply.send(result.map(|()| Driving { asks, page }));
            }
            Ask::Drove { page } => {
                self.control_changed(&page, |control| {
                    control.actions -= 1;
                    control.release_if_unwatched();
                })
                .await;
            }
        }
    }

    fn switch(&mut self, thread: String, page: String) -> Result<(), BrowserError> {
        if self.page(&page).is_none_or(|found| found.thread != thread) {
            return Err(BrowserError::NoSuchPage);
        }
        self.current.insert(thread, page);
        Ok(())
    }

    fn read(&mut self, read: Read) {
        match read {
            Read::Pages { thread, reply } => {
                let _ = reply.send(self.pages.iter().filter(|page| page.thread == thread).map(view).collect());
            }
            Read::Frame { page, reply } => {
                let _ = reply.send(self.page(&page).and_then(|page| page.frame.clone()));
            }
            Read::Messages { page, after, reply } => {
                let found = self
                    .page(&page)
                    .map(|page| page.messages.iter().filter(|message| message.number > after).cloned().collect());
                let _ = reply.send(found.unwrap_or_default());
            }
            Read::View { page, reply } => {
                let _ = reply.send(self.page(&page).map(view));
            }
            Read::Drain { page, reply } => {
                let drained = self.page_mut(&page).map(|page| page.events.drain(..).collect());
                let _ = reply.send(drained.unwrap_or_default());
            }
            Read::FrameSession { page, part, reply } => {
                let session = self.page(&page).and_then(|page| {
                    page.frames
                        .iter()
                        .find(|frame| frame.url.contains(&part))
                        .map(|frame| frame.session.clone())
                });
                let _ = reply.send(session);
            }
            Read::Refs { page, read, reply } => {
                let before = self
                    .page_mut(&page)
                    .map(|page| std::mem::replace(&mut page.refs, read.into_iter().collect()));
                let _ = reply.send(before.unwrap_or_default());
            }
            Read::Current { thread, reply } => {
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
            Read::Network { page, reply } => {
                let busy = self.page(&page).map(|page| Network {
                    in_flight: page.in_flight.values().filter(|began| began.elapsed().as_millis() < 1500).count(),
                    quiet_ms: u64::try_from(page.network_at.elapsed().as_millis()).unwrap_or(u64::MAX),
                });
                let _ = reply.send(busy.unwrap_or_default());
            }
            Read::Downloads { page, reply } => {
                let _ = reply.send(self.page(&page).map_or_default(|page| page.downloads.clone()));
            }
            Read::SavedDownload { page, download, reply } => {
                let _ = reply.send(self.saved_download(&page, &download));
            }
        }
    }

    async fn control_changed<T>(&mut self, id: &str, change: impl FnOnce(&mut Control) -> T) -> Option<T> {
        let page = self.page_mut(id)?;
        let controller = page.control.controller.clone();
        let result = change(&mut page.control);
        if controller != page.control.controller {
            let thread = page.thread.clone();
            let close_tools = page.dev_tools == DevToolsState::Open && page.control.controller.is_none();
            self.sync_desktop_control(id).await;
            if close_tools {
                self.close_dev_tools(id).await;
            }
            self.told(&thread);
        }
        Some(result)
    }

    async fn watch_page(&mut self, id: &str, session: &str, presentation: PagePresentation) -> Result<(), BrowserError> {
        let page = self.page(id).ok_or(BrowserError::NoSuchPage)?;
        let was_casting = page.casts();
        self.control_changed(id, |control| {
            control.watch(session.to_owned());
            control.streams += usize::from(presentation == PagePresentation::Stream);
        })
        .await;
        if !was_casting
            && let Some(page) = self.page(id)
            && page.casts()
            && let Some(running) = self.running(page.runtime)
            && let Some(session) = &page.session
        {
            let _ = start_screencast(&running.cdp, session).await;
        }
        Ok(())
    }

    async fn unwatch_page(&mut self, id: &str, session: &str, presentation: PagePresentation) {
        let Some(page) = self.page(id) else { return };
        let was_casting = page.casts();
        self.control_changed(id, |control| {
            control.unwatch(session);
            control.streams -= usize::from(presentation == PagePresentation::Stream);
        })
        .await;
        if was_casting
            && let Some(page) = self.page_mut(id)
            && !page.casts()
        {
            page.frame = None;
            let session = page.session.clone();
            if let (Some(running), Some(session)) = (&self.desktop, session) {
                let _ = running.cdp.call(Some(&session), "Page.stopScreencast", json!({})).await;
            }
        }
    }
    fn page(&self, id: &str) -> Option<&Page> {
        self.pages.iter().find(|page| page.id == id)
    }

    fn page_mut(&mut self, id: &str) -> Option<&mut Page> {
        self.pages.iter_mut().find(|page| page.id == id)
    }

    fn running(&self, runtime: Runtime) -> Option<&Running> {
        match runtime {
            Runtime::Headless => self.headless.as_ref(),
            Runtime::Desktop => self.desktop.as_ref(),
        }
    }

    /// Asks the browser where the history of the page `id` goes from where it is — back where it has entries before, forward
    /// where after — kept and told where it changed.
    async fn history(&mut self, id: &str) {
        let Some(page) = self.page(id) else { return };
        let (Some(running), Some(session)) = (self.running(page.runtime), page.session.clone()) else {
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

    async fn remember(&self, page: &Page) {
        if let (Some(data), Some(reference)) = (&self.host.references, page.reference())
            && let Ok(bytes) = serde_json::to_vec(&reference)
            && let Err(error) = data.write(&page.thread, &page.id, bytes.into()).await
        {
            tracing::debug!(page = %page.id, %error, "could not remember browser page");
        }
    }

    /// The browser's pipe, the browser started where it is not running.
    async fn headless_cdp(&mut self) -> Result<Cdp, BrowserError> {
        if let Some(running) = &self.headless {
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
        self.headless = Some(running);
        Ok(cdp)
    }

    async fn open(
        &mut self,
        thread: String,
        url: Option<String>,
        size: Size,
        viewer: Option<String>,
        placement: PagePlacement,
    ) -> Result<String, BrowserError> {
        let desktop = self.desktop.is_some() && self.desktop_thread.as_deref() == Some(&thread);
        let runtime = match placement {
            PagePlacement::Workspace if desktop => Runtime::Desktop,
            PagePlacement::Workspace | PagePlacement::Background => Runtime::Headless,
            PagePlacement::Desktop if desktop && viewer.as_ref().is_none_or(|viewer| self.desktop_session.as_ref() == Some(viewer)) => {
                Runtime::Desktop
            }
            PagePlacement::Desktop => return Err(BrowserError::NeedsDesktop),
        };
        if viewer.is_some()
            && let Some(url) = url.as_deref().and_then(|value| Url::parse(value).ok())
            && let Some(page) = self.pages.iter().rev().find(|page| {
                page.thread == thread
                    && (placement == PagePlacement::Workspace || page.runtime == runtime)
                    && Url::parse(&page.url).is_ok_and(|current| current == url)
            })
        {
            return Ok(page.id.clone());
        }
        let id = self.host.commands.new_id()?;
        let mut page = Page::new(id.clone(), thread.clone(), size, url, runtime);
        page.control.waiting_for_view = viewer;
        self.pages.push(page);
        if let Err(error) = self.connect_target(&id).await {
            self.close(&id).await;
            return Err(error);
        }
        if let Some(page) = self.page(&id) {
            self.remember(page).await;
        }
        self.current.insert(thread.clone(), id.clone());
        self.told(&thread);
        Ok(id)
    }

    async fn connect_target(&mut self, id: &str) -> Result<(), BrowserError> {
        let page = self.page(id).ok_or(BrowserError::NoSuchPage)?;
        match page.runtime {
            Runtime::Headless => {
                let cdp = self.headless_cdp().await?;
                self.create(&cdp, id).await
            }
            Runtime::Desktop => {
                let cdp = self.desktop.as_ref().ok_or(BrowserError::NeedsDesktop)?.cdp.clone();
                let thread = page.thread.clone();
                self.create_desktop(&cdp, id, &thread, page.view_size()).await
            }
        }
    }

    async fn retry(&mut self, id: &str, viewer: &str) -> Result<(), BrowserError> {
        let page = self.page(id).ok_or(BrowserError::NoSuchPage)?;
        if page.control.controller.as_deref().is_some_and(|controller| controller != viewer) {
            return Err(BrowserError::PageControlled);
        }
        if page.control.actions > 0 {
            return Err(BrowserError::PageBusy);
        }
        let thread = page.thread.clone();
        match page.load {
            PageLoad::Disconnected => {
                if page.runtime == Runtime::Desktop
                    && (self.desktop_thread.as_deref() != Some(&thread) || self.desktop_session.as_deref() != Some(viewer))
                {
                    return Err(BrowserError::NeedsDesktop);
                }
                let page = self.page_mut(id).ok_or(BrowserError::NoSuchPage)?;
                page.going = page.reference().map(|address| address.url);
                page.load = if page.going.is_some() { PageLoad::Loading } else { PageLoad::Ready };
                page.control.waiting_for_view = Some(viewer.to_owned());
                if let Err(error) = self.connect_target(id).await {
                    if let Some(page) = self.page(id) {
                        self.close_target(page).await;
                    }
                    self.disconnect_page(id);
                    return Err(error);
                }
            }
            PageLoad::Failed { .. } => {
                let url = page.reference().ok_or(BrowserError::Disconnected)?.url;
                let attached = self.attached(id).await?;
                attached
                    .cdp
                    .call(Some(&attached.session), "Page.navigate", json!({ "url": url }))
                    .await?;
            }
            PageLoad::Ready | PageLoad::Loading => return Ok(()),
        }
        self.told(&thread);
        Ok(())
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

    async fn create_desktop(&mut self, cdp: &Cdp, id: &str, thread: &str, size: Size) -> Result<(), BrowserError> {
        if let Some(downloads) = &self.host.downloads {
            std::fs::create_dir_all(downloads).map_err(|error| BrowserError::Folders(error.to_string()))?;
        }
        let params = json!({
            "page": id, "thread": thread, "width": size.width, "height": size.height, "downloads": self.host.downloads,
        });
        cdp.call_within(None, "Octoii.open", params, std::time::Duration::from_secs(8))
            .await?;
        let info = cdp.call(Some(id), "Target.getTargetInfo", json!({})).await?;
        let target = info["targetInfo"]["targetId"]
            .as_str()
            .ok_or_else(|| CdpError::Unread("desktop target identity is missing".into()))?;
        let page = self.page_mut(id).ok_or(BrowserError::NoSuchPage)?;
        page.target = Some(target.to_owned());
        page.session = Some(id.to_owned());
        let url = page.going.take();
        set_up(cdp, id, size).await?;
        cdp.call(
            Some(id),
            "Emulation.setDefaultBackgroundColorOverride",
            json!({ "color": { "r": 255, "g": 255, "b": 255, "a": 1 } }),
        )
        .await?;
        if let Some(url) = url {
            cdp.call(Some(id), "Page.navigate", json!({ "url": url })).await?;
        }
        if self.page(id).is_some_and(Page::casts) {
            start_screencast(cdp, id).await?;
        }
        Ok(())
    }

    async fn attached(&mut self, id: &str) -> Result<Attached, BrowserError> {
        let page = self.page(id).ok_or(BrowserError::NoSuchPage)?;
        if page.load == PageLoad::Disconnected {
            return Err(BrowserError::Disconnected);
        }
        if page.runtime == Runtime::Desktop {
            let cdp = self.desktop.as_ref().ok_or(BrowserError::Gone)?.cdp.clone();
            let session = page.session.clone().ok_or(BrowserError::Gone)?;
            return Ok(Attached { cdp, session });
        }
        let cdp = self.headless.as_ref().ok_or(BrowserError::Gone)?.cdp.clone();
        // The target is set up as the browser tells it attached; what came meanwhile is heard first.
        for _ in 0..200 {
            if let Some(session) = self.page(id).and_then(|page| page.session.clone()) {
                return Ok(Attached { cdp, session });
            }
            let Some(running) = self.headless.as_mut() else {
                return Err(BrowserError::Gone);
            };
            match tokio::time::timeout(std::time::Duration::from_millis(50), running.events.recv()).await {
                Ok(Some(event)) => self.heard(Runtime::Headless, event).await,
                Ok(None) => {
                    self.ended(Runtime::Headless);
                    return Err(BrowserError::Gone);
                }
                Err(_) => {}
            }
        }
        Err(BrowserError::Gone)
    }

    async fn adjust_layout(&mut self, id: &str, change: LayoutChange, presentation: PagePresentation) -> Result<(), BrowserError> {
        let page = self.page(id).ok_or(BrowserError::NoSuchPage)?;
        if page.runtime == Runtime::Desktop && presentation == PagePresentation::Stream {
            return match change {
                LayoutChange::Resize { viewport: None, .. } => Ok(()),
                _ => Err(BrowserError::DesktopViewport),
            };
        }
        match change {
            LayoutChange::Resize {
                mut size,
                viewport,
                measured_by,
            } => {
                if measured_by
                    .as_ref()
                    .is_some_and(|session| page.control.controller.as_ref() != Some(session))
                {
                    return Ok(());
                }
                if viewport.is_none() && page.viewport == ViewportMode::Fixed {
                    (size.width, size.height) = (page.size.width, page.size.height);
                }
                let viewport = viewport.unwrap_or(page.viewport);
                if viewport == ViewportMode::Fixed && (!(1..=4096).contains(&size.width) || !(1..=4096).contains(&size.height)) {
                    return Err(BrowserError::InvalidViewport);
                }
                self.layout(id, size, viewport, page.zoom).await
            }
            LayoutChange::Zoom(percent) if (50..=200).contains(&percent) => self.layout(id, page.size, page.viewport, percent).await,
            LayoutChange::Zoom(_) => Err(BrowserError::InvalidZoom),
        }
    }

    async fn layout(&mut self, id: &str, size: Size, viewport: ViewportMode, zoom: u16) -> Result<(), BrowserError> {
        let page = self.page(id).ok_or(BrowserError::NoSuchPage)?;
        if (page.size, page.viewport, page.zoom) == (size, viewport, zoom) {
            return Ok(());
        }
        let view = size.zoomed(zoom, page.runtime);
        let resized = page.view_size() != view;
        let thread = page.thread.clone();
        let casts = page.casts();
        let target = page
            .session
            .clone()
            .zip(self.running(page.runtime).map(|running| running.cdp.clone()));
        if resized && let Some((session, cdp)) = &target {
            lay_out(cdp, session, view).await?;
        }
        if let Some(page) = self.page_mut(id) {
            page.size = size;
            page.viewport = viewport;
            page.zoom = zoom;
            if resized {
                page.frame = None;
            }
        }
        self.told(&thread);
        if resized
            && casts
            && let Some((session, cdp)) = target
        {
            cdp.call(Some(&session), "Page.stopScreencast", json!({})).await?;
            start_screencast(&cdp, &session).await?;
        }
        Ok(())
    }

    async fn close_thread(&mut self, thread: &str) {
        let pages: Vec<_> = self
            .pages
            .iter()
            .filter(|page| page.thread == thread)
            .map(|page| page.id.clone())
            .collect();
        for page in pages {
            self.close(&page).await;
        }
    }

    async fn change_files(&mut self, id: &str, change: FileChange) -> Result<(), BrowserError> {
        let attached = self.attached(id).await?;
        let page = self.page(id).ok_or(BrowserError::NoSuchPage)?;
        let (session, node, upload) = match change {
            FileChange::Choose { id, upload } => {
                let chooser = page
                    .chooser
                    .as_ref()
                    .filter(|chooser| chooser.shown.id == id)
                    .ok_or(BrowserError::ChooserChanged)?;
                if !chooser.shown.multiple && upload.as_ref().is_some_and(|files| files.paths.len() > 1) {
                    return Err(BrowserError::FileTransfer("this input accepts one file".into()));
                }
                (chooser.session.clone(), chooser.node, upload)
            }
            FileChange::Upload { node, upload } => (attached.session, node, Some(upload)),
        };
        if let Some(files) = &upload {
            let fields = json!({ "backendNodeId": node, "files": files.paths });
            if page.runtime == Runtime::Desktop {
                attached
                    .cdp
                    .call(None, "Octoii.upload", json!({ "page": id, "target": session, "fields": fields }))
                    .await?;
            } else {
                attached.cdp.call(Some(&session), "DOM.setFileInputFiles", fields).await?;
            }
        }
        if let Some(page) = self.page_mut(id) {
            page.uploads.extend(upload);
            if page
                .chooser
                .as_ref()
                .is_some_and(|chooser| chooser.session == session && chooser.node == node)
            {
                page.chooser = None;
                let thread = page.thread.clone();
                self.told(&thread);
            }
        }
        Ok(())
    }

    async fn close(&mut self, id: &str) {
        let Some(at) = self.pages.iter().position(|page| page.id == id) else {
            return;
        };
        let page = self.pages.remove(at);
        self.remember(&page).await;
        self.close_target(&page).await;
        self.told(&page.thread);
    }

    async fn close_target(&self, page: &Page) {
        if let Some(running) = self.running(page.runtime) {
            match (page.runtime, &page.target) {
                (Runtime::Desktop, _) => {
                    let _ = running.cdp.call(None, "Octoii.close", json!({ "page": page.id })).await;
                }
                (Runtime::Headless, Some(target)) => {
                    let _ = running.cdp.call(None, "Target.closeTarget", json!({ "targetId": target })).await;
                }
                (Runtime::Headless, None) => {}
            }
        }
    }

    /// The browser ended. The tabs stay visible and a retry opens a fresh target.
    fn ended(&mut self, runtime: Runtime) {
        if runtime == Runtime::Desktop {
            self.desktop = None;
            self.desktop_thread = None;
            self.desktop_session = None;
        } else {
            self.headless = None;
        }
        let mut changed = HashSet::new();
        for page in self.pages.iter_mut().filter(|page| page.runtime == runtime) {
            page.disconnect();
            changed.insert(page.thread.clone());
        }
        for thread in changed {
            self.told(&thread);
        }
    }

    fn disconnect_page(&mut self, id: &str) {
        if let Some(page) = self.page_mut(id) {
            page.disconnect();
            let thread = page.thread.clone();
            self.told(&thread);
        }
    }

    async fn desktop_event(&mut self, raw: &Value) -> bool {
        match raw["method"].as_str() {
            Some("Octoii.workspace") => self.desktop_workspace(&raw["params"]).await,
            Some("Octoii.download") => {
                if let Ok(download) = serde_json::from_value(raw["params"].clone()) {
                    self.desktop_download(download);
                }
            }
            Some("Octoii.closed") => {
                if let Some(id) = raw["params"]["page"].as_str()
                    && self.page(id).is_some_and(|page| page.runtime == Runtime::Desktop)
                {
                    self.disconnect_page(id);
                }
            }
            Some("Octoii.devtools") => {
                if let (Some(id), Some(open)) = (raw["params"]["page"].as_str(), raw["params"]["open"].as_bool()) {
                    self.set_dev_tools(id, if open { DevToolsState::Open } else { DevToolsState::Closed });
                    if open && self.page(id).is_some_and(|page| page.control.controller.is_none()) {
                        self.close_dev_tools(id).await;
                    }
                }
            }
            _ => return false,
        }
        true
    }

    async fn desktop_workspace(&mut self, params: &Value) {
        self.desktop_thread = params["thread"].as_str().map(str::to_owned);
        let session = params["session"].as_str().map(str::to_owned);
        if self.desktop_session == session {
            return;
        }
        self.desktop_session = session;
        let pages: Vec<_> = self
            .pages
            .iter()
            .filter(|page| page.runtime == Runtime::Desktop)
            .map(|page| page.id.clone())
            .collect();
        for page in pages {
            self.sync_desktop_control(&page).await;
        }
    }

    async fn sync_desktop_control(&self, id: &str) {
        if let (Some(page), Some(desktop)) = (self.page(id), &self.desktop)
            && page.runtime == Runtime::Desktop
        {
            let local = self
                .desktop_session
                .as_ref()
                .is_some_and(|session| page.control.controller.as_ref() == Some(session));
            let _ = desktop
                .cdp
                .call_within(
                    None,
                    "Octoii.control",
                    json!({ "page": id, "local": local }),
                    std::time::Duration::from_secs(3),
                )
                .await;
        }
    }

    fn saved_download(&self, id: &str, download: &str) -> Option<SavedDownload> {
        let page = self.page(id)?;
        let item = page
            .downloads
            .iter()
            .find(|item| item.id == download && item.state == "completed")?;
        Some(SavedDownload {
            thread: page.thread.clone(),
            name: item.name.clone(),
            path: self.download_path(download)?,
        })
    }

    fn download_path(&self, id: &str) -> Option<PathBuf> {
        let directory = self.host.downloads.as_ref()?;
        if id.len() != 36 || !id.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-') {
            return None;
        }
        Some(directory.join(id))
    }

    fn desktop_download(&mut self, event: DesktopDownload) {
        let Some(path) = self.download_path(&event.download.id) else {
            return;
        };
        let Some(page) = self.page_mut(&event.page).filter(|page| page.runtime == Runtime::Desktop) else {
            return;
        };
        let mut download = event.download;
        download.path = (download.state == "completed").then(|| path.to_string_lossy().into_owned());
        if let Some(item) = page.downloads.iter_mut().find(|item| item.id == download.id) {
            *item = download;
        } else {
            page.downloads.push(download);
        }
        let thread = page.thread.clone();
        self.told(&thread);
    }

    fn set_dev_tools(&mut self, id: &str, state: DevToolsState) {
        if let Some(page) = self.page_mut(id)
            && page.runtime == Runtime::Desktop
            && page.dev_tools != state
        {
            page.dev_tools = state;
            let thread = page.thread.clone();
            self.told(&thread);
        }
    }

    async fn close_dev_tools(&mut self, id: &str) {
        if let Some(desktop) = &self.desktop
            && desktop
                .cdp
                .call_within(
                    None,
                    "Octoii.close_devtools",
                    json!({ "page": id }),
                    std::time::Duration::from_secs(5),
                )
                .await
                .is_ok()
        {
            self.set_dev_tools(id, DevToolsState::Closed);
        }
    }

    async fn heard(&mut self, runtime: Runtime, raw: Value) {
        if runtime == Runtime::Desktop && self.desktop_event(&raw).await {
            return;
        }
        let session = raw["sessionId"].as_str().map(str::to_owned);
        if let Some(session) = &session
            && let Some(page) = self
                .pages
                .iter_mut()
                .find(|page| page.runtime == runtime && page.owns_session(session))
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
                Some(parent) if event.target_info.r#type == "iframe" => self.attach_frame(runtime, parent, &event).await,
                _ if runtime == Runtime::Headless => self.attach(&event).await,
                _ => {}
            },
            CdpEvent::TargetTargetInfoChanged(event) => {
                let info = &event.target_info;
                for frame in self.pages.iter_mut().flat_map(|page| page.frames.iter_mut()) {
                    if frame.target == info.target_id.as_ref() {
                        frame.url.clone_from(&info.url);
                    }
                }
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
                    .position(|page| page.target.as_deref() == Some(event.target_id.as_ref()));
                if let Some(at) = gone {
                    let page = self.pages.remove(at);
                    self.remember(&page).await;
                    self.told(&page.thread);
                }
            }
            event @ (CdpEvent::BrowserDownloadWillBegin(_) | CdpEvent::BrowserDownloadProgress(_)) => self.heard_download(event),
            CdpEvent::PageFileChooserOpened(event) => {
                if let Some(session) = session {
                    self.open_file_chooser(runtime, &session, &event);
                }
            }
            other => {
                if let Some(session) = session {
                    self.heard_of_page(runtime, &session, other).await;
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
    async fn heard_of_page(&mut self, runtime: Runtime, session: &str, event: CdpEvent) {
        if let CdpEvent::PageScreencastFrame(frame) = &event {
            self.heard_frame(runtime, session, frame).await;
            return;
        }
        let Some(page) = self
            .pages
            .iter_mut()
            .find(|page| page.runtime == runtime && page.owns_session(session))
        else {
            return;
        };
        let navigation = page.document_event(session, &event);
        let mut changed = navigation.is_some();
        match event {
            CdpEvent::PageJavascriptDialogOpening(dialog) => {
                page.dialog = Some(Dialog {
                    kind: serde_json::to_value(&dialog.r#type)
                        .ok()
                        .and_then(|kind| kind.as_str().map(str::to_owned))
                        .unwrap_or_default(),
                    message: dialog.message.clone(),
                    offered: dialog.default_prompt.clone().unwrap_or_default(),
                });
                changed = true;
            }
            CdpEvent::RuntimeBindingCalled(called) if called.name == TITLE_BINDING && !matches!(page.load, PageLoad::Failed { .. }) => {
                changed |= page.title != called.payload;
                page.title = called.payload;
            }
            CdpEvent::PageFrameNavigated(navigated) => {
                changed |= page
                    .chooser
                    .take_if(|chooser| navigation.is_some() || chooser.frame == navigated.frame.id.as_ref())
                    .is_some();
            }
            CdpEvent::PageFrameDetached(detached) => {
                changed |= page
                    .chooser
                    .take_if(|chooser| chooser.frame == detached.frame_id.as_ref())
                    .is_some();
            }
            CdpEvent::PageJavascriptDialogClosed(_) => {
                page.dialog = None;
                changed = true;
            }
            event => heard_console(page, event),
        }
        if changed {
            let (id, thread) = (page.id.clone(), page.thread.clone());
            if let Some(page) = self.page(&id) {
                self.remember(page).await;
            }
            self.told(&thread);
            if matches!(navigation, Some(DocumentChange::Navigated | DocumentChange::Settled)) {
                self.history(&id).await;
            }
        }
    }

    fn open_file_chooser(
        &mut self,
        runtime: Runtime,
        session: &str,
        event: &chromiumoxide_cdp::cdp::browser_protocol::page::EventFileChooserOpened,
    ) {
        let (Some(node), Ok(id)) = (&event.backend_node_id, self.host.commands.new_id()) else {
            return;
        };
        let Some(page) = self
            .pages
            .iter_mut()
            .find(|page| page.runtime == runtime && page.owns_session(session))
        else {
            return;
        };
        page.chooser = Some(FileChooser {
            shown: PageFileChooser {
                id,
                multiple: event.mode.as_ref() == "selectMultiple",
            },
            frame: event.frame_id.as_ref().to_owned(),
            session: session.to_owned(),
            node: *node.inner(),
        });
        let thread = page.thread.clone();
        self.told(&thread);
    }

    async fn heard_frame(&mut self, runtime: Runtime, session: &str, frame: &EventScreencastFrame) {
        let Some(cdp) = self.running(runtime).map(|running| running.cdp.clone()) else {
            return;
        };
        let _ = cdp
            .call(Some(session), "Page.screencastFrameAck", json!({ "sessionId": frame.session_id }))
            .await;
        let Some(page) = self
            .pages
            .iter_mut()
            .find(|page| page.runtime == runtime && page.session.as_deref() == Some(session))
        else {
            return;
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (width, height) = (frame.metadata.device_width as u32, frame.metadata.device_height as u32);
        let size = page.view_size();
        if !page.casts() || (width, height) != (size.width, size.height) {
            return;
        }
        if size.scale > 1.0 {
            queue_capture(page, &cdp, &mut self.captures);
        } else if let Ok(jpeg) = base64::engine::general_purpose::STANDARD.decode(AsRef::<str>::as_ref(&frame.data)) {
            painted(page, jpeg.into(), &self.events);
        }
    }

    /// Sets up a target the browser attached to, held at its start: the page that asked for it, or one a page of a
    /// thread's opened, made a page of that thread; any other let run and let be.
    /// Sets up a frame of another site's process a page's target attached to, held at its start: its scripts heard,
    /// its own frames attached to in turn, then let run — kept by the page whose session it came through.
    async fn attach_frame(
        &mut self,
        runtime: Runtime,
        parent: &str,
        event: &chromiumoxide_cdp::cdp::browser_protocol::target::EventAttachedToTarget,
    ) {
        let Some(running) = self.running(runtime) else { return };
        let cdp = running.cdp.clone();
        let session = event.session_id.as_ref().to_owned();
        let owner = self
            .pages
            .iter_mut()
            .find(|page| page.runtime == runtime && page.owns_session(parent));
        if let Some(page) = owner {
            page.frames.push(FrameTarget {
                target: event.target_info.target_id.as_ref().to_owned(),
                session: session.clone(),
                url: event.target_info.url.clone(),
            });
        }
        let _ = cdp.call(Some(&session), "Runtime.enable", json!({})).await;
        let _ = intercept_file_chooser(&cdp, &session).await;
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
        let Some(running) = &self.headless else { return };
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
            let Ok(id) = self.host.commands.new_id() else { return };
            let mut page = Page::new(id, thread, size, None, Runtime::Headless);
            page.url.clone_from(&info.url);
            page.title.clone_from(&info.title);
            page.target = Some(target.clone());
            self.pages.push(page);
        }
        let ours = self.pages.iter().position(|page| page.target.as_deref() == Some(target.as_str()));
        if let Some(at) = ours {
            let (size, going) = (self.pages[at].view_size(), self.pages[at].going.take());
            if set_up(&cdp, &session, size).await.is_ok() && start_screencast(&cdp, &session).await.is_ok() {
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

    fn finished_capture(&mut self, capture: Captured) {
        let cdp = self
            .page(&capture.page)
            .and_then(|page| self.running(page.runtime))
            .map(|running| running.cdp.clone());
        let Some(page) = self.pages.iter_mut().find(|page| {
            page.id == capture.page && page.session.as_deref() == Some(&capture.session) && page.target.as_deref() == Some(&capture.target)
        }) else {
            return;
        };
        let dirty = page.paint.dirty;
        page.paint = Paint::default();
        let size = page.view_size();
        if page.casts()
            && size == capture.size
            && let Some(jpeg) = capture.jpeg
        {
            painted(page, jpeg, &self.events);
        }
        if dirty
            && page.casts()
            && size.scale > 1.0
            && let Some(cdp) = cdp
        {
            queue_capture(page, &cdp, &mut self.captures);
        }
    }
}

async fn next_event(running: &mut Option<Running>) -> Option<Value> {
    match running {
        Some(running) => running.events.recv().await,
        None => std::future::pending().await,
    }
}

fn heard_console(page: &mut Page, event: CdpEvent) {
    match event {
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

/// A screencast invalidates the view. A screenshot samples Chromium's emulated DPR without changing input coordinates.
fn queue_capture(page: &mut Page, cdp: &Cdp, captures: &mut JoinSet<Captured>) {
    if page.paint.capturing {
        page.paint.dirty = true;
        return;
    }
    let (Some(session), Some(target)) = (page.session.clone(), page.target.clone()) else {
        return;
    };
    page.paint.capturing = true;
    let (page, size, cdp) = (page.id.clone(), page.view_size(), cdp.clone());
    captures.spawn(async move {
        let jpeg = cdp.screenshot(&session, FRAME_QUALITY).await.ok();
        Captured {
            page,
            session,
            target,
            size,
            jpeg,
        }
    });
}

fn painted(page: &mut Page, jpeg: Bytes, events: &broadcast::Sender<Change>) {
    let size = page.view_size();
    page.frame_number += 1;
    let number = page.frame_number;
    page.frame = Some(Frame {
        jpeg,
        width: size.width,
        height: size.height,
        number,
    });
    let _ = events.send(Change::Painted {
        page: page.id.clone(),
        number,
    });
}

fn view(page: &Page) -> PageView {
    PageView {
        id: page.id.clone(),
        thread: page.thread.clone(),
        url: page.url.clone(),
        title: page.title.clone(),
        size: page.view_size(),
        viewport_size: page.size,
        viewport: page.viewport,
        zoom: page.zoom,
        host: page.runtime,
        dev_tools: page.dev_tools,
        downloads: page
            .downloads
            .iter()
            .map(|item| crate::protocol::PageDownload {
                id: item.id.clone(),
                name: item.name.clone(),
                state: item.state.clone(),
            })
            .collect(),
        chooser: page.chooser.as_ref().map(|chooser| chooser.shown.clone()),
        dialog: page.dialog.clone(),
        load: page.load.clone(),
        back: page.back,
        forward: page.forward,
        controller: page.control.controller.clone(),
    }
}

/// Sets up a page's target before it runs: its page, its scripts, its accessibility and its network heard, its title watched, laid
/// out at `size`, and painted as it changes.
async fn set_up(cdp: &Cdp, session: &str, size: Size) -> Result<(), CdpError> {
    for domain in ["Runtime.enable", "DOM.enable", "Accessibility.enable", "Network.enable"] {
        cdp.call(Some(session), domain, json!({})).await?;
    }
    intercept_file_chooser(cdp, session).await?;
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
    Ok(())
}

async fn intercept_file_chooser(cdp: &Cdp, session: &str) -> Result<(), CdpError> {
    cdp.call(Some(session), "Page.enable", json!({})).await?;
    cdp.call(Some(session), "Page.setInterceptFileChooserDialog", json!({ "enabled": true }))
        .await?;
    Ok(())
}

async fn lay_out(cdp: &Cdp, session: &str, size: Size) -> Result<(), CdpError> {
    let metrics = json!({ "width": size.width, "height": size.height, "deviceScaleFactor": size.scale, "mobile": false });
    cdp.call(Some(session), "Emulation.setDeviceMetricsOverride", metrics).await?;
    Ok(())
}

/// Starts with a fresh frame, including after a static page changes its viewport.
async fn start_screencast(cdp: &Cdp, session: &str) -> Result<(), CdpError> {
    let screencast = json!({ "format": "jpeg", "quality": FRAME_QUALITY, "everyNthFrame": 1 });
    cdp.call(Some(session), "Page.startScreencast", screencast).await?;
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
    let machine = host.commands.open(&open, base, &Writes::Anywhere)?;
    for own in host.profile.iter().chain(&host.downloads) {
        std::fs::create_dir_all(own).map_err(|error| BrowserError::Folders(error.to_string()))?;
    }
    let spec = Spec {
        stdio: Some(Stdio::Channel),
        ..Spec::new(Exec::Argv(argv), home.to_string_lossy())
    };
    let (command, told) = host.commands.spawn(machine.namespace(), &spec)?;
    let (cdp, events) = connect(command, told);
    Ok(Running {
        cdp,
        events,
        _machine: Some(machine),
    })
}

#[cfg(test)]
mod tests {
    use super::{BrowserError, Control, Runtime, Size};
    use crate::protocol::ControlAction::{Release, Take};

    #[test]
    fn zoom_changes_css_dimensions_and_keeps_native_capture_density() {
        let viewport = Size {
            width: 960,
            height: 540,
            scale: 2.0,
        };
        assert_eq!(
            viewport.zoomed(125, Runtime::Desktop),
            Size {
                width: 768,
                height: 432,
                scale: 2.0
            }
        );
        assert_eq!(
            viewport.zoomed(125, Runtime::Headless),
            Size {
                width: 768,
                height: 432,
                scale: 2.5
            }
        );
    }

    #[test]
    fn control_waits_for_input_and_lasts_until_the_last_view_leaves() {
        let mut control = Control::default();
        assert!(matches!(control.change(Take, "viewer".into()), Err(BrowserError::PageNotViewed)));
        control.watch("viewer".into());
        control.watch("viewer".into());
        control.admit(None).unwrap();
        assert!(matches!(control.change(Take, "viewer".into()), Err(BrowserError::PageBusy)));
        control.actions -= 1;
        control.change(Take, "viewer".into()).unwrap();
        assert!(matches!(control.admit(None), Err(BrowserError::PageControlled)));
        assert!(matches!(control.admit(Some("another")), Err(BrowserError::PageControlled)));
        control.admit(Some("viewer")).unwrap();
        assert!(matches!(control.change(Release, "viewer".into()), Err(BrowserError::PageBusy)));
        control.unwatch("viewer");
        control.actions -= 1;
        control.release_if_unwatched();
        assert_eq!(control.controller.as_deref(), Some("viewer"));
        control.unwatch("viewer");
        control.admit(None).unwrap();
    }
}
