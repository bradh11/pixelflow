//! Reading a picture file's bytes into frames.
//!
//! Animated GIFs and WebPs come out as whole frames, each already composed onto the ones before
//! it the way its file says (the `image` crate's decoders do that), with the time each one
//! shows. Everything else is a single frame, turned the way a camera's note in the file says.
//!
//! Frames are shrunk as they're read, never kept at full size: a picture is only ever wanted at
//! about the size of the prop it's drawn on, and two hundred frames of a large GIF would
//! otherwise take hundreds of megabytes. Each halving is a `level`.

use super::resample::{Bitmap, clear_black, resize};
use image::{AnimationDecoder, ImageDecoder, ImageFormat};
use std::io::Cursor;

/// The widest and tallest picture read.
const MAX_SIDE: u32 = 16_384;
/// The most memory one picture's decoder may ask for.
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
    /// Shrunk by taking the nearest pixel instead of averaging.
    pub crisp: bool,
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
                resize(&frame, width, height, self.look.crisp)
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
                *frame = resize(frame, width, height, self.look.crisp);
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
            return animated(decoder.into_frames(), native);
        }
        ImageFormat::WebP => {
            let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).map_err(problem)?;
            if decoder.has_animation() {
                let native = decoder.dimensions();
                if native.0 > MAX_SIDE || native.1 > MAX_SIDE {
                    return Err("it's too large".to_string());
                }
                return animated(decoder.into_frames(), native);
            }
        }
        _ => {}
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits());
    let mut decoder = reader.into_decoder().map_err(problem)?;
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
