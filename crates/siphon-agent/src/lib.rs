//! The agent: a thread's main process and its turns, what its model is told of itself, its work and its tools, and the
//! conversation it is sent, rendered from its log; the calls its replies make taken on; its title, its compaction and
//! its sub-threads; and the second model that judges its calls in auto.

pub mod compaction;
pub mod delegate;
pub mod gather;
pub mod model;
pub mod render;
pub mod resume;
pub mod review;
pub mod skills;
pub mod thread;
pub mod title;
pub mod told;
pub mod turn;

pub use compaction::{CompactInput, CompactProgram};
pub use delegate::{DelegateArgs, DelegateProgram, DelegateTool, DelegatedProgram};
pub use gather::{Gather, Sources, answer_gather};
pub use model::{ModelRequest, answer_model};
pub use resume::{ResumeInput, ResumeProgram};
pub use review::answer_review;
pub use thread::{MainState, ThreadProgram};
pub use title::{TitleInput, TitleProgram};
pub use told::Told;
pub use turn::{TurnInput, TurnProgram, TurnState};
