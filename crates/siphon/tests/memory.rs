//! The memory of a project as the person sees it: each memory by its front matter, its index left out; one deleted
//! with its line of the index, the others' kept; a name that is not a memory's refused.

use siphon::agent::memory::{self, Error, INDEX};

#[test]
fn a_memory_is_listed_by_its_front_matter_and_deleted_with_its_line_of_the_index() {
    let folder = tempfile::tempdir().expect("a folder");
    let path = folder.path();
    std::fs::write(
        path.join("tests.md"),
        "---\nname: Tests\ndescription: \"the suite runs with cargo test\"\ntype: project\n---\nRun `cargo test`.",
    )
    .expect("a memory");
    std::fs::write(path.join("plain.md"), "No front matter.").expect("another");
    std::fs::write(path.join("notes.txt"), "not a memory").expect("a file");
    std::fs::write(
        path.join(INDEX),
        "- [Tests](tests.md) — the suite\n- [plain](plain.md) — a plain one\n",
    )
    .expect("the index");
    let listed = memory::list(path);
    let shown: Vec<_> = listed
        .iter()
        .map(|memory| {
            (
                memory.file.as_str(),
                memory.name.as_str(),
                memory.description.as_str(),
                memory.r#type.as_str(),
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("plain.md", "plain", "", ""),
            ("tests.md", "Tests", "the suite runs with cargo test", "project")
        ]
    );
    memory::delete(path, "tests.md").expect("deleted");
    assert!(!path.join("tests.md").exists());
    assert_eq!(
        std::fs::read_to_string(path.join(INDEX)).expect("the index"),
        "- [plain](plain.md) — a plain one\n"
    );
    assert!(matches!(memory::delete(path, INDEX), Err(Error::NotAMemory(_))));
    assert!(matches!(memory::delete(path, "../tests.md"), Err(Error::NotAMemory(_))));
    assert!(matches!(memory::delete(path, "gone.md"), Err(Error::Io(_))));
    assert!(memory::list(&path.join("none")).is_empty(), "no folder, no memories");
}

#[test]
fn the_model_is_given_the_index_or_told_there_is_none() {
    let folder = tempfile::tempdir().expect("a folder");
    assert!(memory::told(folder.path()).contains("<index>\n(none yet)\n</index>"));
    std::fs::write(folder.path().join(INDEX), "- [Tests](tests.md) — the suite\n\n").expect("the index");
    assert!(memory::told(folder.path()).contains("<index>\n- [Tests](tests.md) — the suite\n</index>"));
}
