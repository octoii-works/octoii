//! An edit that finds its text by tiers and keeps the file's line endings, or says why it cannot be made.

use siphon_text::{Edit, Matching, Refusal};

fn edit<'a>(old: &'a str, new: &'a str) -> Edit<'a> {
    Edit { old, new, all: false }
}

#[test]
fn an_edit_finds_its_text_exactly_then_typographically_then_line_by_line_and_keeps_the_files_line_endings() {
    let exact = edit("let b = 2;\n", "let b = 3;\nlet c = 4;\n")
        .apply("let a = 1;\r\nlet b = 2;\r\n")
        .expect("made");
    assert_eq!(exact.matching, Matching::Exact);
    assert_eq!(exact.text, "let a = 1;\r\nlet b = 3;\r\nlet c = 4;\r\n");
    let curly = edit("say \"hello\" - now", "say \"bye\"")
        .apply("say \u{201c}hello\u{201d} \u{2014} now\n")
        .expect("made");
    assert_eq!(curly.matching, Matching::Typographic);
    assert_eq!(curly.text, "say \"bye\"\n");
    // Lines read loosely: a line the edit keeps is written as the file wrote it, its trailing spaces with it.
    let loose = edit("fn main() {\n    run();\n", "fn main() {\n    go();\n")
        .apply("fn main() {   \n    run();\t\n}\n")
        .expect("made");
    assert_eq!(loose.matching, Matching::Lines);
    assert_eq!(loose.text, "fn main() {   \n    go();\n}\n");
    // A line added among CRLF lines ends as they do; the text's last line keeps having no newline.
    let added = edit("b\nc", "b\nx\nc").apply("a\r\nb\r\nc").expect("made");
    assert_eq!((added.text.as_str(), added.replacements), ("a\r\nb\r\nx\r\nc", 1));
}

#[test]
fn an_edit_refuses_what_it_cannot_make_and_replaces_every_match_only_where_asked() {
    assert_eq!(edit("", "y").apply("x"), Err(Refusal::Empty));
    assert_eq!(edit("x", "x").apply("x"), Err(Refusal::Same));
    assert_eq!(edit("z", "y").apply("x"), Err(Refusal::NotFound));
    assert_eq!(edit("a", "b").apply("a a"), Err(Refusal::Many(2)));
    let every = Edit {
        all: true,
        ..edit("a", "b")
    }
    .apply("a a")
    .expect("made");
    assert_eq!((every.text.as_str(), every.replacements), ("b b", 2));
    // Where the two differ in their `\r` alone, the endings are what is edited: matched and replaced as written.
    assert_eq!(edit("one\r\n", "one\n").apply("one\r\ntwo\n").expect("made").text, "one\ntwo\n");
    assert_eq!(edit("a\n", "a  \n").apply("a  \n"), Err(Refusal::Unchanged));
    assert_eq!(Refusal::NotFound.to_string(), "not-found");
    assert_eq!(Matching::Typographic.name(), "typographic");
}

#[test]
fn lines_are_matched_loosely_typographic_characters_plain_and_trailing_blanks_aside() {
    let made = edit("say \"x\"\t\nnext\n", "said\nnext\n")
        .apply("say \u{201c}x\u{201d}\u{00a0}\nnext\n")
        .expect("made");
    assert_eq!((made.text.as_str(), made.matching), ("said\nnext\n", Matching::Lines));
    assert_eq!(edit("a b\n", "c\n").apply("ab\n"), Err(Refusal::NotFound));
    // A typographic character found by the second matching is replaced whole, the text around it kept.
    let made = edit("x - y", "x + y").apply("x \u{2212} y = z\n").expect("made");
    assert_eq!((made.text.as_str(), made.matching), ("x + y = z\n", Matching::Typographic));
}
