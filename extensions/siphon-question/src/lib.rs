//! Questions as an extension of Siphon's: the `question` tool, by which the model asks the person some questions at
//! once, each with options to choose from, and is told what they answered.

mod context;

mod question;

pub use question::{Asked, Choice, QuestionAnswer, QuestionArgs, QuestionResult, QuestionTool};
use siphon::{Extension, Registrar};

/// Questions, as a host adds them.
#[derive(Debug, Clone, Copy, Default)]
pub struct QuestionExtension;

impl Extension for QuestionExtension {
    fn name(&self) -> &'static str {
        "question"
    }

    fn title(&self) -> &'static str {
        "Questions"
    }

    fn description(&self) -> &'static str {
        "The model asks the person questions with options to choose from, when it cannot find out itself."
    }

    fn build(self: Box<Self>, registrar: &mut Registrar<'_>) {
        registrar.teller(siphon::agent::Shared::Machine, std::sync::Arc::new(crate::context::Context));
        registrar.tool(QuestionTool);
    }
}
