//! The skills a thread's model may use: each a folder holding a `SKILL.md`, whose front matter names it and says what it
//! is for, read whole when a task calls for it. The person's own are in the Octoii Home's `skills/`; a project's in the
//! `.agents/skills/` of each folder from its root down to the workspace, a nearer one standing for one of its name
//! farther away. The model is told each by its name, its file and what it is for.

use std::fmt::Write as _;
use std::io::Read as _;
use std::path::{Path, PathBuf};

/// A skill: its name, its file, and what it is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub path: PathBuf,
    pub description: String,
}

/// The most of a skill's file read for its front matter.
const HEAD_BYTES_MAX: u64 = 16 << 10;

/// The skills of the folders of skills `places`, in the order given, a later one standing for one of its name before.
#[must_use]
pub fn find(places: &[PathBuf]) -> Vec<Skill> {
    let mut found: Vec<Skill> = Vec::new();
    for place in places {
        let Ok(entries) = std::fs::read_dir(place) else { continue };
        let mut here: Vec<Skill> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| skill(&entry.path().join("SKILL.md"), &entry.file_name().to_string_lossy()))
            .collect();
        here.sort_by(|a, b| a.name.cmp(&b.name));
        for skill in here {
            found.retain(|kept| kept.name != skill.name);
            found.push(skill);
        }
    }
    found
}

/// The skill whose file is `path`, named `folder` where its front matter names it not; none where there is no file.
fn skill(path: &Path, folder: &str) -> Option<Skill> {
    let mut head = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(HEAD_BYTES_MAX)
        .read_to_string(&mut head)
        .ok()?;
    let mut name = folder.to_owned();
    let mut description = String::new();
    if let Some(matter) = head.strip_prefix("---\n").and_then(|rest| rest.split("\n---").next()) {
        for line in matter.lines() {
            let Some((key, value)) = line.split_once(':') else { continue };
            let value = value.trim().trim_matches(|quote| quote == '"' || quote == '\'').to_owned();
            match key.trim() {
                "name" if !value.is_empty() => name = value,
                "description" => description = value,
                _ => {}
            }
        }
    }
    Some(Skill {
        name,
        path: path.to_owned(),
        description,
    })
}

/// `skills` as the model is told of them.
#[must_use]
pub fn told(skills: &[Skill]) -> String {
    let mut out = String::from("<skills>\n");
    for skill in skills {
        let _ = writeln!(
            out,
            "<skill name={} path={}>{}</skill>",
            serde_json::json!(skill.name),
            serde_json::json!(skill.path.display().to_string()),
            skill.description
        );
    }
    out.push_str("</skills>");
    out
}
