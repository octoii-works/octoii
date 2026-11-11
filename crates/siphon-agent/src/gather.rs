//! What a thread's model is told, gathered before each of its turns by a `siphon.gather` request, in a task of its own,
//! as its sources are read from the machine: the base every turn starts from; the instructions it follows, which always
//! apply, given whole; the skills it may use; the catalog of its MCP servers' tools; the environment it works in; where
//! the threads are; and the tools it may call — its own, and the web search its provider runs where its model's facts
//! say it does.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use octoii_ai::models::Models;
use octoii_ai::vocabulary::Tool;
use serde::{Deserialize, Serialize};
use siphon_machine::Host;
use siphon_runtime::{Asked, Request, Runtime, Unanswered};
use siphon_threads::{Config, Mode, thread};
use siphon_tools::projects::Place;
use siphon_tools::{Tool as _, Tools};

use crate::delegate::DelegateTool;
use crate::skills;
use crate::told::Told;

/// What a gather request asks: what the model of the thread `thread`, by its id, is told now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gather {
    pub thread: String,
}

impl Request for Gather {
    const NAME: &'static str = "siphon.gather";
    type Answer = Told;
}

/// What a plugin tells a thread's model of the place it works in, gathered before each turn: sections of what it is
/// told, and the names of the tools its scripts may call beside its own — as the MCP servers tell their catalog.
pub trait Teller: Send + Sync + 'static {
    /// What it tells the model of the thread `thread`, working at `place`, on `runtime`.
    ///
    /// # Errors
    /// What keeps it from telling.
    fn tell<'a>(
        &'a self,
        runtime: &'a Runtime,
        thread: &'a str,
        place: &'a Place,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Telling>> + Send + 'a>>;
}

/// What a teller tells: its sections, by name, and the tools a script may call by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Telling {
    pub sections: Vec<(String, String)>,
    pub tools: Vec<String>,
}

/// Where what the model is told comes from: the tools it may call, what the plugins tell, the models' facts, the
/// machine, and the Octoii Home, where the person's own instructions and skills are.
#[derive(Clone)]
pub struct Sources {
    pub tools: Tools,
    pub tellers: Vec<Arc<dyn Teller>>,
    pub models: Arc<Models>,
    pub host: Host,
    pub home: Option<PathBuf>,
}

impl std::fmt::Debug for Sources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sources").field("tools", &self.tools).finish_non_exhaustive()
    }
}

/// The name of a folder's instructions file.
pub const INSTRUCTIONS_FILE: &str = "AGENTS.md";
/// The most of an instructions file read.
const INSTRUCTIONS_BYTES_MAX: u64 = 64 << 10;

/// What every model is told first, whatever it is.
pub const BASE: &str = "You are a coding agent in Octoii, working on the person's machine through your tools. Help the person do what they ask.

Your tools' results:
- A tool gives what it gave as it is. A line in brackets after it is a note on it: what it leaves out and how to go on, how a command ended, how an edit matched.
- A failure is given as the system gives it, in its own words.
- A relative path starts at the workspace, as a shell's does, and a path a tool gives is one you can give back as it is.

Working:
- Follow the instructions in <instructions>: the person's own, for every project, then each folder's AGENTS.md from the root down to the workspace. Where they differ, the later one holds.
- Each <skill> is a file of instructions for one kind of task, told by its name and what it is for: before a task of that kind, read its file and follow it.
- Prefer the file tools to the shell for reading, writing and searching files.
- Make the calls that do not wait on one another in one reply: those that only read run at once; one that may change something runs after the calls before it and before those after, in the order you made them. For work of many steps at once, write a script for `execute`.
- Keep a change in the structure, naming and style of the code around it.
- Take changes you did not make for the person's work: look before you overwrite or delete them, and ask before what cannot be undone.
- Say plainly what you found and what you did, naming files by their paths.
- To show the person an image file, write it in your reply as Markdown, `![what it is](path)`, the path as your tools take it: they see it, and you do not. To point them at a page of the thread's browser, write `::page{id=\"its id\"}` on a line of its own.";

/// Answers the gather request `asked` from `sources`.
///
/// # Errors
/// `Failed` where the thread cannot be read.
pub async fn answer_gather(sources: &Sources, asked: Asked<Gather>) -> Result<Told, Unanswered> {
    gathered(sources, &asked.runtime, &asked.payload.thread)
        .await
        .map_err(|error| Unanswered::Failed(error.to_string()))
}

/// What the model of the thread `id` is told now, as `sources` gather it on `runtime`.
///
/// # Errors
/// The volume's failure; a thread's settings not as Siphon writes them.
pub async fn gathered(sources: &Sources, runtime: &Runtime, id: &str) -> anyhow::Result<Told> {
    let snapshot = runtime.snapshot()?;
    let directory = snapshot
        .stat(&thread::path(id))?
        .and_then(|entry| entry.directory)
        .unwrap_or_default();
    let config = Config::of(&directory)?;
    let workspace = config.workspace.as_deref().map(PathBuf::from);
    let mut told = Told::default();
    let mut catalog = Vec::new();
    let mut add = |name: &str, text: String| told.sections.push((name.to_owned(), text));
    add("base", BASE.to_owned());
    if let Some(workspace) = &workspace {
        let place = Place::of(workspace);
        if let Some(text) = instructions(sources.home.as_deref(), workspace) {
            add("instructions", text);
        }
        let found = skills::find(&skill_places(sources.home.as_deref(), &place));
        if !found.is_empty() {
            add("skills", skills::told(&found));
        }
        for teller in &sources.tellers {
            let telling = teller.tell(runtime, id, &place).await?;
            for (name, text) in telling.sections {
                add(&name, text);
            }
            catalog.extend(telling.tools);
        }
    }
    told.catalog = catalog;
    let mode = config.mode.unwrap_or_default();
    add(
        "environment",
        environment(workspace.as_deref(), mode, beak::machine_confines(), runtime.now_ms()),
    );
    if let Some(folder) = &sources.host.threads {
        add("threads", threads_told(folder, id));
    }
    told.tools = sources
        .tools
        .declared()
        .into_iter()
        .map(|declared| Tool::Function {
            name: declared.name,
            description: declared.description,
            parameters: declared.parameters,
        })
        .collect();
    // A sub-thread delegates no further.
    if directory
        .label(thread::PARENT_LABEL.as_bytes())
        .is_some_and(|parent| !parent.is_empty())
    {
        told.tools
            .retain(|tool| !matches!(tool, Tool::Function { name, .. } if name == DelegateTool::NAME));
    }
    let searches = config
        .model
        .as_deref()
        .and_then(|model| sources.models.model(model))
        .is_some_and(|model| model.searches_web);
    if searches {
        told.tools.push(Tool::WebSearch);
    }
    Ok(told)
}

/// The folders from the machine's root down to `workspace`, the workspace last.
fn folders_down(workspace: &Path) -> Vec<&Path> {
    let mut folders: Vec<&Path> = workspace.ancestors().collect();
    folders.reverse();
    folders
}

/// The instructions the model follows, each file whole: the person's own, in the Octoii Home `home`; then the
/// `AGENTS.md` of each folder from the machine's root down to `workspace`, as each folder's applies within it — the
/// nearest last. None where there are none.
fn instructions(home: Option<&Path>, workspace: &Path) -> Option<String> {
    let mut files: Vec<PathBuf> = home.map(|home| home.join(INSTRUCTIONS_FILE)).into_iter().collect();
    for folder in folders_down(workspace) {
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
        let _ = write!(
            out,
            "<instructions path={}>\n{}\n</instructions>",
            serde_json::json!(path.display().to_string()),
            text.trim_end()
        );
    }
    (!out.is_empty()).then_some(out)
}

/// The text of the file at `path`, its first `bytes_max` bytes; none where it cannot be read.
fn read_text(path: &Path, bytes_max: u64) -> Option<String> {
    use std::io::Read as _;
    let mut text = String::new();
    std::fs::File::open(path).ok()?.take(bytes_max).read_to_string(&mut text).ok()?;
    Some(text)
}

/// The folders of skills a thread working at `place` finds, the farthest first: the person's own, then each project
/// folder's from its root down to the workspace.
fn skill_places(home: Option<&Path>, place: &Place) -> Vec<PathBuf> {
    let mut places: Vec<PathBuf> = home.map(|home| home.join("skills")).into_iter().collect();
    places.extend(
        folders_down(&place.workspace)
            .into_iter()
            .filter(|folder| folder.starts_with(&place.project))
            .map(|folder| folder.join(".agents/skills")),
    );
    places
}

/// The environment a thread works in, as the model is told it: its workspace, the platform and shell its commands run
/// on, what its mode lets its calls do on a machine that confines commands or not, and the day, in UTC.
fn environment(workspace: Option<&Path>, mode: Mode, confines: bool, now_ms: i64) -> String {
    let mut out = String::from("<environment>\n");
    if let Some(workspace) = workspace {
        let _ = writeln!(out, "workspace: {}", workspace.display());
    }
    let _ = writeln!(out, "platform: {} {}", std::env::consts::OS, std::env::consts::ARCH);
    let _ = writeln!(out, "shell: {}", if cfg!(windows) { "powershell" } else { "bash" });
    let _ = writeln!(out, "permissions: {}", permissions(mode, confines));
    let (year, month, day) = civil(now_ms.div_euclid(86_400_000));
    let _ = write!(out, "date: {year}-{month:02}-{day:02} (UTC)\n</environment>");
    out
}

/// What a thread's mode lets its calls do, as the model is told it, on a machine that confines commands or not.
fn permissions(mode: Mode, confines: bool) -> &'static str {
    match (mode, confines) {
        (Mode::Default, true) => {
            "default — edits within the workspace go ahead, and commands run in a sandbox that writes the workspace and the temporary folders alone. An edit beyond the workspace, a command that must write beyond the sandbox — run it with escalate and a justification — and a tool of a server wait for the person to let them, but what they let the project do always. A call they reject comes back with their words."
        }
        (Mode::Default, false) => {
            "default — edits within the workspace go ahead. This machine has no sandbox for commands: each command, an edit beyond the workspace and a tool of a server wait for the person to let them, but what they let the project do always. A call they reject comes back with their words."
        }
        (Mode::Auto, _) => {
            "auto — edits within the workspace and commands that only read go ahead; every other call is judged first, by a second model or by the person, but what the person let the project do always. A call blocked comes back with why: find a safer way within what the person asked, or ask them."
        }
        (Mode::FullAccess, _) => "full-access — every call goes ahead as you make it.",
    }
}

/// The year, month and day of the day `days` after 1970-01-01, as the proleptic Gregorian calendar has it.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Where the threads are, as the model is told it, where the host lays them out: each a folder by its id, read alone
/// through the file tools; and this thread's id.
fn threads_told(folder: &Path, id: &str) -> String {
    format!(
        "<threads>
The threads of this machine are in {}, each a folder by its id — this thread's is {id} — to read with the file tools; nothing writes them.
- log/: its conversation, as JSON Lines in files that sort in the order they began, an entry a line: its id, which rises along the log; its type — input, what the person sent; reply, the model's response; result, a tool's output; turn-end; config, a change of how it runs; system, what the model was told; discarded, an answer that came to nothing — and its fields.
- outputs/: a call's whole output, where it was too long to be given whole.
- files/: the images and files of its conversation, by their SHA-256.
Read them where the person asks about another thread, or about work done before.
</threads>",
        folder.display()
    )
}

#[cfg(test)]
mod tests {
    use super::civil;

    #[test]
    fn a_day_is_its_date_in_the_calendar() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_768), (2026, 11, 11));
        assert_eq!(civil(11_016), (2000, 2, 29));
        assert_eq!(civil(-1), (1969, 12, 31));
    }
}
