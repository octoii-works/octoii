//! The browser of the threads: one headless Chromium spoken to over its DevTools pipe on Beak's channel; its pages each
//! a thread's — painted for the person; read as a screen reader reads them, painted for the model to see, and acted on
//! as a person does, each action waiting till the page settles and telling what came of it.

mod act;
mod browser;
pub mod cdp;
mod keys;
mod page;
mod snapshot;

pub use act::{ActError, Action, Button, Target};
pub use browser::{Attached, Browser, BrowserError, BrowserEvent, BrowserHost, Dialog, Download, Frame, Message, Network, PageView, Size};
pub use cdp::{Cdp, CdpError};
pub use page::{Evaluated, Landed, Page, Probe, Wanted};
pub use snapshot::{Snapshot, node_of, ref_of};
