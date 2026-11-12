//! The agent: a thread's main process and its turns, what its model is told of itself, its work and its tools, and the
//! conversation it is sent, rendered from its log; the calls its replies make taken on; its title, its compaction and
//! its sub-threads; and the second model that judges its calls in auto.

pub(crate) mod compaction;
pub(crate) mod context;
pub mod front_matter;
pub(crate) mod gather;
pub(crate) mod model;
pub(crate) mod primitives;
pub(crate) mod render;
pub(crate) mod resume;
pub(crate) mod review;
pub(crate) mod sections;
pub(crate) mod thread;
pub(crate) mod title;
pub(crate) mod told;
pub(crate) mod turn;
pub(crate) mod waiting;

pub(crate) use compaction::CompactProgram;
pub(crate) use context::Composition;
pub(crate) use gather::{CORE, Gather, OwnedTeller, Sources, answer_gather};
pub(crate) use model::{ModelRequest, Streamed, answer_model};
pub(crate) use primitives::ThreadPrimitives;
pub(crate) use resume::{ResumeInput, ResumeProgram};
pub(crate) use review::answer_review;
pub(crate) use thread::ThreadProgram;
pub(crate) use title::TitleProgram;

pub(crate) use turn::TurnProgram;
pub(crate) use waiting::DelegatedProgram;

/// What extensions and hosts use of it.
pub use gather::{Shared, Teller, Telling};
pub use primitives::{deliver, parent_of};
pub use told::Told;
pub use waiting::how;
