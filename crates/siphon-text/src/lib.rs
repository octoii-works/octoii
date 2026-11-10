//! Text as files hold it: a page of its lines, a file's bytes as text and text as a file's bytes, an edit that finds
//! its text by tiers, and the diff between two texts — each a computation on the text alone.

mod diff;
mod edit;
mod file;
mod lines;

pub use diff::{CONTEXT_LINES, Diff};
pub use edit::{Edit, Edited, Refusal, Tier};
pub use file::{BYTE_ORDER_MARK, Text};
pub use lines::{Bounds, Line, Page, Paged};
