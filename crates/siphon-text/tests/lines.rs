//! A page of a text's lines, gathered across the ranges it is read in.

use siphon_text::{Bounds, Pager};

/// The page `bounds` take of `text`, given in ranges of `range` bytes.
fn paged(text: &[u8], range: usize, bounds: Bounds) -> Pager {
    let mut pager = Pager::new(bounds);
    for piece in text.chunks(range) {
        if !pager.take(piece) {
            return pager;
        }
    }
    pager.finish();
    pager
}

#[test]
fn a_page_gathers_a_texts_lines_across_its_ranges_shown_as_utf8_cut_where_long_and_whether_more_follow() {
    let text = b"one\r\ntwo\n\xffbad\nabcdefghij\nfive";
    for range in [1, 3, 64] {
        let page = paged(
            text,
            range,
            Bounds {
                line_chars_max: 5,
                ..Bounds::default()
            },
        );
        assert!(page.whole());
        let lines: Vec<&str> = page.lines().iter().map(|line| line.text.as_str()).collect();
        assert_eq!(lines, ["one", "two", "\u{fffd}bad", "abcde", "five"], "in ranges of {range}");
        assert!(page.lines()[3].cut);
        assert!(!page.lines()[0].cut);
    }
    let page = paged(
        text,
        4,
        Bounds {
            offset: 2,
            limit: 2,
            ..Bounds::default()
        },
    );
    assert_eq!(page.lines()[0].number, 2);
    assert_eq!(page.lines().len(), 2);
    assert!(!page.whole());
}

#[test]
fn a_text_ending_with_a_newline_has_no_line_after_it_and_a_nul_tells_it_is_no_text() {
    assert_eq!(paged(b"a\nb\n", 2, Bounds::default()).seen(), 2);
    assert_eq!(paged(b"a\nb", 1, Bounds::default()).seen(), 2);
    assert!(paged(b"text\x00more", 64, Bounds::default()).binary());
}

#[test]
fn a_page_holds_no_more_bytes_than_it_may_the_line_past_them_left_to_the_next() {
    let page = paged(
        b"0123456789\n0123456789\n",
        64,
        Bounds {
            bytes_max: 20,
            ..Bounds::default()
        },
    );
    assert_eq!(page.lines().len(), 1);
    assert!(!page.whole());
}

#[test]
fn a_stretch_of_bytes_that_is_no_utf8_is_one_replacement_as_a_decoder_reads_a_maximal_subpart() {
    // A character cut short is one U+FFFD; each byte that begins none is one of its own.
    let page = paged(b"a\xe2\x82b\xff\xfec", 64, Bounds::default());
    assert_eq!(page.lines()[0].text, "a\u{fffd}b\u{fffd}\u{fffd}c");
    // A character split across ranges is whole on the page.
    let page = paged("é€".as_bytes(), 1, Bounds::default());
    assert_eq!(page.lines()[0].text, "é€");
}
