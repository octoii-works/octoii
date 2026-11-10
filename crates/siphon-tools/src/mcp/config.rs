//! A configuration of MCP servers, in the common `mcpServers` shape: each server a command the machine runs — its
//! arguments, its variables set over the machine's own, its folder — or a URL reached over streamable HTTP, with the
//! headers sent to it; `${NAME}` and `${NAME:-default}` in any of these taken from the host's variables. A server that
//! cannot be taken is a problem, saying why, and the rest are taken still.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The name of a project's configuration, at its folder's root.
pub const PROJECT_FILE: &str = ".mcp.json";

/// The name of the person's own configuration, in the Octoii Home.
pub const OWN_FILE: &str = "mcp.json";

/// A server: its name, the configuration it comes from, by its path, and how it is reached. Two alike are one server,
/// which a change of any of these makes another.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct Server {
    pub name: String,
    pub source: PathBuf,
    pub launch: Launch,
}

/// How a server is reached.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Launch {
    /// A command the machine runs in `cwd`, talked to over its standard input and output, `env` set over the
    /// machine's variables.
    Command {
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: PathBuf,
    },
    /// A server reached at `url` over streamable HTTP, sent `headers`.
    Http { url: String, headers: Vec<(String, String)> },
}

impl Server {
    /// What it runs, or where it is reached, as the person is shown it.
    #[must_use]
    pub fn runs(&self) -> String {
        match &self.launch {
            Launch::Command { command, args, .. } => std::iter::once(command)
                .chain(args)
                .map(|word| quoted(word))
                .collect::<Vec<_>>()
                .join(" "),
            Launch::Http { url, .. } => url.clone(),
        }
    }
}

/// `word` as a shell reads it back as one word.
fn quoted(word: &str) -> String {
    let plain = !word.is_empty() && word.chars().all(|char| char.is_ascii_alphanumeric() || "-_./=:@%+,".contains(char));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// The servers configurations name, and what of them could not be taken, each said for the model and the person to
/// read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Configured {
    pub servers: Vec<Server>,
    pub problems: Vec<String>,
}

impl Configured {
    /// These servers and `nearer`'s, one of `nearer`'s standing for one of these of its name, as the nearer setting
    /// holds.
    pub fn extend(&mut self, nearer: Configured) {
        self.servers
            .retain(|server| !nearer.servers.iter().any(|near| near.name == server.name));
        self.servers.extend(nearer.servers);
        self.problems.extend(nearer.problems);
    }
}

#[derive(Deserialize)]
struct File {
    #[serde(rename = "mcpServers", default)]
    servers: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    kind: Option<String>,
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    cwd: Option<String>,
    url: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
}

/// The configuration `text` of the file at `path`, whose commands run in `folder` unless they name a folder of their
/// own, or one relative to it; `variables` gives the host's variables.
#[must_use]
pub fn parse(path: &Path, text: &str, folder: &Path, variables: &dyn Fn(&str) -> Option<String>) -> Configured {
    let mut configured = Configured::default();
    let file: File = match serde_json::from_str(text) {
        Ok(file) => file,
        Err(error) => {
            configured.problems.push(format!("{}: {error}", path.display()));
            return configured;
        }
    };
    for (name, value) in file.servers {
        match server(&name, value, folder, variables) {
            Ok(launch) => configured.servers.push(Server {
                name,
                source: path.to_owned(),
                launch,
            }),
            Err(why) => configured
                .problems
                .push(format!("the MCP server {name} of {}: {why}", path.display())),
        }
    }
    configured
}

/// How the server `name` that `value` describes is reached; else why it cannot be.
fn server(name: &str, value: serde_json::Value, folder: &Path, variables: &dyn Fn(&str) -> Option<String>) -> Result<Launch, String> {
    // A tool of a server is called by its name after the server's and a dot: a dot of the server's own would part
    // them elsewhere.
    if name.is_empty() || name.contains('.') {
        return Err("its name is empty or holds a dot, which parts a server's name from its tools'".into());
    }
    let entry: Entry = serde_json::from_value(value).map_err(|error| error.to_string())?;
    let expand = |text: &str| expanded(text, variables);
    let pairs = |map: BTreeMap<String, String>| {
        map.into_iter()
            .map(|(key, value)| Ok((key, expand(&value)?)))
            .collect::<Result<Vec<_>, String>>()
    };
    let http = match entry.kind.as_deref() {
        None => entry.command.is_none(),
        Some("stdio") => false,
        Some("http" | "streamable-http") => true,
        Some("sse") => return Err("a server over SSE is not taken, only one run as a command or reached over streamable HTTP".into()),
        Some(kind) => return Err(format!("its type {kind} is none of stdio and http")),
    };
    if http {
        let url = match (entry.url, entry.kind) {
            (Some(url), _) => url,
            (None, None) => return Err("it names neither a command nor a URL".into()),
            (None, Some(_)) => return Err("it names no URL".into()),
        };
        return Ok(Launch::Http {
            url: expand(&url)?,
            headers: pairs(entry.headers)?,
        });
    }
    let command = entry.command.ok_or("it names no command")?;
    Ok(Launch::Command {
        command: expand(&command)?,
        args: entry.args.iter().map(|arg| expand(arg)).collect::<Result<_, _>>()?,
        env: pairs(entry.env)?,
        cwd: match entry.cwd {
            Some(cwd) => folder.join(expand(&cwd)?),
            None => folder.to_owned(),
        },
    })
}

/// `text` with each `${NAME}` the host's variable of the name, and each `${NAME:-default}` its default where the host
/// has none; else the variable it names that the host does not have.
fn expanded(text: &str, variables: &dyn Fn(&str) -> Option<String>) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return Ok(out);
        };
        let inside = &after[..end];
        let (name, default) = match inside.split_once(":-") {
            Some((name, default)) => (name, Some(default)),
            None => (inside, None),
        };
        match (variables(name), default) {
            (Some(value), _) => out.push_str(&value),
            (None, Some(default)) => out.push_str(default),
            (None, None) => return Err(format!("${{{name}}} is not set")),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}
