//! The browser of the threads: one headless Chromium spoken to over its DevTools pipe on Beak's channel, its pages each
//! a thread's. The Agent works it through one interface, `Browser::act`: an `Action` — tabs, navigation, reading,
//! looking, a person's input, scripts, waits, the console, downloads, raw CDP — and what came of it, an `Outcome` or a
//! `Refusal` in words the model reads; the same action a tool's call or a script's. The person sees the pages painted,
//! by their frames and the events that tell of them.

mod action;
mod browser;
mod cdp;
mod input;
mod keys;
mod page;
mod plugin;
mod primitives;
pub mod protocol;
mod snapshot;
mod tool;

pub use action::{Action, MouseButton, Outcome, Place, Refusal, Shown};
pub use browser::{Browser, BrowserError, BrowserHost, Change, Frame, PageView, Size};
pub use plugin::BrowserPlugin;
pub use tool::BrowserTool;
