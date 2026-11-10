//! A page of a text's lines, read from where its bytes come, however they are split as they come.

use std::io::BufReader;

use siphon_text::{Bounds, Page, Paged};

/// The page `bounds` take of `text`, its bytes coming `range` at a time.
fn paged(text: &[u8], range: usize, bounds: Bounds) -> Paged {
    Page::read(BufReader::with_capacity(range, text), &bounds).expect("a slice reads")
}

fn page(text: &[u8], range: usize, bounds: Bounds) -> Page {
    match paged(text, range, bounds) {
        Paged::Text(page) => page,
        Paged::Binary => panic!("text read as binary"),
    }
}

#[test]
fn a_page_reads_a_texts_lines_however_they_come_shown_as_utf8_cut_where_long_and_whether_more_follow() {
    let text = b"one\r\ntwo\n\xffbad\nabcdefghij\nfive";
    for range in [1, 3, 64] {
        let page = page(
            text,
            range,
            Bounds {
                line_chars_max: 5,
                ..Bounds::default()
            },
        );
        assert!(page.whole);
        let lines: Vec<&str> = page.lines.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(lines, ["one", "two", "\u{fffd}bad", "abcde", "five"], "coming {range} at a time");
        assert!(page.lines[3].cut);
        assert!(!page.lines[0].cut);
    }
    let from_two = page(
        text,
        4,
        Bounds {
            offset: 2,
            limit: 2,
            ..Bounds::default()
        },
    );
    assert_eq!(from_two.lines[0].number, 2);
    assert_eq!(from_two.lines.len(), 2);
    assert!(!from_two.whole);
}

#[test]
fn a_text_ending_with_a_newline_has_no_line_after_it_and_a_nul_tells_it_is_no_text() {
    assert_eq!(page(b"a\nb\n", 2, Bounds::default()).seen, 2);
    assert_eq!(page(b"a\nb", 1, Bounds::default()).seen, 2);
    assert_eq!(page(b"", 1, Bounds::default()).seen, 0);
    assert_eq!(paged(b"text\x00more", 64, Bounds::default()), Paged::Binary);
}

#[test]
fn a_page_holds_no_more_bytes_than_it_may_the_line_past_them_left_to_the_next() {
    let bounded = page(
        b"0123456789\n0123456789\n",
        64,
        Bounds {
            bytes_max: 20,
            ..Bounds::default()
        },
    );
    assert_eq!(bounded.lines.len(), 1);
    assert!(!bounded.whole);
}

#[test]
fn a_stretch_of_bytes_that_is_no_utf8_is_one_replacement_as_a_decoder_reads_a_maximal_subpart() {
    // A character cut short is one U+FFFD; each byte that begins none is one of its own.
    assert_eq!(
        page(b"a\xe2\x82b\xff\xfec", 64, Bounds::default()).lines[0].text,
        "a\u{fffd}b\u{fffd}\u{fffd}c"
    );
    // A character split across what comes is whole on the page.
    assert_eq!(page("é€".as_bytes(), 1, Bounds::default()).lines[0].text, "é€");
}

#[test]
fn a_line_longer_than_it_shows_is_read_through_and_the_next_line_read_after_it() {
    let long = [vec![b'x'; 100_000], b"\nnext".to_vec()].concat();
    let read = page(
        &long,
        8192,
        Bounds {
            line_chars_max: 10,
            ..Bounds::default()
        },
    );
    assert_eq!((read.lines[0].text.as_str(), read.lines[0].cut), ("xxxxxxxxxx", true));
    assert_eq!(read.lines[1].text, "next");
}
