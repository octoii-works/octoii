//! `present_file`: a file kept in the thread before its resource link is returned; HTML embeds its local images.

use octoii::machine::Base;
use octoii::threads::files;
use octoii::tools::{Context, Effect, Refusal, Told, Tool};
use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::Deserialize;

/// The arguments of `present_file`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct PresentFileArgs {
    /// Regular file to present: relative to the workspace, absolute, or starting with ~ for the home folder.
    #[schemars(length(min = 1))]
    pub path: String,
    /// Optional display title. The downloadable file keeps its original basename.
    pub title: Option<String>,
}

/// `present_file`.
#[derive(Debug, Clone, Copy, Default)]
pub struct PresentFileTool;

impl Tool for PresentFileTool {
    const NAME: &'static str = "present_file";
    const DESCRIPTION: &'static str = "Save a readable regular file of at most 32 MiB in the conversation and return its resource link. Directories are refused. The saved copy survives source changes; non-HTML bytes are unchanged, while HTML embeds local images without modifying the source.";
    type Args = PresentFileArgs;

    fn effect(&self, _: &PresentFileArgs, _: &Base) -> Effect {
        Effect::Acts {
            tool: Self::NAME.to_owned(),
        }
    }

    async fn exec(&self, context: &Context, args: PresentFileArgs) -> Result<Told, Refusal> {
        let read_context = context.clone();
        let path = args.path;
        let (name, bytes) = tokio::task::spawn_blocking(move || {
            if path.is_empty() || path.contains('\0') {
                return Err(Refusal::new("invalid file path"));
            }
            let machine = read_context.machine()?;
            let (path, mut bytes) = machine
                .read_file(&machine.path(&path), files::BYTES_MAX as u64)
                .map_err(|error| Refusal(format!("{}: {error}", machine.shown(&machine.path(&path)))))?;
            let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
            if files::media_type(&name, &bytes) == "text/html" {
                let source = std::str::from_utf8(&bytes).map_err(|_| Refusal::new("HTML needs UTF-8 text"))?;
                bytes = siphon_html::inline_local_images(&machine, &path, source)
                    .map_err(|error| Refusal(error.to_string()))?
                    .into_bytes()
                    .into();
            }
            Ok((name, bytes))
        })
        .await
        .map_err(|error| Refusal(format!("the file read failed: {error}")))??;
        let (file, size) = context
            .volume()
            .keep_file(&context.thread, bytes, &name)
            .await
            .map_err(|error| Refusal(format!("{name}: {error}")))?;
        Ok(Told {
            content: vec![
                InputContent::text(format!(
                    "Presented {} ({size} bytes).",
                    file.filename.as_deref().unwrap_or("the file")
                )),
                InputContent::ResourceLink { file, size },
            ],
            metadata: args.title.map(|title| serde_json::json!({ "title": title })),
            ..Told::default()
        })
    }
}
