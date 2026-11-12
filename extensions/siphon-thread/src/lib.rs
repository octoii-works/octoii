//! Threads as an extension of Siphon's: the `thread` tool, by which a thread's model starts, messages and stops the
//! person's other threads where the person asks it to; and the threads as a folder of the machine's, which the model
//! reads with the file tools — each thread's log, outputs and files — told it where the host lays them out.

mod tool;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use siphon::agent::{Shared, Teller, Telling};
use siphon::tools::projects::Place;
use siphon::{Extension, Registrar};
use siphon_runtime::Runtime;
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
        "The model starts, messages and stops the person's other threads when asked, and reads what threads did before."
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
        _: &'a Runtime,
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

/// Where the threads are, as the model is told it, where the host lays them out: each a folder by its id, read alone
/// through the file tools; and this thread's id.
fn threads_told(folder: &Path, id: &str) -> String {
    format!(
        "<threads>
Each thread of this machine is a folder in {}, named by its id; this thread's is {id}. Read them with the file tools; nothing writes them.
- log/: its conversation as JSON Lines, an entry a line, in files that sort in the order they began. An entry has an id, rising along the log; a type — input (what the person sent), reply (the model's response), result (a tool's output), turn-end, config (a change of settings), system (what the model was told), discarded (an answer that came to nothing); and its fields.
- outputs/: the whole output of a call too long to be given whole.
- files/: the images and files of its conversation, named by their SHA-256.
Read them when the person asks about another thread or about work done before.
</threads>",
        folder.display()
    )
}
