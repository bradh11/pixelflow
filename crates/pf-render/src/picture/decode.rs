//! Reading a picture file's bytes into frames.
//!
//! Animated GIFs and WebPs come out as whole frames, each already composed onto the ones before
//! it the way its file says (the `image` crate's decoders do that), with the time each one
//! shows. Everything else is a single frame, turned the way a camera's note in the file says.
//!
//! Frames are shrunk as they're read, never kept at full size: a picture is only ever wanted at
//! about the size of the prop it's drawn on, and two hundred frames of a large GIF would
//! otherwise take hundreds of megabytes. Each halving is a `level`.

use super::resample::{Bitmap, Resize, clear_black, resize};
use image::{AnimationDecoder, ImageDecoder, ImageFormat};
use std::io::Cursor;

/// The widest and tallest picture read.
const MAX_SIDE: u32 = 16_384;
/// The most pixels in a picture read (8,192 × 8,192: more than any camera's photo).
const MAX_PIXELS: u64 = 64 * 1024 * 1024;
/// The most memory one picture's decoder may ask for, and the most its decoded pixels may take.
const MAX_DECODE_BYTES: u64 = 512 * 1024 * 1024;
/// The most frames of an animation kept (the rest are left out).
pub(crate) const MAX_FRAMES: usize = 2_000;
/// The most halvings.
pub(crate) const MAX_LEVEL: u8 = 12;
/// How long a frame with no time of its own shows, as browsers do it.
const DEFAULT_DELAY_MS: u32 = 100;

/// How frames are treated as they're read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct Look {
    /// How they're resized to the size they're drawn at (see [`Resize::on_reading`] for how
    /// they're shrunk here).
    pub resize: Resize,
    /// Pixels no brighter than this (red + green + blue) are made clear.
    pub black: Option<u16>,
}

/// A picture's frames at one size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Decoded {
    /// The picture's own size.
    pub native: (u32, u32),
    /// How many times the frames were halved.
    pub level: u8,
    pub frames: Vec<Bitmap>,
    /// How long each frame shows, in milliseconds.
    pub delays: Vec<u32>,
}

/// A frame's size after `level` halvings.
pub(crate) fn size_at((width, height): (u32, u32), level: u8) -> (u32, u32) {
    let halve = |n: u32| (((u64::from(n) + (1 << level) - 1) >> level) as u32).max(1);
    (halve(width), halve(height))
}

fn problem(error: image::ImageError) -> String {
    match error {
        image::ImageError::Limits(_) => "it's too large".to_string(),
        image::ImageError::Unsupported(_) => "it isn't a kind of picture PixelFlow reads".to_string(),
        _ => "it's damaged, or isn't a picture".to_string(),
    }
}

fn limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    limits
}

/// Refuses a picture whose own size says its pixels would take too much memory. A file's
/// header can claim any size in a few bytes, so this is asked before anything is decoded.
fn check_size((width, height): (u32, u32), decoded_bytes: u64) -> Result<(), String> {
    let pixels = u64::from(width) * u64::from(height);
    if width > MAX_SIDE || height > MAX_SIDE || pixels > MAX_PIXELS || decoded_bytes > MAX_DECODE_BYTES {
        return Err("it's too large".to_string());
    }
    Ok(())
}

fn bitmap(image: image::RgbaImage) -> Bitmap {
    let (width, height) = image.dimensions();
    Bitmap {
        width,
        height,
        pixels: image.pixels().map(|p| p.0).collect(),
    }
}

/// Frames being collected, shrunk as they arrive and again when together they pass `budget`.
struct Collected {
    native: (u32, u32),
    level: u8,
    look: Look,
    budget: usize,
    frames: Vec<Bitmap>,
    delays: Vec<u32>,
}

impl Collected {
    fn bytes(&self) -> usize {
        self.frames.iter().map(Bitmap::bytes).sum()
    }

    fn add(&mut self, mut frame: Bitmap, delay_ms: u32) {
        // Black is made clear at full size, so a shrunk picture's edges are part clear rather
        // than dark.
        if let Some(level) = self.look.black {
            clear_black(&mut frame, level);
        }
        let (width, height) = size_at(self.native, self.level);
        self.frames
            .push(if (frame.width, frame.height) == (width, height) {
                frame
            } else {
                resize(&frame, width, height, self.look.resize.on_reading())
            });
        self.delays.push(delay_ms);
        while self.bytes() > self.budget && self.level < MAX_LEVEL {
            let (width, height) = size_at(self.native, self.level + 1);
            if self
                .frames
                .first()
                .is_some_and(|f| (f.width, f.height) == (width, height))
            {
                break;
            }
            self.level += 1;
            for frame in &mut self.frames {
                *frame = resize(frame, width, height, self.look.resize.on_reading());
            }
        }
    }
}

/// The picture in `bytes`: its frames halved as many times as `level_for` says for its size
/// (more, to keep them within `budget` bytes together).
pub(crate) fn decode(
    bytes: &[u8],
    level_for: impl Fn((u32, u32)) -> u8,
    look: Look,
    budget: usize,
) -> Result<Decoded, String> {
    let format = image::guess_format(bytes).map_err(problem)?;
    let animated = |frames: image::Frames, native: (u32, u32)| -> Result<Decoded, String> {
        let mut out = Collected {
            native,
            level: level_for(native).min(MAX_LEVEL),
            look,
            budget,
            frames: Vec::new(),
            delays: Vec::new(),
        };
        for frame in frames.take(MAX_FRAMES) {
            // A file cut short still shows the frames it has.
            let frame = match frame {
                Ok(frame) => frame,
                Err(_) if !out.frames.is_empty() => break,
                Err(error) => return Err(problem(error)),
            };
            let (numerator, denominator) = frame.delay().numer_denom_ms();
            let ms = numerator.checked_div(denominator).unwrap_or(0);
            let ms = if ms <= 10 { DEFAULT_DELAY_MS } else { ms };
            out.add(bitmap(frame.into_buffer()), ms);
        }
        if out.frames.is_empty() {
            return Err("it has no frames".to_string());
        }
        Ok(Decoded {
            native,
            level: out.level,
            frames: out.frames,
            delays: out.delays,
        })
    };
    match format {
        ImageFormat::Gif => {
            let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).map_err(problem)?;
            decoder.set_limits(limits()).map_err(problem)?;
            let native = decoder.dimensions();
            check_size(native, decoder.total_bytes())?;
            return animated(decoder.into_frames(), native);
        }
        ImageFormat::WebP => {
            let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).map_err(problem)?;
            if decoder.has_animation() {
                // Its frames are composed on a canvas this size, which no limit of the decoder's
                // own covers.
                let native = decoder.dimensions();
                check_size(native, decoder.total_bytes())?;
                return animated(decoder.into_frames(), native);
            }
        }
        _ => {}
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits());
    let mut decoder = reader.into_decoder().map_err(problem)?;
    // The reader's limits cover what the decoder allocates for itself, not the buffer its pixels
    // are decoded into.
    check_size(decoder.dimensions(), decoder.total_bytes())?;
    let orientation = decoder.orientation().map_err(problem)?;
    let mut picture = image::DynamicImage::from_decoder(decoder).map_err(problem)?;
    picture.apply_orientation(orientation);
    let frame = bitmap(picture.into_rgba8());
    let native = (frame.width, frame.height);
    let mut out = Collected {
        native,
        level: level_for(native).min(MAX_LEVEL),
        look,
        budget,
        frames: Vec::new(),
        delays: Vec::new(),
    };
    out.add(frame, DEFAULT_DELAY_MS);
    Ok(Decoded {
        native,
        level: out.level,
        frames: out.frames,
        delays: out.delays,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The header of a 24-bit BMP that says it is `width` by `height`, and none of its pixels.
    fn bmp_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"BM".to_vec();
        bytes.extend(0u32.to_le_bytes()); // file size
        bytes.extend(0u32.to_le_bytes()); // reserved
        bytes.extend(54u32.to_le_bytes()); // where the pixels start
        bytes.extend(40u32.to_le_bytes()); // info header size
        bytes.extend(width.to_le_bytes());
        bytes.extend(height.to_le_bytes());
        bytes.extend(1u16.to_le_bytes()); // planes
        bytes.extend(24u16.to_le_bytes()); // bits per pixel
        bytes.extend([0u8; 24]); // no compression; sizes and palette left unsaid
        bytes
    }

    fn read(bytes: &[u8]) -> Result<Decoded, String> {
        decode(bytes, |_| 0, Look::default(), usize::MAX)
    }

    #[test]
    fn a_header_that_claims_a_huge_picture_is_refused_before_its_pixels_are_made_room_for() {
        // Within the side limit, but 268 million pixels: 800 MB to decode into.
        assert_eq!(read(&bmp_header(16_384, 16_384)).unwrap_err(), "it's too large");
        // Over the pixel limit by one row.
        assert_eq!(read(&bmp_header(8_192, 8_193)).unwrap_err(), "it's too large");
        assert_eq!(read(&bmp_header(16_385, 1)).unwrap_err(), "it's too large");
        // A size that's allowed gets as far as its (missing) pixels.
        assert_eq!(
            read(&bmp_header(64, 64)).unwrap_err(),
            "it's damaged, or isn't a picture"
        );
    }

    #[test]
    fn the_size_limits_are_on_sides_pixels_and_decoded_bytes() {
        assert!(check_size((8_192, 8_192), 8_192 * 8_192 * 4).is_ok());
        assert!(check_size((16_384, 4_096), 16_384 * 4_096 * 4).is_ok());
        // Sixteen bits a channel: the same pixels, twice the bytes.
        assert!(check_size((8_192, 8_192), 8_192 * 8_192 * 8).is_ok());
        assert!(check_size((8_192, 8_192), 8_192 * 8_192 * 8 + 1).is_err());
        assert!(check_size((16_384, 16_384), 0).is_err());
        assert!(check_size((16_385, 1), 0).is_err());
        assert!(check_size((1, 16_385), 0).is_err());
    }

    #[test]
    fn an_animation_on_a_huge_canvas_is_refused() {
        // A one-pixel GIF (35 bytes) whose screen, which every frame is composed on, is `width`
        // by `height`.
        let gif = |width: u16, height: u16| {
            let mut gif = b"GIF89a".to_vec();
            gif.extend(width.to_le_bytes());
            gif.extend(height.to_le_bytes());
            gif.extend([0x80, 0, 0, 0xFF, 0xFF, 0xFF, 0, 0, 0]); // two colors
            gif.extend([0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0]); // a 1 × 1 frame
            gif.extend([2, 2, 0x44, 1, 0, 0x3B]); // its pixel, and the end
            gif
        };
        let small = read(&gif(4, 2)).unwrap();
        assert_eq!((small.native, small.frames.len()), ((4, 2), 1));
        // 134 million pixels: 512 MB a frame, which the decoder's own limit lets through.
        assert_eq!(read(&gif(16_384, 8_192)).unwrap_err(), "it's too large");
    }
}
