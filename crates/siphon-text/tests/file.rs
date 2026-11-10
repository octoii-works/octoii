//! A text file's bytes as text, and new text written as the file held its own.

use siphon_text::Text;

#[test]
fn a_text_files_bytes_are_text_with_its_mark_apart_and_new_text_is_written_as_the_file_held_its_own() {
    let marked = Text::of(b"\xef\xbb\xbfone\r\ntwo\r\n").expect("text");
    assert!(marked.marked);
    assert_eq!(marked.text, "one\r\ntwo\r\n");
    assert_eq!(Text::of(b"a\x00b"), None);
    assert_eq!(Text::of(b"\xff\xfe"), None);
    let written = marked.with_line_endings("three\nfour\r\n");
    assert_eq!(written, "three\r\nfour\r\n");
    assert_eq!(marked.encode(&written), b"\xef\xbb\xbfthree\r\nfour\r\n");
    assert_eq!(Text::of(b"plain\n").expect("text").with_line_endings("a\nb"), "a\nb");
    assert_eq!(Text::of(b"plain\n").expect("text").encode("a"), b"a");
}
