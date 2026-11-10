//! An edit that finds its text by tiers and keeps the file's line endings, or says why it cannot be made.

use siphon_text::{Refusal, Request, Tier, edit, same_loosely};

fn request<'a>(old: &'a str, new: &'a str) -> Request<'a> {
    Request { old, new, all: false }
}

#[test]
fn an_edit_finds_its_text_exactly_then_typographically_then_line_by_line_and_keeps_the_files_line_endings() {
    let exact = edit("let a = 1;\r\nlet b = 2;\r\n", &request("let b = 2;\n", "let b = 3;\nlet c = 4;\n")).expect("made");
    assert_eq!(exact.tier, Tier::Exact);
    assert_eq!(exact.text, "let a = 1;\r\nlet b = 3;\r\nlet c = 4;\r\n");
    let curly = edit(
        "say \u{201c}hello\u{201d} \u{2014} now\n",
        &request("say \"hello\" - now", "say \"bye\""),
    )
    .expect("made");
    assert_eq!(curly.tier, Tier::Typographic);
    assert_eq!(curly.text, "say \"bye\"\n");
    // Lines read loosely: a line the edit keeps is written as the file wrote it, its trailing spaces with it.
    let loose = edit(
        "fn main() {   \n    run();\t\n}\n",
        &request("fn main() {\n    run();\n", "fn main() {\n    go();\n"),
    )
    .expect("made");
    assert_eq!(loose.tier, Tier::Lines);
    assert_eq!(loose.text, "fn main() {   \n    go();\n}\n");
    // A line added among CRLF lines ends as they do; the text's last line keeps having no newline.
    let added = edit("a\r\nb\r\nc", &request("b\nc", "b\nx\nc")).expect("made");
    assert_eq!(added.text, "a\r\nb\r\nx\r\nc");
    assert_eq!(added.replacements, 1);
}

#[test]
fn an_edit_refuses_what_it_cannot_make_and_replaces_every_match_only_where_asked() {
    assert_eq!(edit("x", &request("", "y")), Err(Refusal::Empty));
    assert_eq!(edit("x", &request("x", "x")), Err(Refusal::Same));
    assert_eq!(edit("x", &request("z", "y")), Err(Refusal::NotFound));
    assert_eq!(edit("a a", &request("a", "b")), Err(Refusal::Many(2)));
    let every = edit(
        "a a",
        &Request {
            all: true,
            ..request("a", "b")
        },
    )
    .expect("made");
    assert_eq!((every.text.as_str(), every.replacements), ("b b", 2));
    // Where the two differ in their `\r` alone, the endings are what is edited: matched and replaced as written.
    assert_eq!(edit("one\r\ntwo\n", &request("one\r\n", "one\n")).expect("made").text, "one\ntwo\n");
    assert_eq!(edit("a  \n", &request("a\n", "a  \n")), Err(Refusal::Unchanged));
    assert_eq!(Refusal::NotFound.to_string(), "not-found");
    assert_eq!(Tier::Typographic.name(), "typographic");
}

#[test]
fn lines_are_the_same_loosely_with_typographic_characters_plain_and_trailing_blanks_aside() {
    assert!(same_loosely("say \u{201c}x\u{201d}\u{00a0}", "say \"x\"\t"));
    assert!(!same_loosely("a b", "ab"));
    // A typographic character found by the second tier is replaced whole, the text around it kept.
    let made = edit("x \u{2212} y = z\n", &request("x - y", "x + y")).expect("made");
    assert_eq!((made.text.as_str(), made.tier), ("x + y = z\n", Tier::Typographic));
}
