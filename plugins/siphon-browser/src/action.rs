//! What the Agent does in the browser, through one interface: an `Action` — its tabs opened, switched and closed; a
//! page taken to an address, back, forward, loaded again; read as a screen reader reads it, looked at, measured; given
//! a person's input; its scripts run, in a frame of another site's process too; waited on; its console and downloads
//! read; raw CDP where nothing else does — each on the thread's current page unless it names one. The same `Action` is
//! a tool's call and a script's, so the Agent works a page a step at a time, or many steps in one script. What comes of
//! each is an `Outcome` — the page it left, a line of what came of it, and what it read — or a `Refusal` the model reads.

use std::fmt::Write as _;
use std::time::Duration;

use bytes::Bytes;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::browser::{Browser, BrowserError, Size};
use crate::input::{Button, Input, InputError, Target};
use crate::page::{Evaluated, Landed, Page, Wanted};
use crate::primitives::PageInfo;
use crate::snapshot::node_of;

/// The size a page the Agent opens is laid out at.
const OPENED: Size = Size {
    width: 1280,
    height: 800,
    scale: 1.0,
};

/// Where an input goes: an element by the ref a reading gave it, an element a CSS selector picks, or a point of the
/// page's view in CSS pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Place {
    Ref {
        /// The element's ref, as a reading gave it: `e12`.
        #[serde(rename = "ref")]
        reference: String,
    },
    Selector {
        /// A CSS selector.
        selector: String,
    },
    Point {
        /// Pixels from the view's left.
        x: f64,
        /// Pixels from the view's top.
        y: f64,
    },
}

/// A mouse button.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    #[default]
    Left,
    Middle,
    Right,
}

/// What the model does in the browser, on the thread's current page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    /// Opens a page, at `url` or blank, and makes it the current one.
    Open {
        /// The address to open; a blank page where none.
        url: Option<String>,
    },
    /// Lists the thread's pages.
    Tabs,
    /// Makes the page `page` the current one.
    Switch {
        /// The page's id, as tabs gives it.
        page: String,
    },
    /// Closes the page `page`, or the current one.
    Close {
        /// The page's id, as tabs gives it; the current page where none.
        page: Option<String>,
    },
    /// Goes to `url` in the current page.
    Navigate {
        /// The address to go to.
        url: String,
    },
    /// Goes back in the page's history.
    Back,
    /// Goes forward in the page's history.
    Forward,
    /// Loads the page again.
    Reload,
    /// Reads the page as a screen reader does: a line for each thing on it, nested, its element's ref, refs new since
    /// the last reading marked `*`.
    Read,
    /// Shows the page as it is painted now.
    Look,
    /// Tells the page's address, title, view, scroll and size, or the dialog that holds it.
    Info,
    /// Clicks `target`, brought into view first.
    Click {
        /// What to click.
        target: Place,
        /// The button; left where none.
        #[serde(default)]
        button: MouseButton,
        /// 2 for a double click.
        clicks: Option<u32>,
    },
    /// Moves the mouse over `target`.
    Hover {
        /// What to hover over.
        target: Place,
    },
    /// Types `text` at the caret of `target`, focused first, or of what has the focus; `clear` replaces what is there,
    /// `submit` presses Enter after.
    Type {
        /// Where to type; what has the focus where none.
        target: Option<Place>,
        /// The text to type.
        text: String,
        /// Replace what is there.
        #[serde(default)]
        clear: bool,
        /// Press Enter after.
        #[serde(default)]
        submit: bool,
    },
    /// Presses a key as the DOM names it — `Enter`, `Tab`, `ArrowDown`, `a` — with modifiers: Alt, Control, Meta, Shift.
    Press {
        /// The key, as the DOM names it.
        key: String,
        /// The keys held while it is pressed.
        #[serde(default)]
        modifiers: Vec<String>,
    },
    /// Chooses options of a `<select>`, by value or label.
    Select {
        /// The `<select>`.
        target: Place,
        /// The options to choose, by value or label.
        values: Vec<String>,
    },
    /// Scrolls by pixels over `target`, or the page's middle.
    Scroll {
        /// What to scroll over; the page's middle where none.
        target: Option<Place>,
        /// Pixels to the right; to the left below 0.
        #[serde(default)]
        delta_x: f64,
        /// Pixels down; up below 0.
        #[serde(default)]
        delta_y: f64,
    },
    /// Drags from `from` to `to`, the left button held.
    Drag {
        /// Where the drag begins.
        from: Place,
        /// Where it ends.
        to: Place,
    },
    /// Gives files of the machine, by absolute paths, to a file input.
    Upload {
        /// The file input.
        target: Place,
        /// The files, by absolute paths.
        files: Vec<String>,
    },
    /// Answers the dialog that holds the page; `text` for a prompt.
    Dialog {
        /// Accept it, or dismiss it.
        accept: bool,
        /// What to answer a prompt with.
        text: Option<String>,
    },
    /// Runs a JavaScript expression as the page's own script — or a frame's, of another site's process, whose address
    /// holds `frame` — awaiting a promise it gives: its value as JSON, or what it threw.
    Evaluate {
        /// A JavaScript expression.
        expression: String,
        /// Part of the address of the frame to run it in; the page where none.
        frame: Option<String>,
    },
    /// Waits till the page shows `text`, no longer shows `gone`, is at an address holding `url`, has an element
    /// `selector` picks — drawn, with `visible` — and its network has been quiet `network_idle_ms`: each given, case
    /// counting; `timeout_ms` at most, 10 s where none.
    Wait {
        /// Text the page is to show.
        text: Option<String>,
        /// Text the page is to show no longer.
        gone: Option<String>,
        /// Part of the address the page is to be at.
        url: Option<String>,
        /// A CSS selector of an element the page is to have.
        selector: Option<String>,
        /// The element drawn, not only there.
        #[serde(default)]
        visible: bool,
        /// How long the network is to have been quiet, in ms.
        network_idle_ms: Option<u64>,
        /// How long to wait at most, in ms.
        timeout_ms: Option<u64>,
    },
    /// The messages of the page's console and the errors its scripts threw, numbered past `since`.
    Console {
        /// The number of the last message read already; every message where none.
        since: Option<u64>,
    },
    /// What the page downloaded, or downloads.
    Downloads,
    /// A raw CDP call on the page's target — or a frame's, whose address holds `frame` — where nothing else does.
    Cdp {
        /// The method, as `DOM.getDocument`.
        method: String,
        /// Its parameters.
        #[serde(default)]
        params: Value,
        /// Part of the address of the frame to call on; the page's own where none.
        frame: Option<String>,
    },
}

/// A page, as an outcome names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shown {
    pub id: String,
    pub url: String,
    pub title: String,
}

/// What came of an action: the page it left, what came of it in words the model reads, and what it read — a value of
/// a script's, a picture of the page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outcome {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<Shown>,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// A JPEG of the page, for `look`.
    #[serde(skip)]
    pub image: Option<Bytes>,
}

/// Why an action was not done, in words the model reads.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Refusal(pub String);

impl From<BrowserError> for Refusal {
    fn from(error: BrowserError) -> Self {
        Self(error.to_string())
    }
}

impl From<InputError> for Refusal {
    fn from(error: InputError) -> Self {
        Self(error.to_string())
    }
}

fn said(text: impl Into<String>) -> Outcome {
    Outcome {
        page: None,
        text: text.into(),
        value: None,
        image: None,
    }
}

impl Browser {
    /// Does `action` for the agent of `thread`: what came of it.
    ///
    /// # Errors
    /// A `Refusal`, saying why, where it could not be done as asked.
    pub async fn act(&self, thread: &str, action: Action) -> Result<Outcome, Refusal> {
        let mut outcome = match action {
            Action::Open { url } => self.opened(thread, url.as_deref()).await?,
            Action::Tabs => self.tabs(thread).await,
            Action::Switch { page } => {
                self.switch(thread, &page)
                    .await
                    .map_err(|_| Refusal(format!("no page {page} of the thread's; list them with tabs")))?;
                said(format!("page {page} is the current one"))
            }
            Action::Close { page } => {
                let page = match page {
                    Some(page) => page,
                    None => self.current_page(thread).await?.id,
                };
                self.close(&page);
                said(format!("closed page {page}"))
            }
            on_page => {
                let page = self.current_page(thread).await?;
                self.on_page(&page, on_page).await?
            }
        };
        if outcome.page.is_none()
            && let Some(id) = self.current(thread).await
            && let Some(view) = self.view(&id).await
        {
            outcome.page = Some(Shown {
                id,
                url: view.url,
                title: view.title,
            });
        }
        Ok(outcome)
    }

    /// The page of the thread's the agent works now.
    async fn current_page(&self, thread: &str) -> Result<Page, Refusal> {
        let id = self
            .current(thread)
            .await
            .ok_or_else(|| Refusal("no page is open; open one first".into()))?;
        Ok(self.page(&id))
    }

    async fn opened(&self, thread: &str, url: Option<&str>) -> Result<Outcome, Refusal> {
        let id = self.open(thread, url, OPENED).await?;
        let page = self.page(&id);
        page.attached().await?;
        page.wait_for_load(Duration::from_secs(15)).await;
        Ok(said(format!("opened page {id}")))
    }

    async fn tabs(&self, thread: &str) -> Outcome {
        let current = self.current(thread).await;
        let mut text = String::new();
        for page in self.pages(thread).await {
            let mark = if current.as_deref() == Some(page.id.as_str()) {
                " (current)"
            } else {
                ""
            };
            let _ = writeln!(text, "{}{mark}: {} {}", page.id, serde_json::json!(page.title), page.url);
        }
        said(if text.is_empty() { "no pages".into() } else { text })
    }

    /// Does `action`, one of a page's, on `page`.
    async fn on_page(&self, page: &Page, action: Action) -> Result<Outcome, Refusal> {
        match action {
            Action::Navigate { url } => Ok(landed(page.navigate(&url).await?)),
            Action::Back => went(page.go(-1).await?, "no entry back in its history"),
            Action::Forward => went(page.go(1).await?, "no entry forward in its history"),
            Action::Reload => Ok(landed(page.reload().await?)),
            Action::Read => self.read(page).await,
            Action::Look => {
                let image = page.screenshot().await?;
                Ok(Outcome {
                    image: Some(image),
                    ..said("the page as it is painted now")
                })
            }
            Action::Info => Ok(said(info_text(&page.info().await?))),
            Action::Evaluate { expression, frame } => {
                let session = frame_of(page, frame.as_deref()).await?;
                match page.js(&expression, session.as_deref()).await? {
                    Evaluated::Value(value) => Ok(Outcome {
                        value: Some(value),
                        ..said("ran")
                    }),
                    Evaluated::Threw(words) => Err(Refusal(format!("the script threw: {words}"))),
                }
            }
            Action::Wait {
                text,
                gone,
                url,
                selector,
                visible,
                network_idle_ms,
                timeout_ms,
            } => {
                let wanted = Wanted { text, gone, url };
                let timeout = Duration::from_millis(timeout_ms.unwrap_or(10_000));
                wait(page, &wanted, selector.as_deref(), visible, network_idle_ms, timeout).await
            }
            Action::Console { since } => Ok(self.console(page, since.unwrap_or_default()).await),
            Action::Downloads => Ok(self.downloaded(page).await),
            Action::Cdp { method, params, frame } => {
                let params = if params.is_null() { serde_json::json!({}) } else { params };
                let value = match frame_of(page, frame.as_deref()).await? {
                    Some(session) => page.cdp_on(&session, &method, params).await?,
                    None => page.cdp(&method, params).await?,
                };
                Ok(Outcome {
                    value: Some(value),
                    ..said("called")
                })
            }
            input => {
                let input = input_of(page, input).await?;
                Ok(said(page.input(&input).await?))
            }
        }
    }

    async fn read(&self, page: &Page) -> Result<Outcome, Refusal> {
        let known = self.swap_refs(&page.id, Vec::new()).await;
        let reading = page.snapshot(&known).await?;
        self.swap_refs(&page.id, reading.refs.clone()).await;
        let mut text = reading.text;
        if let Some(dialog) = self.view(&page.id).await.and_then(|view| view.dialog) {
            text = format!(
                "a {} dialog holds the page: {}\n{text}",
                dialog.kind,
                serde_json::json!(dialog.message)
            );
        }
        Ok(said(text))
    }

    async fn console(&self, page: &Page, since: u64) -> Outcome {
        let mut text = String::new();
        for message in self.messages(&page.id, since).await {
            let _ = writeln!(text, "#{} {}: {}", message.number, message.level, message.text);
        }
        said(if text.is_empty() { "no messages".into() } else { text })
    }

    async fn downloaded(&self, page: &Page) -> Outcome {
        let mut text = String::new();
        for download in self.downloads(&page.id).await {
            let kept = download.path.map(|path| format!(" at {path}")).unwrap_or_default();
            let _ = writeln!(
                text,
                "{} {}: {} of {} bytes{kept}",
                serde_json::json!(download.name),
                download.state,
                download.received_bytes,
                download.total_bytes
            );
        }
        said(if text.is_empty() { "no downloads".into() } else { text })
    }
}

fn landed(landed: Landed) -> Outcome {
    let text = match landed.failed {
        Some(failed) => format!("could not go there: {failed}; the page is at {}", landed.url),
        None => format!("at {}", landed.url),
    };
    Outcome {
        page: None,
        text,
        value: None,
        image: None,
    }
}

fn went(landed: Option<Landed>, none: &str) -> Result<Outcome, Refusal> {
    landed.map(self::landed).ok_or_else(|| Refusal(none.into()))
}

fn info_text(info: &PageInfo) -> String {
    match info {
        PageInfo::Shown {
            url,
            title,
            width,
            height,
            scroll_x,
            scroll_y,
            page_width,
            page_height,
        } => format!(
            "{} at {url}; its view {width}×{height}, scrolled to {scroll_x},{scroll_y} of {page_width}×{page_height}",
            serde_json::json!(title)
        ),
        PageInfo::Dialog { kind, message } => format!("a {kind} dialog holds the page: {}", serde_json::json!(message)),
    }
}

/// The session of the frame whose address holds `frame`, where one is named; refused where the page has none such.
async fn frame_of(page: &Page, frame: Option<&str>) -> Result<Option<String>, Refusal> {
    let Some(part) = frame else { return Ok(None) };
    let session = page.frame_session(part).await;
    session.map(Some).ok_or_else(|| {
        Refusal(format!(
            "no frame of another site's process at an address holding {}",
            serde_json::json!(part)
        ))
    })
}

/// The input `action`, one of a person's, as the page takes it: its places resolved to elements or points.
async fn input_of(page: &Page, action: Action) -> Result<Input, Refusal> {
    Ok(match action {
        Action::Click { target, button, clicks } => Input::Click {
            target: target_of(page, &target).await?,
            button: match button {
                MouseButton::Left => Button::Left,
                MouseButton::Middle => Button::Middle,
                MouseButton::Right => Button::Right,
            },
            clicks: clicks.unwrap_or(1),
        },
        Action::Hover { target } => Input::Hover {
            target: target_of(page, &target).await?,
        },
        Action::Type {
            target,
            text,
            clear,
            submit,
        } => Input::Type {
            node: match &target {
                Some(place) => Some(node_at(page, place).await?),
                None => None,
            },
            text,
            clear,
            submit,
        },
        Action::Press { key, modifiers } => Input::Press { key, modifiers },
        Action::Select { target, values } => Input::Select {
            node: node_at(page, &target).await?,
            values,
        },
        Action::Scroll { target, delta_x, delta_y } => Input::Scroll {
            target: match &target {
                Some(place) => Some(target_of(page, place).await?),
                None => None,
            },
            delta_x,
            delta_y,
        },
        Action::Drag { from, to } => Input::Drag {
            from: target_of(page, &from).await?,
            to: target_of(page, &to).await?,
        },
        Action::Upload { target, files } => Input::Upload {
            node: node_at(page, &target).await?,
            files,
        },
        Action::Dialog { accept, text } => Input::Dialog { accept, text },
        other => return Err(Refusal(format!("{other:?} is no input"))),
    })
}

async fn target_of(page: &Page, place: &Place) -> Result<Target, Refusal> {
    match place {
        Place::Point { x, y } => Ok(Target::Point { x: *x, y: *y }),
        element => Ok(Target::Node(node_at(page, element).await?)),
    }
}

/// The element `place` names.
async fn node_at(page: &Page, place: &Place) -> Result<i64, Refusal> {
    match place {
        Place::Ref { reference } => {
            node_of(reference).ok_or_else(|| Refusal(format!("{reference} is no ref; a reading gives refs as e12")))
        }
        Place::Selector { selector } => Ok(page.node(selector).await?),
        Place::Point { .. } => Err(Refusal("an element is named here, by its ref or a selector, not a point".into())),
    }
}

/// Waits till the page shows what is wanted.
async fn wait(
    page: &Page,
    wanted: &Wanted,
    selector: Option<&str>,
    visible: bool,
    idle_ms: Option<u64>,
    timeout: Duration,
) -> Result<Outcome, Refusal> {
    let start = std::time::Instant::now();
    let left = || timeout.saturating_sub(start.elapsed());
    let shown = page.wait(wanted, left()).await?;
    let element = match selector {
        Some(selector) => page.wait_for_element(selector, visible, left()).await?,
        None => true,
    };
    let quiet = match idle_ms {
        Some(idle) => page.wait_for_network_idle(Duration::from_millis(idle), left()).await,
        None => true,
    };
    if shown && element && quiet {
        Ok(said(format!("waited {} ms", start.elapsed().as_millis())))
    } else {
        Err(Refusal(format!(
            "the page did not show what was waited for in {} ms",
            timeout.as_millis()
        )))
    }
}
