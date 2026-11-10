//! The agent: a thread's main process and its turns, what its model is told of itself, its work and its tools, and the
//! conversation it is sent, rendered from its log; the calls its replies make taken on; and the second model that
//! judges its calls in auto.

pub mod gather;
pub mod model;
pub mod render;
pub mod skills;
pub mod thread;
pub mod told;
pub mod turn;

pub use gather::{Gather, Sources, answer_gather};
pub use model::{ModelRequest, answer_model};
pub use thread::{MainState, ThreadProgram};
pub use told::Told;
pub use turn::{TurnInput, TurnProgram, TurnState};
