//! A link drawn as a QR code in a terminal: two of its rows to a line, black on white, its quiet zone around it.

#[test]
fn a_link_is_drawn_two_rows_to_a_line_black_on_white_with_its_quiet_zone() {
    let drawn = octoii::qr::drawn("https://app.octoii.com/pair#key=k&relay=https://r.example&link=a.b").expect("a code");
    let lines: Vec<&str> = drawn.lines().collect();
    let width = lines[0]
        .trim_start_matches("\x1b[30;107m")
        .trim_end_matches("\x1b[0m")
        .chars()
        .count();
    assert!(width >= 21 + 8, "a code of 21 modules at least, and the quiet zone");
    assert_eq!(lines.len(), width.div_ceil(2), "two rows to a line");
    assert!(
        lines
            .iter()
            .all(|line| line.starts_with("\x1b[30;107m") && line.ends_with("\x1b[0m"))
    );
    assert!(lines[0].trim_start_matches("\x1b[30;107m").starts_with("    "), "light around it");
    assert!(octoii::qr::drawn(&"x".repeat(8000)).is_none(), "too long for a code");
}
