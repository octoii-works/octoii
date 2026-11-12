//! The machine through a namespace: paths as the model writes them, files read, written and edited as they held their
//! text, a walk as git sees a tree, glob and grep paged, programs run to their end, and what the host hides denied.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::machine::paths::Base;
use crate::machine::{EditOutcome, GrepOptions, Host, Machine, Program, Read, ReadOptions, RunError, Writes};
use beak::{Beak, Kernel, Options};
use siphon_text::Edit;
use tokio_util::sync::CancellationToken;

fn kernel(beak: &Beak) -> Kernel {
    Kernel::open(beak, &Options::default()).expect("a kernel").0
}

/// A machine working in a fresh folder, by its real path, and the folder.
fn machine(kernel: &Kernel, host: &Host) -> (Machine, tempfile::TempDir, PathBuf) {
    let folder = tempfile::tempdir().expect("a folder");
    let workspace = folder.path().canonicalize().expect("its real path");
    let base = Base {
        workspace: workspace.clone(),
        home: Some(PathBuf::from("/home/me")),
    };
    (
        Machine::open(kernel, host, base, &Writes::Anywhere, None).expect("opened"),
        folder,
        workspace,
    )
}

#[test]
fn a_path_names_one_place_whatever_its_spelling_and_is_shown_back_as_the_model_would_write_it() {
    let base = Base {
        workspace: PathBuf::from("/work/app"),
        home: Some(PathBuf::from("/home/me")),
    };
    let cases = [
        ("src/main.rs", "/work/app/src/main.rs", "src/main.rs"),
        ("./src//main.rs", "/work/app/src/main.rs", "src/main.rs"),
        ("src/../build.rs", "/work/app/build.rs", "build.rs"),
        ("", "/work/app", "."),
        ("/work/app/src", "/work/app/src", "src"),
        ("../lib/x.rs", "/work/lib/x.rs", "/work/lib/x.rs"),
        ("/etc/hosts", "/etc/hosts", "/etc/hosts"),
        ("~/.config/app.toml", "/home/me/.config/app.toml", "/home/me/.config/app.toml"),
        ("~", "/home/me", "/home/me"),
        ("~me/x", "/work/app/~me/x", "~me/x"),
        ("/work/application", "/work/application", "/work/application"),
    ];
    for (given, place, shown) in cases {
        let resolved = base.resolve(given);
        assert_eq!(resolved, Path::new(place), "{given}");
        assert_eq!(base.shown(&resolved), shown, "{given}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_is_written_read_a_page_at_a_time_and_edited_as_it_held_its_text() {
    let beak = Beak::new().expect("Beak");
    let kernel = kernel(&beak);
    let (machine, _folder, workspace) = machine(&kernel, &Host::default());
    let path = machine.path("deep/down/notes.txt");
    let made = machine.write(&path, "one\ntwo\nthree\n", "deep/down/notes.txt").expect("written");
    assert!(made.created);
    assert_eq!((made.diff.additions, made.diff.deletions), (3, 0));
    // A file whose lines end \r\n, after a byte-order mark, keeps both as it is written and edited.
    let marked = workspace.join("marked.txt");
    std::fs::write(&marked, b"\xef\xbb\xbfa\r\nb\r\n").expect("written");
    let path = machine.path("marked.txt");
    machine.write(&path, "a\nc\n", "marked.txt").expect("written");
    assert_eq!(std::fs::read(&marked).expect("read"), b"\xef\xbb\xbfa\r\nc\r\n");
    let edit = Edit {
        old: "c",
        new: "d",
        all: false,
    };
    let EditOutcome::Made { change, .. } = machine.edit(&path, &edit, "marked.txt").expect("edited") else {
        panic!("made")
    };
    assert_eq!((change.diff.additions, change.diff.deletions), (1, 1));
    assert_eq!(std::fs::read(&marked).expect("read"), b"\xef\xbb\xbfa\r\nd\r\n");
    let refused = machine
        .edit(
            &path,
            &Edit {
                old: "zzz",
                new: "y",
                all: false,
            },
            "marked.txt",
        )
        .expect("answered");
    assert!(matches!(refused, EditOutcome::Refused(_)));
    let Read::Text { page, .. } = machine
        .read(&machine.path("deep/down/notes.txt"), &ReadOptions::default())
        .expect("read")
    else {
        panic!("text")
    };
    let lines: Vec<&str> = page.lines.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(lines, ["one", "two", "three"]);
    std::fs::write(workspace.join("shot.png"), b"\x89PNG\r\n\x1a\n rest").expect("written");
    assert!(matches!(
        machine.read(&machine.path("shot.png"), &ReadOptions::default()).expect("read"),
        Read::Media {
            media_type: "image/png",
            ..
        }
    ));
    // A name typed with plain spaces finds the one the system holds with a narrow one.
    std::fs::write(workspace.join("Screenshot 10.00\u{202f}AM.txt"), "shot").expect("written");
    assert!(matches!(
        machine
            .read(&machine.path("Screenshot 10.00 AM.txt"), &ReadOptions::default())
            .expect("read"),
        Read::Text { .. }
    ));
    let Read::Directory { entries, total, .. } = machine.read(&machine.path(""), &ReadOptions::default()).expect("read") else {
        panic!("a folder")
    };
    assert_eq!((total, &entries[0].name[..]), (4, &b"deep"[..]), "folders first");
}

fn write(root: &Path, path: &str, text: &str) {
    let place = root.join(path);
    std::fs::create_dir_all(place.parent().expect("a parent")).expect("folders");
    std::fs::write(place, text).expect("written");
}

/// A repository whose ignore files leave out its build, its logs, a generated file, a local folder and the person's
/// secrets: the machine working in it, and what holds its folders.
fn repository(kernel: &Kernel) -> (Machine, [tempfile::TempDir; 2]) {
    let excludes = tempfile::tempdir().expect("a folder");
    let personal = excludes.path().canonicalize().expect("its real path").join("ignore");
    std::fs::write(&personal, "*.secret\n").expect("written");
    let host = Host {
        excludes: Some(personal),
        ..Host::default()
    };
    let (machine, folder, root) = machine(kernel, &host);
    write(&root, ".git/info/exclude", "local/\n");
    write(&root, ".gitignore", "target/\n*.log\n");
    write(&root, "src/main.rs", "fn main() {\n    println!(\"hello\");\n}\n");
    write(&root, "src/lib.rs", "pub fn hello() {}\n// hello again\n");
    write(&root, "src/deep/.ignore", "generated.rs\n");
    write(&root, "src/deep/generated.rs", "hello\n");
    write(&root, "src/deep/kept.rs", "hello\n");
    write(&root, "target/debug/out.rs", "hello\n");
    write(&root, "local/notes.rs", "hello\n");
    write(&root, "run.log", "hello\n");
    write(&root, "key.secret", "hello\n");
    std::fs::write(root.join("data.bin"), b"hello\x00\x01").expect("written");
    (machine, [excludes, folder])
}

#[tokio::test(flavor = "multi_thread")]
async fn a_walk_leaves_out_what_ignore_files_name_and_glob_pages_what_is_left_the_newest_first() {
    let beak = Beak::new().expect("Beak");
    let kernel = kernel(&beak);
    let (machine, _folders) = repository(&kernel);
    let shown = |paths: &[String]| paths.iter().map(|path| machine.shown(path)).collect::<Vec<_>>();
    let walked = machine.walk(&machine.path("")).expect("walked");
    assert_eq!(
        shown(&walked.files),
        [
            ".gitignore",
            "data.bin",
            "src/deep/.ignore",
            "src/deep/kept.rs",
            "src/lib.rs",
            "src/main.rs"
        ]
    );
    // From within the tree, the rules above it hold still.
    let deep = machine.walk(&machine.path("src/deep")).expect("walked");
    assert_eq!(shown(&deep.files), ["src/deep/.ignore", "src/deep/kept.rs"]);
    let globbed = machine.glob(&machine.path(""), "src/**/*.rs", 0, 2).expect("globbed");
    assert_eq!((globbed.files.len(), globbed.total), (2, 3));
}

#[tokio::test(flavor = "multi_thread")]
async fn grep_pages_its_matches_in_order_by_path_with_their_context_and_passes_binary_files_over() {
    let beak = Beak::new().expect("Beak");
    let kernel = kernel(&beak);
    let (machine, _folders) = repository(&kernel);
    let options = GrepOptions {
        context: 1,
        glob: Some("*.rs".into()),
        limit: 2,
        ..default_grep()
    };
    let first = machine.grep(&machine.path(""), "hello", &options).expect("grepped");
    let found: Vec<(String, u64, bool)> = first
        .files
        .iter()
        .flat_map(|file| file.lines.iter().map(|line| (machine.shown(&file.path), line.number, line.matched)))
        .collect();
    assert_eq!(
        found,
        [("src/deep/kept.rs".into(), 1, true), ("src/lib.rs".into(), 1, true)],
        "a match past the page is not shown as another's context"
    );
    assert!(first.more, "a page says more follow");
    let rest = machine
        .grep(
            &machine.path(""),
            "hello",
            &GrepOptions {
                offset: 2,
                ..options.clone()
            },
        )
        .expect("grepped");
    let matched: Vec<u64> = rest
        .files
        .iter()
        .flat_map(|file| file.lines.iter().filter(|line| line.matched).map(|line| line.number))
        .collect();
    assert_eq!((matched, rest.more), (vec![2, 2], false));
    let main = &rest.files[1].lines;
    assert_eq!((main[0].number, main[0].matched), (1, false), "its context about it");
    let everywhere = machine
        .grep(
            &machine.path(""),
            "HELLO",
            &GrepOptions {
                case_insensitive: true,
                glob: None,
                limit: 100,
                ..options
            },
        )
        .expect("grepped");
    assert_eq!(everywhere.binary, 1, "the binary file passed over");
    assert!(machine.grep(&machine.path(""), "(", &default_grep()).is_err());
}

fn default_grep() -> GrepOptions {
    GrepOptions {
        literal: false,
        case_insensitive: false,
        context: 0,
        glob: None,
        offset: 0,
        limit: 10,
        line_chars_max: 500,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_program_runs_to_its_end_and_what_the_host_hides_is_denied() {
    let beak = Beak::new().expect("Beak");
    let kernel = kernel(&beak);
    let hidden = tempfile::tempdir().expect("a folder");
    let hidden_place = hidden.path().canonicalize().expect("its real path");
    std::fs::write(hidden_place.join("key"), "secret").expect("written");
    let host = Host {
        hidden: vec![hidden_place.clone()],
        ..Host::default()
    };
    let (machine, _folder, workspace) = machine(&kernel, &host);
    let never = CancellationToken::new();
    let echo = Program::new(
        vec!["sh".into(), "-c".into(), "printf out; printf err >&2; exit 3".into()],
        &workspace,
    );
    let ran = machine.run(1_000, b"", &echo, &never).await.expect("ran");
    assert_eq!(
        (ran.exit, &ran.output[..], &ran.errors[..]),
        (beak::Exit::Code(3), &b"out"[..], &b"err"[..])
    );
    let missing = Program::new(vec!["no-such-program-anywhere".into()], &workspace);
    assert!(matches!(
        machine.run(1_000, b"", &missing, &never).await,
        Err(RunError::MissingProgram(_))
    ));
    let slow = Program {
        timeout: Duration::from_millis(200),
        ..Program::new(vec!["sleep".into(), "30".into()], &workspace)
    };
    assert!(matches!(machine.run(1_000, b"", &slow, &never).await, Err(RunError::TimedOut(_))));
    let key = machine.path(&hidden_place.join("key").to_string_lossy());
    assert!(
        machine.read(&key, &ReadOptions::default()).is_err(),
        "nothing reaches what the host hides"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn work_held_to_its_workspace_writes_there_and_reads_everywhere_else() {
    let beak = Beak::new().expect("Beak");
    let kernel = kernel(&beak);
    let (workspace_folder, outside_folder) = (tempfile::tempdir().expect("a folder"), tempfile::tempdir().expect("a folder"));
    let workspace = workspace_folder.path().canonicalize().expect("its real path");
    let outside = outside_folder.path().canonicalize().expect("its real path");
    std::fs::write(outside.join("notes.txt"), "kept").expect("written");
    let base = Base {
        workspace: workspace.clone(),
        home: None,
    };
    let machine = Machine::open(&kernel, &Host::default(), base, &Writes::Within(vec![workspace.clone()]), None).expect("opened");
    machine
        .write(&machine.path("made.txt"), "here", "made.txt")
        .expect("written within");
    let elsewhere = machine.path(&outside.join("notes.txt").to_string_lossy());
    assert!(
        machine.write(&elsewhere, "changed", "notes.txt").is_err(),
        "nothing written beyond the workspace"
    );
    assert!(
        matches!(machine.read(&elsewhere, &ReadOptions::default()).expect("read"), Read::Text { .. }),
        "read everywhere"
    );
    let never = CancellationToken::new();
    let line = format!("echo made > made-too.txt && echo changed > {}", outside.join("notes.txt").display());
    let ran = machine
        .run(1_000, b"", &Program::new(vec!["sh".into(), "-c".into(), line], &workspace), &never)
        .await;
    if beak::machine_confines() {
        let ran = ran.expect("ran");
        assert_ne!(ran.exit, beak::Exit::Code(0), "its command held to the workspace");
        assert_eq!(std::fs::read_to_string(outside.join("notes.txt")).expect("read"), "kept");
        assert!(workspace.join("made-too.txt").exists());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_coprocess_reads_what_it_is_given_answers_on_its_output_keeps_its_errors_and_ends_as_its_input_closes() {
    use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};

    let beak = Beak::new().expect("Beak");
    let kernel = kernel(&beak);
    let (machine, _folder, workspace) = machine(&kernel, &Host::default());
    let argv = vec![
        "sh".into(),
        "-c".into(),
        "printf started >&2; while read line; do echo \"$WORD $line\"; done".into(),
    ];
    let variables = crate::machine::Variables::Over(vec![("WORD".into(), "heard".into())]);
    let crate::machine::Coprocess { mut stdin, stdout, stderr } =
        machine.coprocess(1_000, b"", argv, &workspace, variables).expect("started");
    let mut stdout = BufReader::new(stdout);
    stdin.write_all(b"one\n").await.expect("written");
    let mut line = String::new();
    stdout.read_line(&mut line).await.expect("read");
    assert_eq!(line, "heard one\n");
    assert_eq!(&stderr.kept().expect("kept")[..], b"started");
    stdin.shutdown().await.expect("closed");
    let mut rest = String::new();
    stdout.read_to_string(&mut rest).await.expect("read to its end");
    assert_eq!(rest, "", "its output ends as it does");
    let missing = machine.coprocess(
        1_000,
        b"",
        vec!["no-such-program-anywhere".into()],
        &workspace,
        crate::machine::Variables::Over(Vec::new()),
    );
    assert!(matches!(missing, Err(RunError::MissingProgram(_))));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_coprocess_given_its_variables_alone_runs_with_those_alone() {
    use tokio::io::AsyncReadExt as _;

    let beak = Beak::new().expect("Beak");
    let kernel = kernel(&beak);
    let (machine, _folder, workspace) = machine(&kernel, &Host::default());
    let argv = vec!["/bin/sh".into(), "-c".into(), "printf '%s|%s' \"$WORD\" \"${HOME-none}\"".into()];
    let only = crate::machine::Variables::Only(vec![("WORD".into(), "alone".into())]);
    let mut coprocess = machine.coprocess(1_000, b"", argv, &workspace, only).expect("started");
    let mut told = String::new();
    coprocess.stdout.read_to_string(&mut told).await.expect("read");
    assert_eq!(told, "alone|none", "nothing of the machine's own");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_bound_back_over_what_the_host_hides_is_reached_by_a_command_and_the_rest_is_not() {
    use tokio::io::AsyncReadExt as _;

    let beak = Beak::new().expect("Beak");
    let kernel = kernel(&beak);
    let kept = tempfile::tempdir().expect("a folder");
    let kept = kept.path().canonicalize().expect("its real path");
    let own = kept.join("extension");
    std::fs::create_dir_all(&own).expect("its folder");
    std::fs::write(kept.join("secret"), "s").expect("written");
    let host = Host {
        hidden: vec![kept.clone()],
        ..Host::default()
    };
    let (machine, _folder, workspace) = machine(&kernel, &host);
    machine.bind(&own).expect("bound");
    let line = format!(
        "printf x > '{}/made' && printf ok; cat '{}/secret' 2>/dev/null || printf ' hidden'",
        own.display(),
        kept.display()
    );
    let argv = vec!["/bin/sh".into(), "-c".into(), line];
    let mut coprocess = machine
        .coprocess(1_000, b"", argv, &workspace, crate::machine::Variables::Over(Vec::new()))
        .expect("started");
    let mut told = String::new();
    coprocess.stdout.read_to_string(&mut told).await.expect("read");
    assert_eq!(told, if beak::machine_confines() { "ok hidden" } else { "oks" });
    assert!(own.join("made").is_file(), "its own folder written");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_at_which_nothing_is_is_told_the_nearest_folder_and_the_names_like_it() {
    let beak = Beak::new().expect("Beak");
    let kernel = kernel(&beak);
    let (machine, _folder, workspace) = machine(&kernel, &Host::default());
    std::fs::create_dir_all(workspace.join("src")).expect("a folder");
    for name in ["main.rs", "lib.rs", "Mainly.txt", "other.md"] {
        std::fs::write(workspace.join("src").join(name), "").expect("a file");
    }
    let missing = machine.missing(&machine.path("src/mian.rs")).expect("looked");
    assert_eq!(machine.shown(&missing.folder), "src");
    assert_eq!(missing.like, vec!["main.rs".to_owned()], "within two edits");
    let deeper = machine.missing(&machine.path("src/gone/away/main.rs")).expect("looked");
    assert_eq!(machine.shown(&deeper.folder), "src", "the nearest folder on its way");
    assert!(deeper.like.is_empty(), "nothing in it like gone");
    let cased = machine.missing(&machine.path("src/MAIN")).expect("looked");
    assert_eq!(
        cased.like,
        vec!["main.rs".to_owned(), "Mainly.txt".to_owned()],
        "the same stem first, then one holding it"
    );
}
