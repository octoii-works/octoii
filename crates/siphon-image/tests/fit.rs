//! An image fitted within bounds: kept as it is where it fits them, else resized and written again.

use std::borrow::Cow;

use siphon_image::{Bounds, Error, Format, READ_EVERYWHERE, Size, fit};

const MODELS: Bounds = Bounds {
    width_max: 1568,
    height_max: 1568,
    pixels_max: 1_150_000,
    bytes_max: 3_750_000,
    formats: READ_EVERYWHERE,
};

/// A `width` by `height` PNG of the pixels `paint` gives.
fn png(width: u32, height: u32, paint: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let image = image::RgbaImage::from_fn(width, height, |x, y| image::Rgba(paint(x, y)));
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).expect("written");
    out.into_inner()
}

fn grey(_: u32, _: u32) -> [u8; 4] {
    [128, 128, 128, 255]
}

/// Every pixel a colour of its own, as a photograph's grain is: what a PNG writes worst.
fn noise(x: u32, y: u32) -> [u8; 4] {
    let hash = (x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(2_246_822_519)).wrapping_mul(3_266_489_917);
    let [red, green, blue, _] = hash.to_le_bytes();
    [red, green, blue, 255]
}

#[test]
fn a_size_is_scaled_within_the_box_and_the_pixels_its_shape_kept() {
    let size = |width, height| Size { width, height }.within(&MODELS);
    assert_eq!(size(3000, 1000), Size { width: 1568, height: 522 });
    assert_eq!(size(2000, 2000), Size { width: 1072, height: 1072 });
    assert_eq!(size(800, 600), Size { width: 800, height: 600 });
    assert_eq!(size(100_000, 1), Size { width: 1568, height: 1 });
}

#[test]
fn an_image_is_kept_where_it_fits_its_bounds_and_its_format_is_one_they_keep() {
    let small = png(100, 50, grey);
    let kept = fit(&small, &MODELS).expect("fitted");
    assert!(matches!(kept.bytes, Cow::Borrowed(bytes) if bytes.as_ptr() == small.as_ptr()));
    assert_eq!(kept.format, Format::Png);
    assert_eq!(kept.format.media_type(), "image/png");
}

#[test]
fn past_its_box_an_image_is_resized_and_written_as_a_png_where_that_loses_least_else_as_a_jpeg() {
    let wide = png(3000, 1000, grey);
    let wide = fit(&wide, &MODELS).expect("fitted");
    assert_eq!(
        (wide.size, wide.original),
        (Size { width: 1568, height: 522 }, Size { width: 3000, height: 1000 })
    );
    assert_eq!(fit(&png(2000, 1000, noise), &MODELS).expect("fitted").format, Format::Jpeg);
    let translucent = png(2000, 400, |_, _| [128, 128, 128, 100]);
    let translucent = fit(&translucent, &MODELS).expect("fitted");
    assert_eq!(translucent.format, Format::Png);
    // Where the bounds keep no format, an image is written again whatever it came as.
    let none_kept = Bounds { formats: &[], ..MODELS };
    let drawing = png(256, 256, |x, y| if (x + y) % 2 == 0 { [0, 0, 0, 255] } else { [255; 4] });
    let drawing = fit(&drawing, &none_kept).expect("fitted");
    assert_eq!(drawing.format, Format::Png);
    assert!(matches!(drawing.bytes, Cow::Owned(_)));
}

#[test]
fn a_gif_stays_a_gif_but_where_the_bounds_keep_png_and_jpeg_alone_and_words_are_no_image() {
    let gif = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff\x21\xf9\x04\x01\x00\x00\x00\x00\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b";
    assert_eq!(fit(gif, &MODELS).expect("fitted").format, Format::Gif);
    let two = Bounds {
        formats: &[Format::Png, Format::Jpeg],
        ..MODELS
    };
    let again = fit(gif, &two).expect("fitted");
    assert_eq!(again.format, Format::Png);
    assert!(again.bytes.starts_with(b"\x89PNG"));
    assert_eq!(fit(b"plain words", &MODELS), Err(Error::NotAnImage));
    assert_eq!(fit(b"\x89PNG\r\n\x1a\nbroken", &MODELS), Err(Error::Malformed));
}
