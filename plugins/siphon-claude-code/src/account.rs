//! One account's Claude Code: the person's own login, or an account they added with its configuration — and its
//! login — kept apart in a folder of its own. It keeps where its conversations stand in its sessions, which session
//! each conversation holds, its live sessions standing idle, and how one is started: `claude` with its own tools,
//! settings, skills, memory and hooks off, the thread's tools an MCP server of the session's, Claude Code's preset
//! system prompt with the thread's appended and its dynamic sections left out, each message given as the thread wrote
//! it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use octoii_ai::vocabulary::{Failure, FailureKind, ReasoningEffort};
use serde_json::{Value, json};
use siphon_machine::paths::Base;
use siphon_machine::{Machine, Writes};
use tokio::sync::watch;

use crate::process::{Launch, PREFIX, Process, SERVER, Started};
use crate::session::Session;
use crate::store::{Holds, Position, Positions, Windows};

/// The variables that would make Claude Code use another account than the login it keeps, or act on its own.
const CLEARED: [&str; 9] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_SIMPLE",
    "CLAUDE_CODE_EFFORT_LEVEL",
    "MCP_TOOL_TIMEOUT",
    "CLAUDECODE",
    "CLAUDE_CONFIG_DIR",
];

/// What Claude Code does beside the thread, turned off: the thread keeps its own of each.
const ISOLATION: [(&str, &str); 10] = [
    ("CLAUDE_CODE_DISABLE_CLAUDE_MDS", "1"),
    ("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1"),
    ("DISABLE_AUTO_COMPACT", "1"),
    ("CLAUDE_CODE_ENABLE_AWAY_SUMMARY", "0"),
    ("ENABLE_TOOL_SEARCH", "false"),
    ("CLAUDE_CODE_DISABLE_ADVISOR_TOOL", "1"),
    ("CLAUDE_CODE_DISABLE_NONSTREAMING_FALLBACK", "1"),
    ("CLAUDE_CODE_DISABLE_REFUSAL_FALLBACK", "1"),
    // The thread bounds what its tools give; Claude Code's own bound would cut it again.
    ("MAX_MCP_OUTPUT_TOKENS", "200000"),
    // As the SDK's sessions are: kept from the person's own picker of sessions to resume.
    ("CLAUDE_CODE_ENTRYPOINT", "sdk-ts"),
];

/// How long a live session may stand unasked before it is closed.
pub const IDLE: Duration = Duration::from_mins(10);
/// How long a call may be held before Claude Code gives up on it, in milliseconds.
const HOLD_MS: u64 = 20 * 60 * 1000;
/// What Claude is told of a tool's result past which it is cut; the thread bounds its own already.
const RESULT_CHARS_MAX: u64 = 500_000;
/// How long a session no conversation holds may stand unused before it is removed.
const COLLECT_AFTER: Duration = Duration::from_hours(24);
/// The longest a folder's name may be in Claude Code's projects before it is cut and hashed.
const PROJECT_NAME_MAX: usize = 200;

/// Where Claude Code runs: the kernel and the machine, the person's `claude` — none, where it is not installed — and
/// the plugin's own folder, among what the host keeps for itself: the folder Claude Code works in, which holds nothing
/// of the person's, and each account's files.
#[derive(Debug, Clone)]
pub struct Place {
    pub kernel: beak::Kernel,
    pub machine: siphon_machine::Host,
    pub program: Option<PathBuf>,
    pub folder: PathBuf,
}

impl Place {
    /// The folder Claude Code works in.
    #[must_use]
    pub fn work(&self) -> PathBuf {
        self.folder.join("work")
    }
}

/// How a session begins: anew, or resumed at an entry of a session — in place at its tip, or forked behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Begin {
    New,
    Resume { at: Position, fork: bool },
}

/// What a session is asked with: the model, by Claude Code's name for it; the system prompt appended to its own; the
/// thread's tools, as MCP lists them; how hard it thinks — none, its own default; the schema its answer is JSON of;
/// whether its session is kept; and its settings, which a live one must share to go on.
#[derive(Debug, Clone)]
pub struct Asked {
    pub model: String,
    pub system: String,
    pub tools: Vec<Value>,
    pub effort: Option<ReasoningEffort>,
    pub schema: Option<Value>,
    pub kept: bool,
    pub settings: String,
}

/// What an account keeps, under one lock: its positions, its holds, its live sessions standing idle and those
/// settling after an answer, and the windows of its models.
#[derive(Debug)]
pub struct Kept {
    pub positions: Positions,
    pub holds: Holds,
    pub live: Vec<Session>,
    pub settling: Vec<Settling>,
    pub windows: Windows,
}

/// A session settling after its answer: where it stands, what it was asked with, and whether it settled, which a
/// request that would go on from there waits for.
#[derive(Debug)]
pub struct Settling {
    pub position: String,
    pub settings: String,
    pub settled: watch::Receiver<bool>,
}

/// An account's Claude Code.
#[derive(Debug)]
pub struct Account {
    pub place: Arc<Place>,
    /// Its own configuration's folder, as `CLAUDE_CONFIG_DIR` names it; none for the person's own.
    pub config: Option<PathBuf>,
    pub folder: PathBuf,
    pub kept: Mutex<Kept>,
}

impl Account {
    /// The account whose files are in `folder`, its configuration in `config`.
    ///
    /// # Errors
    /// Where its folder cannot be made or its files read.
    pub fn open(place: Arc<Place>, folder: PathBuf, config: Option<PathBuf>) -> std::io::Result<Self> {
        std::fs::create_dir_all(&folder)?;
        let kept = Kept {
            positions: Positions::open(folder.join("positions.jsonl"))?,
            holds: Holds::open(folder.join("holds.jsonl"))?,
            live: Vec::new(),
            settling: Vec::new(),
            windows: Windows::read(&folder.join("windows.json")).unwrap_or_default(),
        };
        Ok(Self {
            place,
            config,
            folder,
            kept: Mutex::new(kept),
        })
    }

    /// What the account keeps, locked.
    ///
    /// # Panics
    /// Where a holder of the lock panicked.
    pub fn kept(&self) -> std::sync::MutexGuard<'_, Kept> {
        self.kept.lock().expect("no holder of the account panics")
    }

    /// The command a person signs this account's Claude Code in with.
    #[must_use]
    pub fn sign_in(&self) -> String {
        match &self.config {
            Some(config) => format!("CLAUDE_CONFIG_DIR='{}' claude auth login", config.display()),
            None => "claude auth login".to_owned(),
        }
    }

    /// Starts a session asked `asked`, beginning as `begin` says, at `time`.
    ///
    /// # Errors
    /// Where `claude` is not installed, or failed before it answered.
    pub async fn start(&self, time: i64, asked: &Asked, begin: &Begin) -> Result<Session, Failure> {
        let names = asked
            .tools
            .iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
            .collect::<HashSet<_>>();
        let initialize = initialize(asked, &names);
        let (process, _, heard) = self
            .launched(time, arguments(asked, begin), asked.tools.clone(), initialize)
            .await?;
        Ok(Session::new(process, heard, asked.settings.clone(), names))
    }

    /// A `claude` started for what is asked of it alone — no tools, no session kept — and what it answered
    /// `initialize`.
    ///
    /// # Errors
    /// As `start`.
    pub async fn asked(&self, time: i64) -> Result<(Process, Value), Failure> {
        let arguments = [
            "--output-format=stream-json",
            "--verbose",
            "--input-format=stream-json",
            "--tools=",
            "--setting-sources=",
            "--strict-mcp-config",
            "--no-session-persistence",
        ]
        .map(str::to_owned)
        .to_vec();
        let initialize = json!({ "subtype": "initialize", "skills": [] });
        let (process, answer, _) = self.launched(time, arguments, Vec::new(), initialize).await?;
        Ok((process, answer))
    }

    /// `claude` started with `arguments` at `time` in the work folder, its MCP server offering `tools`, initialized
    /// with `initialize`: with its answer, and what it is heard to write.
    async fn launched(
        &self,
        time: i64,
        arguments: Vec<String>,
        tools: Vec<Value>,
        initialize: Value,
    ) -> Result<(Process, Value, tokio::sync::mpsc::UnboundedReceiver<crate::process::Heard>), Failure> {
        let Some(program) = &self.place.program else {
            return Err(self.not_installed());
        };
        let base = Base {
            workspace: self.place.work(),
            home: self.place.machine.home.clone(),
        };
        let opened = Machine::open(&self.place.kernel, &self.place.machine, base, &Writes::Anywhere, None)
            .and_then(|machine| machine.bind(&self.place.folder).map(|()| machine));
        let machine = opened.map_err(|error| Failure::new(FailureKind::Server, error.to_string()))?;
        let launch = Launch {
            program: program.to_string_lossy().into_owned(),
            arguments,
            cwd: self.place.work(),
            variables: self.variables(),
        };
        Process::start(&machine, time, launch, tools, initialize)
            .await
            .map_err(|started| match started {
                Started::Missing => self.not_installed(),
                Started::Failed(why) => Failure::new(FailureKind::Server, format!("Claude Code did not start: {why}")),
            })
    }

    fn not_installed(&self) -> Failure {
        Failure::new(
            FailureKind::Auth,
            format!(
                "Claude Code is not installed on this computer: install it with `curl -fsSL https://claude.ai/install.sh | bash`, then sign in with `{}`",
                self.sign_in()
            ),
        )
    }

    /// The variables Claude Code runs with: the machine's, but those that would change whose account it uses or what
    /// it does on its own; this account's configuration where it has one of its own.
    fn variables(&self) -> Vec<(String, String)> {
        let mut variables: Vec<(String, String)> = std::env::vars().filter(|(name, _)| !CLEARED.contains(&name.as_str())).collect();
        variables.retain(|(name, _)| !ISOLATION.iter().any(|(isolated, _)| isolated == name));
        variables.extend(ISOLATION.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())));
        let config = self
            .config
            .clone()
            .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from));
        if let Some(config) = config {
            variables.push(("CLAUDE_CONFIG_DIR".to_owned(), config.to_string_lossy().into_owned()));
        }
        variables
    }

    /// The folder Claude Code keeps the sessions of its work folder in; none where its name would be cut.
    fn sessions(&self) -> Option<PathBuf> {
        let root = self
            .config
            .clone()
            .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from))
            .or_else(|| self.place.machine.home.as_ref().map(|home| home.join(".claude")))?;
        let work = self.place.work();
        let work = work.to_string_lossy();
        let name: String = work
            .chars()
            .map(|each| if each.is_ascii_alphanumeric() { each } else { '-' })
            .collect();
        (name.len() <= PROJECT_NAME_MAX).then(|| root.join("projects").join(name))
    }

    /// Whether Claude Code still has the session `id`.
    #[must_use]
    pub fn has(&self, id: &str) -> bool {
        self.sessions().is_some_and(|folder| folder.join(format!("{id}.jsonl")).is_file())
    }

    /// Lets go of the session `id`: its live sessions closed, its positions forgotten, and it removed.
    pub fn let_go(&self, id: &str) {
        {
            let mut kept = self.kept();
            kept.live.retain(|session| session.id.as_deref() != Some(id));
            if let Err(error) = kept.positions.forget(id) {
                tracing::warn!(%error, "the positions of Claude's sessions could not be kept");
            }
        }
        if let Some(folder) = self.sessions() {
            let _ = std::fs::remove_file(folder.join(format!("{id}.jsonl")));
            let _ = std::fs::remove_dir_all(folder.join(id));
        }
    }

    /// Lets go of what `conversation` holds, as it ended: its session, where no other conversation holds it.
    pub fn forget(&self, conversation: &str) {
        let released = self.kept().holds.release(conversation);
        match released {
            Ok(Some(session)) => self.let_go(&session),
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "the holds of Claude's sessions could not be kept"),
        }
    }

    /// Closes the live sessions that stood unasked past `IDLE`, and removes the sessions no conversation holds that
    /// none used past `COLLECT_AFTER`.
    pub fn sweep(&self) {
        self.kept()
            .live
            .retain(|session| session.is_open() && session.used.elapsed() < IDLE);
        let Some(folder) = self.sessions() else { return };
        let Ok(entries) = std::fs::read_dir(folder) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()).map(str::to_owned) else {
                continue;
            };
            let old = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age > COLLECT_AFTER));
            let in_use = {
                let kept = self.kept();
                kept.holds.is_held(&id) || kept.live.iter().any(|session| session.id.as_deref() == Some(id.as_str()))
            };
            if path.extension().is_some_and(|extension| extension == "jsonl") && old && !in_use {
                self.let_go(&id);
            }
        }
    }

    /// Takes the live session that stands at `prefix`, asked with `settings` — one settling there once it settled —
    /// none where none does.
    pub async fn take_live(&self, prefix: &str, settings: &str) -> Option<Session> {
        loop {
            let mut settled = {
                let mut kept = self.kept();
                let standing = |position: Option<&str>, asked: &str| position == Some(prefix) && asked == settings;
                if let Some(at) = kept
                    .live
                    .iter()
                    .position(|session| session.is_open() && standing(session.position.as_deref(), &session.settings))
                {
                    return Some(kept.live.swap_remove(at));
                }
                let settling = kept.settling.iter().find(|each| standing(Some(&each.position), &each.settings))?;
                settling.settled.clone()
            };
            let _ = settled.wait_for(|settled| *settled).await;
        }
    }

    /// Keeps the windows Claude Code measured its models against, by the names it gave them, where they differ.
    pub fn measured(&self, measured: &HashMap<String, u64>, rows: &[(String, String)]) {
        if measured.is_empty() {
            return;
        }
        let mut kept = self.kept();
        let mut changed = false;
        for (name, window) in measured {
            for (value, resolved) in rows {
                if (value == name || resolved == name) && kept.windows.windows.get(value) != Some(window) {
                    kept.windows.windows.insert(value.clone(), *window);
                    changed = true;
                }
            }
        }
        if changed && let Err(error) = kept.windows.write(&self.folder.join("windows.json")) {
            tracing::warn!(%error, "the windows of Claude's models could not be kept");
        }
    }
}

/// The arguments `claude` is started with for what is `asked`, beginning as `begin` says.
fn arguments(asked: &Asked, begin: &Begin) -> Vec<String> {
    let mut out: Vec<String> = [
        "--output-format=stream-json",
        "--verbose",
        "--input-format=stream-json",
        "--include-partial-messages",
        "--tools=",
        "--setting-sources=",
        "--strict-mcp-config",
        "--permission-mode=dontAsk",
        "--settings={\"disableAllHooks\":true}",
    ]
    .map(str::to_owned)
    .to_vec();
    out.push(format!("--model={}", asked.model));
    match asked.effort {
        Some(ReasoningEffort::None) => out.push("--thinking=disabled".to_owned()),
        effort => {
            out.extend(["--thinking=adaptive".to_owned(), "--thinking-display=summarized".to_owned()]);
            let named = match effort {
                Some(ReasoningEffort::Minimal | ReasoningEffort::Low) => Some("low"),
                Some(ReasoningEffort::Medium) => Some("medium"),
                Some(ReasoningEffort::High) => Some("high"),
                Some(ReasoningEffort::Xhigh) => Some("xhigh"),
                Some(ReasoningEffort::Max) => Some("max"),
                Some(ReasoningEffort::None) | None => None,
            };
            out.extend(named.map(|named| format!("--effort={named}")));
        }
    }
    if !asked.tools.is_empty() {
        out.push(format!("--allowedTools={PREFIX}*"));
    }
    if let Some(schema) = &asked.schema {
        out.push(format!("--json-schema={schema}"));
    }
    if let Begin::Resume { at, fork } = begin {
        out.push(format!("--resume={}", at.session));
        out.push(format!("--resume-session-at={}", at.entry));
        if *fork {
            out.push("--fork-session".to_owned());
        }
    }
    if !asked.kept {
        out.push("--no-session-persistence".to_owned());
    }
    out
}

/// What a session is initialized with: the thread's tools on our MCP server, each called by its own name too; Claude
/// Code's preset system prompt, the thread's appended, its dynamic sections left out and taken anew as a session is
/// resumed; no skills; and the schema its answer is JSON of.
fn initialize(asked: &Asked, names: &HashSet<String>) -> Value {
    let mut initialize = json!({
        "subtype": "initialize",
        "appendSystemPrompt": asked.system,
        "excludeDynamicSections": true,
        "systemPromptSnapshot": false,
        "skills": [],
    });
    if !asked.tools.is_empty() {
        let aliases: serde_json::Map<String, Value> = names.iter().map(|name| (name.clone(), json!(format!("{PREFIX}{name}")))).collect();
        initialize["sdkMcpServers"] = json!([SERVER]);
        initialize["sdkMcpServerConfigs"] = json!({ SERVER: { "timeout": HOLD_MS } });
        initialize["toolAliases"] = Value::Object(aliases);
    }
    if let Some(schema) = &asked.schema {
        initialize["jsonSchema"] = schema.clone();
    }
    initialize
}

/// A tool of the thread's as MCP lists it: as it was declared, never read-only, so Claude Code makes one call at a
/// time; always loaded, its result cut where Claude Code's own bound would not.
#[must_use]
pub fn listed_tool(name: &str, description: &str, parameters: &Value) -> Value {
    let schema = if parameters["type"] == "object" {
        parameters.clone()
    } else {
        json!({ "type": "object" })
    };
    json!({
        "name": name,
        "description": description,
        "inputSchema": schema,
        "annotations": { "readOnlyHint": false },
        "_meta": { "anthropic/alwaysLoad": true, "anthropic/maxResultSizeChars": RESULT_CHARS_MAX },
    })
}

/// The person's own `claude`: the first on the machine's path, else where its installer puts it.
#[must_use]
pub fn found(home: Option<&Path>) -> Option<PathBuf> {
    let on_path = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).map(|folder| folder.join("claude")).collect::<Vec<_>>())
        .unwrap_or_default();
    let installed = home.map(|home| home.join(".local/bin/claude"));
    on_path.into_iter().chain(installed).find(|candidate| candidate.is_file())
}
