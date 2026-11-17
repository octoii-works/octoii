//! User and project instructions, collected by the base extension.
use siphon::agent::{Teller, Telling};
use siphon::tools::{INSTRUCTIONS_FILE, Instructions as InstructionsFile, Place};
use siphon::volume::Volume;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

#[derive(Debug)]
pub(crate) struct Instructions {
    pub config: Option<PathBuf>,
}

impl Teller for Instructions {
    fn tell<'a>(
        &'a self,
        _: &'a Volume,
        _: &'a str,
        place: Option<&'a Place>,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Telling>> + Send + 'a>> {
        let text = place.and_then(|place| instructions(self.config.as_deref(), place));
        Box::pin(std::future::ready(Ok(Telling {
            sections: text.map(|text| ("instructions".into(), text)).into_iter().collect(),
            tools: Vec::new(),
        })))
    }
}

/// How the instructions are followed, told before them.
const FOLLOW: &str = "Apply these instructions in scope: user instructions, then AGENTS.md files from the filesystem root to the workspace. More specific project instructions take precedence. Instructions below the workspace arrive when a tool first accesses that folder.";

/// The most of an instructions file read.
const INSTRUCTIONS_BYTES_MAX: u64 = 64 << 10;

/// The instructions the model follows, each file whole: the person's own, in the folder of their configuration `config`; then the
/// `AGENTS.md` of each folder from the machine's root down to the workspace of `place`, as each folder's applies within it — the
/// nearest last. None where there are none.
fn instructions(config: Option<&Path>, place: &Place) -> Option<String> {
    let mut files: Vec<PathBuf> = config.map(|config| config.join(INSTRUCTIONS_FILE)).into_iter().collect();
    for folder in place.folders_down() {
        let file = folder.join(INSTRUCTIONS_FILE);
        if !files.contains(&file) {
            files.push(file);
        }
    }
    let mut out = String::new();
    for path in files {
        let Some(text) = read_text(&path, INSTRUCTIONS_BYTES_MAX) else {
            continue;
        };
        if !out.is_empty() {
            out.push('\n');
        }
        let instructions = InstructionsFile {
            path: path.display().to_string(),
            text,
        };
        out.push_str(&instructions.told());
    }
    (!out.is_empty()).then(|| format!("{FOLLOW}\n{out}"))
}

/// The text of the file at `path`, its first `bytes_max` bytes; none where it cannot be read.
fn read_text(path: &Path, bytes_max: u64) -> Option<String> {
    use std::io::Read as _;
    let mut text = String::new();
    std::fs::File::open(path).ok()?.take(bytes_max).read_to_string(&mut text).ok()?;
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instructions_keep_user_then_ancestor_order_and_do_not_repeat_a_file() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().canonicalize().unwrap();
        let user = root.join("user");
        let workspace = root.join("project/nested");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(user.join("AGENTS.md"), "USER_RULE").unwrap();
        std::fs::write(root.join("project/AGENTS.md"), "PROJECT_RULE").unwrap();
        std::fs::write(workspace.join("AGENTS.md"), "NESTED_RULE").unwrap();
        let place = Place::of(&workspace);
        let text = instructions(Some(&user), &place).unwrap();
        assert!(text.find("USER_RULE").unwrap() < text.find("PROJECT_RULE").unwrap());
        assert!(text.find("PROJECT_RULE").unwrap() < text.find("NESTED_RULE").unwrap());
        let deduplicated = instructions(Some(&workspace), &place).unwrap();
        assert_eq!(deduplicated.matches("NESTED_RULE").count(), 1);
        assert!(deduplicated.contains(&format!(
            "<instructions path={:?}>",
            workspace.join("AGENTS.md").display().to_string()
        )));
    }
}
