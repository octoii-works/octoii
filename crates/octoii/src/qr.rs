//! A QR code drawn in a terminal: a link a phone's camera reads off the screen. Each character holds two of the code's
//! rows, in black on white whatever the terminal's own colours are, as a camera reads a dark code on a light ground,
//! with the quiet zone of four modules the standard asks around it.

use qrcode::{Color, QrCode};

/// The modules of light around a code.
const QUIET: usize = 4;

/// `text` as a QR code drawn in a terminal; none where it is too long for one.
#[must_use]
pub fn drawn(text: &str) -> Option<String> {
    let code = QrCode::new(text.as_bytes()).ok()?;
    let width = code.width();
    let colors = code.to_colors();
    let dark = |row: usize, column: usize| {
        let (Some(row), Some(column)) = (row.checked_sub(QUIET), column.checked_sub(QUIET)) else {
            return false;
        };
        row < width && column < width && colors[row * width + column] == Color::Dark
    };
    let across = width + 2 * QUIET;
    let mut out = String::new();
    for row in (0..across).step_by(2) {
        // Black on bright white, whatever the terminal's colours.
        out.push_str("\x1b[30;107m");
        for column in 0..across {
            out.push(match (dark(row, column), dark(row + 1, column)) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            });
        }
        out.push_str("\x1b[0m\n");
    }
    Some(out)
}
