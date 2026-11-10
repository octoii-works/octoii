//! Images as models and a person's view are given them: an image fitted within bounds — decoded memory-safely, as an
//! image may come from anywhere, from PNG, JPEG, GIF's first frame, WebP, BMP or QOI; kept as it is where it already
//! fits and its format is one the bounds keep; else resized and written again: as a PNG, losing nothing, where it fits
//! and is translucent or no larger than a JPEG of it — as a screenshot or a drawing is — else as a JPEG, as a
//! photograph is, its quality lowered till it fits. A computation on bytes alone.

use std::borrow::Cow;
use std::io::Cursor;

use fast_image_resize::images::Image as Pixels;
use fast_image_resize::{PixelType, Resizer};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder, ImageFormat, ImageReader, Limits};

/// The formats an image is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Gif,
    Webp,
    Bmp,
    Qoi,
}

impl Format {
    /// The media type of an image in this format.
    #[must_use]
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
            Self::Bmp => "image/bmp",
            Self::Qoi => "image/qoi",
        }
    }

    fn of(format: ImageFormat) -> Option<Self> {
        Some(match format {
            ImageFormat::Png => Self::Png,
            ImageFormat::Jpeg => Self::Jpeg,
            ImageFormat::Gif => Self::Gif,
            ImageFormat::WebP => Self::Webp,
            ImageFormat::Bmp => Self::Bmp,
            ImageFormat::Qoi => Self::Qoi,
            _ => return None,
        })
    }
}

/// The formats models and browsers read, which a fitting keeps unless its bounds say fewer.
pub const UNIVERSAL_FORMATS: &[Format] = &[Format::Png, Format::Jpeg, Format::Gif, Format::Webp];

/// The box an image is fitted to — the widest and the tallest it may be, its pixels and its bytes — and the formats it
/// may stay in as it is, of those models and browsers read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub width_max: u32,
    pub height_max: u32,
    pub pixels_max: u64,
    pub bytes_max: usize,
    pub formats: &'static [Format],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    /// This size scaled down, its shape kept, to fit `bounds`' box and pixels; as it is where it fits; never a side
    /// under one pixel. The side that bounds it most is met exactly, its scale a fraction; the pixels, bounding it more,
    /// by the square root of theirs.
    #[must_use]
    pub fn within(self, bounds: &Bounds) -> Self {
        let (width, height) = (u64::from(self.width), u64::from(self.height));
        let (mut over, mut under) = (1, 1);
        if u64::from(bounds.width_max) * under < over * width {
            (over, under) = (u64::from(bounds.width_max), width);
        }
        if u64::from(bounds.height_max) * under < over * height {
            (over, under) = (u64::from(bounds.height_max), height);
        }
        let (mut w, mut h) = ((width * over / under).max(1), (height * over / under).max(1));
        if w * h > bounds.pixels_max {
            #[expect(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "sides far below 2^52"
            )]
            {
                let scale = (bounds.pixels_max as f64 / (w * h) as f64).sqrt();
                (w, h) = (((w as f64 * scale) as u64).max(1), ((h as f64 * scale) as u64).max(1));
            }
            // A scale rounded up past an exact side is taken back a pixel at a time, off the longer side.
            while w * h > bounds.pixels_max && w.max(h) > 1 {
                if w > h {
                    w -= 1;
                } else {
                    h -= 1;
                }
            }
        }
        // Neither side grows past the one it came from.
        Self {
            width: u32::try_from(w).unwrap_or(self.width),
            height: u32::try_from(h).unwrap_or(self.height),
        }
    }
}

/// An image fitted: its format, its bytes — those it came as where it is kept — its size, and the size it came at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fitted<'a> {
    pub format: Format,
    pub bytes: Cow<'a, [u8]>,
    pub size: Size,
    pub original: Size,
}

/// What a fitting may come to: bytes that are no image of a format read; an image of one that does not decode; or one
/// of more pixels than are decoded, or that fits no bytes the bounds give.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("not-an-image")]
    NotAnImage,
    #[error("malformed")]
    Malformed,
    #[error("too-large")]
    TooLarge,
}

/// The most pixels an image may have to be decoded at all: past it, its memory is never taken.
const DECODE_PIXELS_MAX: u64 = 64_000_000;

/// The qualities a JPEG is written at, the first where it fits the bounds' bytes.
const JPEG_QUALITIES: [u8; 5] = [85, 70, 55, 40, 25];

/// The image in `bytes` within `bounds`, as the crate's comment says.
///
/// # Errors
/// As `Error` says.
pub fn fit<'a>(bytes: &'a [u8], bounds: &Bounds) -> Result<Fitted<'a>, Error> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| Error::NotAnImage)?;
    let format = reader.format().and_then(Format::of).ok_or(Error::NotAnImage)?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(DECODE_PIXELS_MAX * 4);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|error| match error {
        image::ImageError::Limits(_) => Error::TooLarge,
        _ => Error::Malformed,
    })?;
    let original = Size {
        width: decoded.width(),
        height: decoded.height(),
    };
    let size = original.within(bounds);
    let kept_as_is = size == original && bounds.formats.contains(&format) && UNIVERSAL_FORMATS.contains(&format);
    if kept_as_is && bytes.len() <= bounds.bytes_max {
        return Ok(Fitted {
            format,
            bytes: Cow::Borrowed(bytes),
            size,
            original,
        });
    }
    let rgba = decoded.into_rgba8().into_raw();
    let pixels = if size == original { rgba } else { resized(rgba, original, size) };
    let fitted = |format, bytes| Fitted {
        format,
        bytes: Cow::Owned(bytes),
        size,
        original,
    };
    let png = written_png(&pixels, size);
    if png.len() <= bounds.bytes_max {
        if translucent(&pixels) {
            return Ok(fitted(Format::Png, png));
        }
        let jpeg = written_jpeg(&pixels, size, JPEG_QUALITIES[0]);
        return Ok(if png.len() <= jpeg.len() {
            fitted(Format::Png, png)
        } else {
            fitted(Format::Jpeg, jpeg)
        });
    }
    JPEG_QUALITIES
        .iter()
        .map(|quality| written_jpeg(&pixels, size, *quality))
        .find(|jpeg| jpeg.len() <= bounds.bytes_max)
        .map(|jpeg| fitted(Format::Jpeg, jpeg))
        .ok_or(Error::TooLarge)
}

/// RGBA `pixels` of `from` resized to `to`.
fn resized(pixels: Vec<u8>, from: Size, to: Size) -> Vec<u8> {
    let source = Pixels::from_vec_u8(from.width, from.height, pixels, PixelType::U8x4).expect("RGBA of its size");
    let mut into = Pixels::new(to.width, to.height, PixelType::U8x4);
    Resizer::new().resize(&source, &mut into, None).expect("images of one pixel type");
    into.into_vec()
}

/// Whether any of RGBA `pixels` lets what is behind it through.
fn translucent(pixels: &[u8]) -> bool {
    pixels.as_chunks::<4>().0.iter().any(|[.., alpha]| *alpha != 255)
}

fn written_png(pixels: &[u8], size: Size) -> Vec<u8> {
    let mut out = Vec::new();
    PngEncoder::new_with_quality(&mut out, CompressionType::Default, FilterType::Adaptive)
        .write_image(pixels, size.width, size.height, ExtendedColorType::Rgba8)
        .expect("RGBA of its size writes");
    out
}

/// RGBA `pixels` written as a JPEG of `quality`, what lets through what is behind it laid on black, as JPEG keeps no
/// alpha.
fn written_jpeg(pixels: &[u8], size: Size, quality: u8) -> Vec<u8> {
    let rgb: Vec<u8> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|[red, green, blue, _]| [*red, *green, *blue])
        .collect();
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality)
        .write_image(&rgb, size.width, size.height, ExtendedColorType::Rgb8)
        .expect("RGB of its size writes");
    out
}
