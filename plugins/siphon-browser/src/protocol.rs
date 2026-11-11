//! The browser's methods, events and topics, as its plugin adds them to the protocol: a page of a thread's opened,
//! moved, answered and closed, and the person's own mouse, keys and text on it; the pages of a thread as they change,
//! and a page painted, each frame the newest the browser painted of it.

use serde::{Deserialize, Serialize};
use siphon_protocol::{Nothing, Scope};
use ts_rs::TS;

siphon_protocol::methods! {
    events: BrowserEvent, topics: BrowserTopic;
    /// A page opened in a thread's browser, at an address or blank, sized as the person's view of it is: its id.
    "browser.open" Open(OpenPage) -> PageOpened, Some(Scope::BrowserOperate);
    /// A page taken to an address.
    "browser.navigate" Navigate(Navigate) -> Nothing, Some(Scope::BrowserOperate);
    /// A page taken back in its history.
    "browser.back" Back(OfPage) -> Nothing, Some(Scope::BrowserOperate);
    /// A page taken forward in its history.
    "browser.forward" Forward(OfPage) -> Nothing, Some(Scope::BrowserOperate);
    /// A page loaded again.
    "browser.reload" Reload(OfPage) -> Nothing, Some(Scope::BrowserOperate);
    /// The dialog that holds a page answered: accepted, with the text a prompt is given, or dismissed.
    "browser.dialog" Dialog(AnswerDialog) -> Nothing, Some(Scope::BrowserOperate);
    /// A page closed.
    "browser.close" Close(OfPage) -> Nothing, Some(Scope::BrowserOperate);
    /// A page's view sized anew.
    "browser.resize" Resize(ResizePage) -> Nothing, Some(Scope::BrowserOperate);
    /// The person's mouse on a page.
    "browser.mouse" Mouse(OnPage<PageMouse>) -> Nothing, Some(Scope::BrowserOperate);
    /// A key the person pressed or let go on a page.
    "browser.key" Key(OnPage<PageKey>) -> Nothing, Some(Scope::BrowserOperate);
    /// Text the person typed into a page at once, as an input method commits it.
    "browser.text" Text(TypeText) -> Nothing, Some(Scope::BrowserOperate);
}

/// A page, by its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OfPage {
    pub page: String,
}

/// A page's view: CSS pixels across and down, and device pixels to each — one, where none is given.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct View {
    pub width: u32,
    pub height: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scale: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct OpenPage {
    pub thread: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    pub view: View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PageOpened {
    pub page: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Navigate {
    pub page: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AnswerDialog {
    pub page: String,
    pub accept: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ResizePage {
    pub page: String,
    pub view: View,
}

/// What the person did on a page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct OnPage<E> {
    pub page: String,
    pub event: E,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TypeText {
    pub page: String,
    pub text: String,
}

/// What the mouse did: pressed, let go, moved, or its wheel turned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MouseMotion {
    Down,
    Up,
    Move,
    Wheel,
}

/// A mouse button, as the DOM names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum PageButton {
    None,
    Left,
    Middle,
    Right,
}

/// The mouse over a page, at a point of its CSS pixels: what it did, the button pressed or let go, the buttons held
/// and the modifiers, as the DOM counts them, how many clicks a press makes, and how far the wheel turned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PageMouse {
    pub motion: MouseMotion,
    pub x: f64,
    pub y: f64,
    #[serde(default = "no_button")]
    pub button: PageButton,
    #[serde(default)]
    pub buttons: u32,
    #[serde(default)]
    pub clicks: u32,
    #[serde(default)]
    pub modifiers: u32,
    #[serde(default)]
    pub delta_x: f64,
    #[serde(default)]
    pub delta_y: f64,
}

fn no_button() -> PageButton {
    PageButton::None
}

/// A key pressed or let go on a page: its name and the key it is on, as the DOM names them, the text it types, the
/// modifiers held, its key code, and the editing commands it stands for as the person's platform binds its shortcuts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PageKey {
    pub pressed: bool,
    pub key: String,
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
    #[serde(default)]
    pub modifiers: u32,
    #[serde(default)]
    pub key_code: u32,
    #[serde(default)]
    pub commands: Vec<String>,
}

/// A dialog that holds a page till it is answered: its kind — alert, confirm, prompt, beforeunload — its message, and
/// the text a prompt offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PageDialog {
    pub kind: String,
    pub message: String,
    pub offered: String,
}

/// A page of a thread's: its id, where it is and what it is called, its view, the dialog that holds it, whether it
/// loads, and whether its history goes back and forward from where it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PageListed {
    pub id: String,
    pub url: String,
    pub title: String,
    pub view: View,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dialog: Option<PageDialog>,
    pub loading: bool,
    pub back: bool,
    pub forward: bool,
}

/// What a client subscribes to of the browser's: a thread's pages, or a page as it is painted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserTopic {
    Pages { thread: String },
    Page { page: String },
}

/// An event of a subscription to the browser's: a thread's pages as they changed; or a frame of a page, the newest the
/// browser painted — its number among the page's, its size in device pixels — sent as a binary frame whose bytes after
/// this event are the frame's JPEG.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum BrowserEvent {
    Pages {
        subscription: String,
        pages: Vec<PageListed>,
    },
    Frame {
        subscription: String,
        number: u64,
        width: u32,
        height: u32,
    },
}
