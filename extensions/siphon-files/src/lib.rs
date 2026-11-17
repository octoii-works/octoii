//! Files a thread presents to the person, as an extension of Siphon's: `present_file` keeps a file the thread may
//! read, embedding local images into HTML, then returns a resource link without adding its bytes to the model's context.

mod context;

mod tool;

use siphon::{Extension, Registrar};
pub use tool::{PresentFileArgs, PresentFileTool};

/// File presentation, as a host adds it.
#[derive(Debug, Clone, Copy, Default)]
pub struct FilesExtension;

impl Extension for FilesExtension {
    fn name(&self) -> &'static str {
        "files"
    }

    fn title(&self) -> &'static str {
        "Files"
    }

    fn description(&self) -> &'static str {
        "The model presents files to the person, with HTML's local images embedded, kept in the conversation for download."
    }

    fn build(self: Box<Self>, registrar: &mut Registrar<'_>) {
        registrar.teller(siphon::agent::Shared::Machine, std::sync::Arc::new(crate::context::Context));
        registrar.tool(PresentFileTool);
    }
}
