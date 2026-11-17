//! The shell's language read: a command line as Bash reads it, by tree-sitter's grammar — every simple command it runs,
//! wherever it stands: the variables it sets, its program, its words with their quotes and escapes taken away, and the
//! redirections bound to it, those of the statements around it as bash binds them — or why no reading of the line can
//! know what it runs: a syntax error, a command or a process substitution, a function it defines, a program an
//! expansion names. A computation on the line alone.

mod reader;

/// A word as the shell hands it to a program: its value, where the line alone fixes it, its quotes and escapes taken
/// away; or, where the shell expands it as it runs, its source as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Word {
    Literal(String),
    Expanded(String),
}

impl Word {
    /// Its value, where the line alone fixes it.
    #[must_use]
    pub fn literal(&self) -> Option<&str> {
        match self {
            Self::Literal(value) => Some(value),
            Self::Expanded(_) => None,
        }
    }

    /// Its value, or its source where the shell expands it.
    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            Self::Literal(text) | Self::Expanded(text) => text,
        }
    }
}

/// A variable a command sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub name: String,
    pub value: Word,
}

/// A redirection: the descriptor written before its operator, where one is; its operator; and what it reads or writes —
/// a here-document by its delimiter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirection {
    pub descriptor: Option<String>,
    pub operator: String,
    pub target: Word,
}

/// A simple command: the variables it sets, its program — none where it only sets variables or redirects — its
/// arguments, and the redirections bound to it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Command {
    pub assignments: Vec<Assignment>,
    pub program: Option<String>,
    pub arguments: Vec<Word>,
    pub redirections: Vec<Redirection>,
}

/// Why no reading of a line knows what it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Unknowable {
    #[error("syntax-error")]
    SyntaxError,
    #[error("command-substitution")]
    CommandSubstitution,
    #[error("process-substitution")]
    ProcessSubstitution,
    #[error("function-definition")]
    FunctionDefinition,
    #[error("dynamic-program")]
    DynamicProgram,
}

/// The simple commands `line` runs, in the order they stand.
///
/// # Errors
/// Why no reading of the line knows what it runs.
pub fn parse(line: &str) -> Result<Vec<Command>, Unknowable> {
    reader::parse(line)
}
