//! Threads as an extension of Siphon's: the `thread` tool, by which a thread's model lists, searches, reads, starts,
//! messages and stops the person's other threads where the person asks it to; and the threads as a folder of the
//! machine's, which the model reads with the file tools for what the tool leaves out — each thread's log, outputs and
//! files — told it where the host lays them out.

mod conversation;
mod reading;
mod result;
mod tool;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

pub use result::{
    ContentFile, ContentPart, ConversationContent, ConversationEntry, ModelContent, ReplyItem, ResultStatus, SearchHit, SettingChange,
    ThreadResult, ThreadStatus, ThreadSummary, TurnOutcome,
};
use siphon::agent::{Shared, Teller, Telling};
use siphon::tools::Place;
use siphon::{Extension, Registrar};
pub use tool::{ThreadAction, ThreadTool};

/// Threads, as a host adds them.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadExtension;

impl Extension for ThreadExtension {
    fn name(&self) -> &'static str {
        "thread"
    }

    fn title(&self) -> &'static str {
        "Threads"
    }

    fn description(&self) -> &'static str {
        "The model lists, searches, reads, starts, messages, stops and organizes the person's other threads when asked."
    }

    fn build(self: Box<Self>, registrar: &mut Registrar<'_>) {
        registrar.tool(ThreadTool);
        if let Some(folder) = registrar.machine().threads.clone() {
            registrar.teller(Shared::Thread, Arc::new(Folder(folder)));
        }
    }
}

/// The folder the threads are laid out in, as the model is told of it.
struct Folder(PathBuf);

impl Teller for Folder {
    fn tell<'a>(
        &'a self,
        _: &'a siphon::volume::Volume,
        thread: &'a str,
        _: Option<&'a Place>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Telling>> + Send + 'a>> {
        let telling = Telling {
            sections: vec![("threads".to_owned(), threads_told(&self.0, thread))],
            tools: Vec::new(),
        };
        Box::pin(std::future::ready(Ok(telling)))
    }
}

/// Where the threads are, as the model is told it, where the host lays them out: searched and read by the `thread` tool,
/// each a folder by its id for what that leaves out, read through the file tools alone, as they are no folder on the
/// disk; and this thread's id.
fn threads_told(folder: &Path, id: &str) -> String {
    format!(
        "<threads>
This thread is {id}. When the person asks about other threads or earlier work, find what was said with `thread`'s search and read the conversation around it with `thread`'s read. Every thread also has a read-only folder in {}, named by its id, for what read leaves out — reach it with read, glob and grep, or with tools.* in execute; the shell does not see it:
- log/: every entry of the conversation as JSON Lines, the model's reasoning and each call's whole result among them.
- outputs/: the whole output of calls too long to return whole.
- files/: the images and files of the conversation, named by their SHA-256.
</threads>",
        folder.display()
    )
}
