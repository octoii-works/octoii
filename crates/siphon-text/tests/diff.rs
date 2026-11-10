//! The diff between two texts: a unified diff with its context, and the lines it adds and removes.

use siphon_text::{Diff, diff};

#[test]
fn a_unified_diff_shows_each_change_with_its_context_marks_a_missing_newline_and_names_a_new_files_dev_null() {
    assert_eq!(
        diff(Some("one\ntwo\nthree\n"), "one\ndeux\nthree\n", "f"),
        Diff {
            unified: "--- f\n+++ f\n@@ -1,3 +1,3 @@\n one\n-two\n+deux\n three".into(),
            additions: 1,
            deletions: 1,
        }
    );
    assert_eq!(
        diff(None, "new", "g").unified,
        "--- /dev/null\n+++ g\n@@ -0,0 +1 @@\n+new\n\\ No newline at end of file"
    );
    assert_eq!(
        diff(Some("same\n"), "same\n", "h"),
        Diff {
            unified: String::new(),
            additions: 0,
            deletions: 0,
        }
    );
}

#[test]
fn changes_far_apart_are_hunks_of_their_own_and_a_crlf_files_lines_are_shown_without_their_cr() {
    let far = diff(
        Some("1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n9\r\n10\r\n"),
        "one\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n9\r\nten\r\n",
        "f",
    );
    assert_eq!(far.unified.matches("@@ -").count(), 2);
    assert!(!far.unified.contains('\r'));
    assert_eq!((far.additions, far.deletions), (2, 2));
    // Changes nearer than twice the context are one hunk.
    let near = diff(Some("1\n2\n3\n4\n5\n6\n"), "one\n2\n3\n4\n5\nsix\n", "f");
    assert_eq!(near.unified.matches("@@ -").count(), 1);
}

#[test]
fn the_shortest_edit_finds_lines_moved_as_kept_not_rewritten_whole() {
    let within = diff(Some("s\na\nb\nc\ne\n"), "s\nb\nc\nx\ne\n", "f");
    assert_eq!((within.additions, within.deletions), (1, 1));
    let made = diff(Some(""), "a\nb\n", "f");
    assert_eq!((made.additions, made.deletions), (2, 0));
}
