//! The thread, filesystem permissions and command capability of a tool request.

use std::path::PathBuf;

use octoii_ai::vocabulary::{FileRef, ImageDetail, InputContent};
use octoii_siphon_lib::{RequestCommands, RuntimeHandle};
use tokio_util::sync::CancellationToken;

use crate::machine::{Base, Host, Machine};
use crate::tools::Refusal;
use crate::tools::permissions::Writes;

/// What a call's exec is given: the thread whose model called it, its key, where its paths start, where it may write,
/// and the command capability for its request. A stop cancels its token so it can return what has come.
#[derive(Debug, Clone)]
pub struct Context {
    pub thread: String,
    pub call: String,
    pub base: Base,
    pub writes: Writes,
    /// The places beside the workspace its work writes as freely.
    pub places: Vec<PathBuf>,
    pub host: Host,
    pub(crate) runtime: RuntimeHandle,
    pub(crate) request_commands: RequestCommands,
    pub cancelled: CancellationToken,
}

impl Context {
    /// The command capability for this exact call's execution request.
    #[must_use]
    pub fn commands(&self) -> &RequestCommands {
        &self.request_commands
    }

    /// The volume, as an extension reaches it.
    #[must_use]
    pub fn volume(&self) -> crate::volume::Volume {
        crate::volume::Volume {
            runtime: self.runtime.clone(),
        }
    }

    /// The machine, opened for this call: held to the thread's workspace and the places beside it written as freely, or
    /// writing anywhere, as its weighing said; the threads shown where the host lays them out.
    ///
    /// # Errors
    /// Beak's failure to open its namespace.
    pub fn machine(&self) -> Result<Machine, Refusal> {
        Machine::open(
            &self.runtime,
            &self.host,
            self.base.clone(),
            &self.writes_of(),
            Some(crate::threads::thread::VIEW),
        )
        .map_err(|error| Refusal(format!("the machine: {error}")))
    }

    /// Where this call's work may write, as its weighing said: its thread's workspace and the places beside it written
    /// as freely, made where they are not, or anywhere.
    #[must_use]
    pub fn writes_of(&self) -> crate::machine::Writes {
        match self.writes {
            Writes::Anywhere => crate::machine::Writes::Anywhere,
            Writes::Workspace => {
                let places = self.places.iter().filter(|place| made(place)).cloned();
                crate::machine::Writes::Within(std::iter::once(self.base.workspace.clone()).chain(places).collect())
            }
        }
    }

    /// The variables a thread's commands run with over the machine's own: `TMPDIR`, its scratchpad, so what their work
    /// leaves for a while goes there rather than into the workspace.
    #[must_use]
    pub fn variables(&self) -> Vec<(String, String)> {
        self.scratchpad()
            .map(|scratchpad| ("TMPDIR".to_owned(), scratchpad.to_string_lossy().into_owned()))
            .into_iter()
            .collect()
    }

    /// The thread's scratchpad, made where it is not — the system clears its temporary folder.
    fn scratchpad(&self) -> Option<PathBuf> {
        self.host.scratchpad_of(&self.thread).filter(made)
    }

    /// Keeps the original bytes of the regular file at `path`, reached through this call's machine, in the thread's
    /// files by their content. The file keeps its basename and is never fitted for a model. Its reference and size
    /// are returned only after storage commits; a call interrupted before its result commits may leave a blob until
    /// the thread is removed, as `show_image` can.
    ///
    /// # Errors
    /// An invalid path, an inaccessible or non-regular file, more than 32 MiB, or a failure to keep the bytes.
    pub async fn keep_file(&self, path: &str) -> Result<(FileRef, u64), Refusal> {
        if path.is_empty() || path.contains('\0') {
            return Err(Refusal::new("invalid file path"));
        }
        let context = self.clone();
        let path = path.to_owned();
        let (name, bytes) = tokio::task::spawn_blocking(move || {
            let machine = context.machine()?;
            let (path, bytes) = machine
                .read_file(&machine.path(&path), crate::threads::files::BYTES_MAX as u64)
                .map_err(|error| Refusal(format!("{}: {error}", machine.shown(&machine.path(&path)))))?;
            let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
            Ok::<_, Refusal>((name, bytes))
        })
        .await
        .map_err(|error| Refusal(format!("the file read failed: {error}")))??;
        self.volume()
            .keep_file(&self.thread, bytes, &name)
            .await
            .map_err(|error| Refusal(format!("{name}: {error}")))
    }

    /// The image `bytes`, named `name` where it has one, as the model is shown it: fitted to what every model reads,
    /// kept in the thread's files by its content, after a line saying what it is.
    ///
    /// # Errors
    /// A `Refusal` where it is no image that can be read, or it could not be kept.
    pub async fn show_image(&self, bytes: &[u8], name: Option<&str>) -> Result<Vec<InputContent>, Refusal> {
        let (file, size) = self
            .volume()
            .keep_image(&self.thread, bytes, name)
            .await
            .map_err(|error| Refusal(format!("{}: {error}", name.unwrap_or("the image"))))?;
        let size = format!("{}×{}", size.width, size.height);
        let said = match name {
            Some(name) => format!("{name}, an image of {size}"),
            None => format!("an image of {size}"),
        };
        Ok(vec![
            InputContent::text(said),
            InputContent::InputImage {
                file,
                detail: ImageDetail::Auto,
            },
        ])
    }
}

/// Whether the folder `folder` is there, made where it was not, as a place work writes must be to be mounted.
fn made(folder: &PathBuf) -> bool {
    std::fs::create_dir_all(folder).is_ok()
}
