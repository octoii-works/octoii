//! What a call does to the machine, as its tool tells it from its arguments: it reads alone; it edits files; it runs a
//! command line; or it acts through a tool that may do anything. A command line is read as the shell's language reads
//! it — each simple command its program and its words — so a grant covers a command by the words that name what it
//! does, `git push *` of `git push origin main`; a line no reading knows whole is covered by no grant.

use std::path::PathBuf;

use siphon_shell::{Command, Unknowable};

/// What a call does to the machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// It reads, and changes nothing.
    Reads,
    /// It edits the files at these absolute paths.
    Edits(Vec<PathBuf>),
    /// It runs a command line, asking to write beyond the sandbox where `outside_sandbox`, and why.
    Runs {
        line: String,
        outside_sandbox: bool,
        justification: Option<String>,
    },
    /// It acts through the tool `tool`, which may do anything.
    Acts { tool: String },
    /// It works the thread's browser: a page's state changes, the machine's not.
    Browses,
    /// It runs a program whose own calls are each weighed as they are made: a script.
    Calls,
}

/// A command line as a grant and a weighing read it: each simple command, the pattern a grant of it takes, and
/// whether the whole line reads alone; none where no reading knows it whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub commands: Vec<String>,
    pub patterns: Vec<String>,
    pub reads_alone: bool,
}

impl Line {
    /// The command line `line` as it is read.
    ///
    /// # Errors
    /// Where no reading knows it whole: a syntax error, a substitution, a function defined, a program an expansion
    /// names.
    pub fn read(line: &str) -> Result<Self, Unknowable> {
        let commands = siphon_shell::parse(line)?;
        let mut read = Self {
            commands: Vec::new(),
            patterns: Vec::new(),
            reads_alone: true,
        };
        for command in &commands {
            let words: Vec<&str> = command
                .program
                .iter()
                .map(String::as_str)
                .chain(command.arguments.iter().map(siphon_shell::Word::text))
                .collect();
            if words.is_empty() {
                continue;
            }
            read.reads_alone &= command.assignments.is_empty() && !writes_a_file(command) && reads_alone(&words);
            read.commands.push(words.join(" "));
            read.patterns.push(format!("{} *", prefix_of(&words).join(" ")));
        }
        Ok(read)
    }
}

/// Programs known to read alone.
const READERS: [&str; 40] = [
    "ls",
    "cat",
    "head",
    "tail",
    "wc",
    "pwd",
    "echo",
    "printf",
    "grep",
    "egrep",
    "fgrep",
    "which",
    "whoami",
    "id",
    "file",
    "stat",
    "du",
    "df",
    "basename",
    "dirname",
    "realpath",
    "readlink",
    "uname",
    "true",
    "false",
    "test",
    "[",
    "diff",
    "cmp",
    "comm",
    "nl",
    "cut",
    "tr",
    "column",
    "jq",
    "md5",
    "md5sum",
    "shasum",
    "sha1sum",
    "sha256sum",
];
/// What git does that reads alone, and the words that make it write.
const GIT_READERS: [&str; 9] = [
    "status",
    "diff",
    "log",
    "show",
    "rev-parse",
    "ls-files",
    "blame",
    "describe",
    "grep",
];
const GIT_WRITERS: [&str; 3] = ["--output", "-O", "--open-files-in-pager"];
/// The words that make `find` act.
const FIND_ACTORS: [&str; 9] = [
    "-exec", "-execdir", "-ok", "-okdir", "-delete", "-fprint", "-fprint0", "-fprintf", "-fls",
];

fn reads_alone(words: &[&str]) -> bool {
    let (program, rest) = (words[0], &words[1..]);
    match program {
        "rg" => !rest.iter().any(|word| word.starts_with("--pre")),
        "find" => !rest.iter().any(|word| FIND_ACTORS.contains(word)),
        "git" => {
            rest.first().is_some_and(|verb| GIT_READERS.contains(verb))
                && !rest.iter().any(|word| GIT_WRITERS.iter().any(|writer| word.starts_with(writer)))
        }
        _ => READERS.contains(&program),
    }
}

/// Whether a redirection of `command` writes a file — not a descriptor, nor /dev/null.
fn writes_a_file(command: &Command) -> bool {
    command.redirections.iter().any(|redirection| {
        let sends = [">", ">>", ">|", "&>", "&>>", "<>", ">&"].contains(&redirection.operator.as_str());
        let target = redirection.target.text();
        let descriptor = target == "-" || target.chars().all(|char| char.is_ascii_digit());
        sends && !(redirection.operator == ">&" && descriptor) && target != "/dev/null"
    })
}

/// How many words name what a command does, by the words it begins with.
const ARITIES: [(&[&str], usize); 43] = [
    (&["git"], 2),
    (&["git", "config"], 3),
    (&["git", "remote"], 3),
    (&["git", "stash"], 3),
    (&["npm"], 2),
    (&["npm", "run"], 3),
    (&["npm", "exec"], 3),
    (&["npx"], 2),
    (&["pnpm"], 2),
    (&["pnpm", "run"], 3),
    (&["pnpm", "dlx"], 3),
    (&["yarn"], 2),
    (&["yarn", "run"], 3),
    (&["bun"], 2),
    (&["bun", "run"], 3),
    (&["bun", "x"], 3),
    (&["bunx"], 2),
    (&["deno"], 2),
    (&["deno", "task"], 3),
    (&["cargo"], 2),
    (&["cargo", "run"], 3),
    (&["go"], 2),
    (&["zig"], 2),
    (&["make"], 2),
    (&["python"], 2),
    (&["python3"], 2),
    (&["pip"], 2),
    (&["uv"], 2),
    (&["uv", "run"], 3),
    (&["poetry"], 2),
    (&["docker"], 2),
    (&["docker", "compose"], 3),
    (&["kubectl"], 2),
    (&["helm"], 2),
    (&["terraform"], 2),
    (&["gh"], 3),
    (&["brew"], 2),
    (&["swift"], 2),
    (&["gradle"], 2),
    (&["mvn"], 2),
    (&["systemctl"], 2),
    (&["tmux"], 2),
    (&["rustup"], 2),
];

/// The words of `words` that name what it does: as many as the longest known beginning's arity says, its program alone
/// where none is known.
fn prefix_of<'w>(words: &'w [&'w str]) -> &'w [&'w str] {
    let known = ARITIES
        .iter()
        .filter(|(begins, _)| words.len() >= begins.len() && words[..begins.len()] == **begins)
        .max_by_key(|(begins, _)| begins.len());
    let arity = known.map_or(1, |(_, arity)| *arity);
    &words[..arity.min(words.len())]
}
