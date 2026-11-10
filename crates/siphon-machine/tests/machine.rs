//! The machine through a namespace: paths as the model writes them, files read, written and edited as they held their
//! text, a walk as git sees a tree, glob and grep paged, programs run to their end, and what the host hides denied.

use std::path::{Path, PathBuf};
use std::time::Duration;

use beak::{Beak, Kernel, Options};
use siphon_machine::paths::Base;
use siphon_machine::{EditOutcome, GrepOptions, Host, Machine, Program, Read, ReadOptions, RunError};
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
    (Machine::open(kernel, host, base, None).expect("opened"), folder, workspace)
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
