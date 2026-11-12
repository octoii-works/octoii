//! The machine a piece of work reaches through a namespace of Beak's: paths as the model writes them, files read,
//! written and edited, folders paged, searched as ripgrep searches, and programs run to their end or beside the host as
//! coprocesses — each held to what the namespace reaches.

pub(crate) mod commands;
pub(crate) mod coprocess;
pub(crate) mod files;
pub(crate) mod namespace;
pub(crate) mod paths;
pub(crate) mod search;

pub(crate) use files::{EditOutcome, Is};
pub(crate) use namespace::Errors;
pub(crate) use search::{GrepOptions, Grepped};

/// The commands' own words, as Beak gives them: a command and its events, what starts one, how it ended, a
/// terminal's screen and a piece of what it wrote.
pub use beak::{Chunk, Command, CommandEvent, CommandEvents, Exec, Exit, Screen, Spec, Stdio};
/// What extensions and hosts use of it.
pub use commands::Commands;
pub use coprocess::{Coprocess, Stderr, Stdin, Stdout, Variables};
pub use files::{Error, Read, ReadOptions, media_type};
pub use namespace::{Host, Machine, Program, RunError, Writes, inside};
pub use paths::Base;
