//! The machine a piece of work reaches through a namespace of Beak's: paths as the model writes them, files read,
//! written and edited, folders paged, searched as ripgrep searches, and programs run to their end or beside the host as
//! coprocesses — each held to what the namespace reaches.

mod coprocess;
mod files;
pub mod git;
mod namespace;
pub mod paths;
mod search;

pub use coprocess::{Coprocess, Stderr, Stdin, Stdout, Variables};
pub use files::{Change, EditOutcome, Error, Is, Missing, Read, ReadOptions, Stat, media_type};
pub use namespace::{Errors, Host, Machine, Program, Ran, RunError, ThreadsView, Writes};
pub use search::{Globbed, GrepError, GrepFile, GrepLine, GrepOptions, Grepped, Listed, Walked};
