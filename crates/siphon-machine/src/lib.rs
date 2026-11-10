//! The machine a piece of work reaches through a namespace of Beak's: paths as the model writes them, files read,
//! written and edited, folders paged, searched as ripgrep searches, and programs run to their end — each held to what
//! the namespace reaches.

mod files;
mod machine;
pub mod paths;
mod search;

pub use files::{Change, EditOutcome, Error, Read, ReadOptions, media_type};
pub use machine::{Host, Machine, Program, Ran, RunError, ThreadsView};
pub use search::{Globbed, GrepError, GrepFile, GrepLine, GrepOptions, Grepped, Walked};
