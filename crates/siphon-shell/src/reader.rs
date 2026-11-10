//! The commands of a line, read from its tree statement by statement.

use tree_sitter::{Node, Parser};

use crate::{Assignment, Command, Redirection, Unknowable, Word};

pub(crate) fn parse(line: &str) -> Result<Vec<Command>, Unknowable> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .expect("the grammar is of the library's version");
    let tree = parser.parse(line, None).expect("a parse with no timeout and no cancellation ends");
    let root = tree.root_node();
    if root.has_error() {
        return Err(Unknowable::SyntaxError);
    }
    if let Some(why) = unknowable_in(root) {
        return Err(why);
    }
    let mut reader = Reader {
        line,
        commands: Vec::new(),
        dynamic: false,
    };
    reader.statement(root);
    if reader.dynamic {
        return Err(Unknowable::DynamicProgram);
    }
    Ok(reader.commands)
}

/// What, within `node`, runs what no reading of the line knows: the first substitution or function definition in it.
fn unknowable_in(node: Node<'_>) -> Option<Unknowable> {
    match node.kind() {
        "command_substitution" => Some(Unknowable::CommandSubstitution),
        "process_substitution" => Some(Unknowable::ProcessSubstitution),
        "function_definition" => Some(Unknowable::FunctionDefinition),
        _ => children(node).find_map(unknowable_in),
    }
}

fn children(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    (0..node.child_count()).filter_map(move |at| node.child(at))
}

/// The children of `node` the grammar names, each with its field, empty where it has none.
fn named(node: Node<'_>) -> impl Iterator<Item = (Node<'_>, &str)> {
    (0..node.child_count()).filter_map(move |at| {
        let child = node.child(at)?;
        child.is_named().then(|| (child, node.field_name_for_child(at).unwrap_or("")))
    })
}

/// The first child of `node` the grammar leaves anonymous: a keyword or an operator, its source saying which.
fn first_anonymous(node: Node<'_>) -> Option<Node<'_>> {
    children(node).find(|child| !child.is_named())
}

struct Reader<'a> {
    line: &'a str,
    commands: Vec<Command>,
    /// Whether an expansion names a program.
    dynamic: bool,
}

impl Reader<'_> {
    fn source(&self, node: Node<'_>) -> &str {
        &self.line[node.start_byte()..node.end_byte()]
    }

    /// Reads the commands a statement runs: a command; a statement with its redirections; variables set alone; else the
    /// statements within it — of a list, a pipeline, a group, a subshell, a loop or a condition — in their order.
    fn statement(&mut self, node: Node<'_>) {
        match node.kind() {
            "command" => self.command(node),
            "declaration_command" | "unset_command" | "test_command" => self.keyword_command(node),
            "variable_assignment" => self.assignments_alone(&[node]),
            "variable_assignments" => self.assignments_alone(&named(node).map(|(child, _)| child).collect::<Vec<_>>()),
            "redirected_statement" => self.redirected(node),
            _ => named(node).for_each(|(child, _)| self.statement(child)),
        }
    }

    fn command(&mut self, node: Node<'_>) {
        let mut command = Command::default();
        for (child, field) in named(node) {
            match field {
                "name" => match self.word(named(child).next().map_or(child, |(name, _)| name)) {
                    Word::Literal(program) => command.program = Some(program),
                    Word::Expanded(_) => self.dynamic = true,
                },
                "argument" => command.arguments.push(self.word(child)),
                "redirect" => command.redirections.push(self.redirection(child)),
                _ if child.kind() == "variable_assignment" => command.assignments.push(self.assignment(child)),
                _ => {}
            }
        }
        self.commands.push(command);
    }

    /// `export`, `declare`, `local`, `readonly`, `unset`, `[[` and `[`: the keyword the program, what follows its words.
    fn keyword_command(&mut self, node: Node<'_>) {
        let mut command = Command {
            program: first_anonymous(node).map(|keyword| self.source(keyword).to_owned()),
            ..Command::default()
        };
        for (child, _) in named(node) {
            if child.kind() == "variable_assignment" {
                command.assignments.push(self.assignment(child));
            } else {
                command.arguments.push(self.word(child));
            }
        }
        self.commands.push(command);
    }

    /// A statement that only sets variables: a command with no program.
    fn assignments_alone(&mut self, nodes: &[Node<'_>]) {
        let assignments = nodes.iter().map(|node| self.assignment(*node)).collect();
        self.commands.push(Command {
            assignments,
            ..Command::default()
        });
    }

    /// A statement and the redirections after it, bound as bash binds them: after a list or a pipeline, to its last
    /// statement; after any other, to every command it runs, in its groups and subshells too. A here-document's line
    /// may go on: what it redirects there binds with it, the arguments there are its command's, and the pipeline or the
    /// list it goes on with binds to none of it. Redirections with no statement are a command with no program.
    fn redirected(&mut self, node: Node<'_>) {
        let (mut bound, mut end) = (self.commands.len(), self.commands.len());
        let (mut redirections, mut arguments) = (Vec::new(), Vec::new());
        for (child, field) in named(node) {
            if field == "body" {
                bound = self.last_statement(child);
                end = self.commands.len();
            } else {
                redirections.push(self.redirection(child));
                if child.kind() == "heredoc_redirect" {
                    self.here_document_line(child, &mut redirections, &mut arguments);
                }
            }
        }
        if end == bound {
            self.commands.insert(bound, Command::default());
            end = bound + 1;
        }
        for command in &mut self.commands[bound..end] {
            command.redirections.extend(redirections.iter().cloned());
        }
        self.commands[end - 1].arguments.extend(arguments);
    }

    /// Reads a statement after which redirections stand; the first of the commands they bind to: of a list or a
    /// pipeline, those of its last statement; of any other statement, all it runs.
    fn last_statement(&mut self, node: Node<'_>) -> usize {
        if !matches!(node.kind(), "list" | "pipeline") {
            let first = self.commands.len();
            self.statement(node);
            return first;
        }
        let mut last = self.commands.len();
        for (child, _) in named(node) {
            last = self.last_statement(child);
        }
        last
    }

    /// What a here-document's line goes on with past its delimiter: its redirections and its command's arguments, kept,
    /// and the statements it goes on to, read.
    fn here_document_line(&mut self, node: Node<'_>, redirections: &mut Vec<Redirection>, arguments: &mut Vec<Word>) {
        for (child, field) in named(node) {
            match (field, child.kind()) {
                ("redirect", _) => redirections.push(self.redirection(child)),
                ("argument", _) => arguments.push(self.word(child)),
                (_, "heredoc_start" | "heredoc_body" | "heredoc_end" | "file_descriptor") => {}
                _ => self.statement(child),
            }
        }
    }

    fn redirection(&self, node: Node<'_>) -> Redirection {
        let mut made = Redirection {
            descriptor: None,
            operator: first_anonymous(node)
                .map(|operator| self.source(operator).to_owned())
                .unwrap_or_default(),
            target: Word::Literal(String::new()),
        };
        for (child, field) in named(node) {
            if child.kind() == "file_descriptor" {
                made.descriptor = Some(self.source(child).to_owned());
            } else if child.kind() == "heredoc_start" {
                made.target = Word::Literal(self.source(child).trim_matches(['\'', '"']).to_owned());
            } else if field == "destination" || node.kind() == "herestring_redirect" {
                made.target = self.word(child);
            }
        }
        made
    }

    fn assignment(&self, node: Node<'_>) -> Assignment {
        let mut made = Assignment {
            name: String::new(),
            value: Word::Literal(String::new()),
        };
        for (child, field) in named(node) {
            match field {
                "name" => self.source(child).clone_into(&mut made.name),
                "value" => made.value = self.word(child),
                _ => {}
            }
        }
        made
    }

    /// A word's value, its quotes and escapes taken away, where the line alone fixes it: a bare word with no pattern
    /// and no leading `~`, a number, a name, a single-quoted string, a double-quoted one that expands nothing, and what
    /// is made of such alone; else its source.
    fn word(&self, node: Node<'_>) -> Word {
        let written = self.source(node);
        let expanded = || Word::Expanded(written.to_owned());
        match node.kind() {
            "number" | "variable_name" => Word::Literal(written.to_owned()),
            "raw_string" => Word::Literal(written[1..written.len() - 1].to_owned()),
            "word" if written.contains(['*', '?', '[']) || written.starts_with('~') => expanded(),
            "word" => Word::Literal(unescaped(written, Escapes::Any)),
            "string" => named(node)
                .map(|(child, _)| (child.kind() == "string_content").then(|| unescaped(self.source(child), Escapes::DoubleQuoted)))
                .collect::<Option<String>>()
                .map_or_else(expanded, Word::Literal),
            "concatenation" => children(node)
                .map(|part| match self.word(part) {
                    Word::Literal(value) => Some(value),
                    Word::Expanded(_) => None,
                })
                .collect::<Option<String>>()
                .map_or_else(expanded, Word::Literal),
            _ => expanded(),
        }
    }
}

/// Which characters a backslash takes away before: any, as outside quotes; or `\`, `"`, `$` and `` ` `` alone, as
/// within double quotes. Before a newline it is taken away with it, as a line goes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Escapes {
    Any,
    DoubleQuoted,
}

fn unescaped(text: &str, escapes: Escapes) -> String {
    let mut value = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\\'
            && let Some(&next) = characters.peek()
            && (next == '\n' || escapes == Escapes::Any || matches!(next, '\\' | '"' | '$' | '`'))
        {
            characters.next();
            if next != '\n' {
                value.push(next);
            }
            continue;
        }
        value.push(character);
    }
    value
}
