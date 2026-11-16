//! `present_file`: a file's original bytes kept in the thread before its resource link is returned.

use octoii_ai::vocabulary::InputContent;
use schemars::JsonSchema;
use serde::Deserialize;
use siphon::machine::Base;
use siphon::tools::{Context, Effect, Refusal, Told, Tool};

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
    const DESCRIPTION: &'static str = "Present one file to the person for download, keeping its original bytes in this conversation. Use after generating a report, image, archive, or other result. Accepts a regular file up to 32 MiB that this call may read; directories are refused. The saved file survives changes to the source. This does not read the file into model context or publish it on the internet.";
    type Args = PresentFileArgs;

    fn effect(&self, _: &PresentFileArgs, _: &Base) -> Effect {
        Effect::Acts {
            tool: Self::NAME.to_owned(),
        }
    }

    async fn exec(&self, context: &Context, args: PresentFileArgs) -> Result<Told, Refusal> {
        let (file, size) = context.keep_file(&args.path).await?;
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
