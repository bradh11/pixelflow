//! Key frames of a sequence on one picture: a contact sheet. Each frame is drawn as the video
//! draws it (the 2D preview's front view, see [`crate::raster`]) at the sequence frame playback
//! would show then, with its music and pictures, and captioned underneath in the pixel font the
//! Text effect uses. Written as a PNG, whole or not at all.

use crate::VideoError;
use crate::raster::{Canvas, Look, Scene};
use pf_engine::{DraftRenderer, SequenceExport, preview_props_of};
use std::path::Path;

/// One frame on the sheet: when it is, and its caption (a line or more; the first is bright,
/// the rest dimmer).
#[derive(Debug, Clone, PartialEq)]
pub struct Still {
    pub time_ms: u64,
    pub caption: Vec<String>,
}

/// How the sheet is laid out.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetOptions {
    /// Each frame's size, in pixels.
    pub frame_width: u32,
    pub frame_height: u32,
    /// Frames side by side.
    pub columns: u32,
    /// The dots' size against the preview's (as for a video).
    pub pixel_size: f32,
    /// Lines across the top of the sheet.
    pub title: Vec<String>,
}

/// What was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SheetSummary {
    pub width: u32,
    pub height: u32,
    pub frames: usize,
}

const BACKGROUND: [u8; 3] = [24, 24, 28];
const BRIGHT: [u8; 3] = [235, 235, 235];
const DIM: [u8; 3] = [160, 160, 170];
/// Picture pixels to a dot of the font.
const DOT: u32 = 2;
/// A line of words, and a character, in picture pixels.
const LINE: u32 = 8 * DOT + 6;
const ADVANCE: u32 = 6 * DOT;
/// The space around and between frames.
const GAP: u32 = 14;

/// A picture being put together.
struct Sheet {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

impl Sheet {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgb: BACKGROUND.repeat((width * height) as usize),
        }
    }

    fn set(&mut self, x: u32, y: u32, color: [u8; 3]) {
        if x < self.width && y < self.height {
            let at = ((y * self.width + x) * 3) as usize;
            self.rgb[at..at + 3].copy_from_slice(&color);
        }
    }

    /// `text` from (x, y), cut short (with "...") where it would pass `most` pixels wide.
    fn words(&mut self, x: u32, y: u32, text: &str, most: u32, color: [u8; 3]) {
        let room = (most / ADVANCE) as usize;
        let chars: Vec<char> = text.chars().collect();
        let shown: Vec<char> = if chars.len() > room {
            chars[..room.saturating_sub(3)]
                .iter()
                .copied()
                .chain("...".chars())
                .collect()
        } else {
            chars
        };
        for (n, c) in shown.into_iter().enumerate() {
            let left = x + n as u32 * ADVANCE;
            for (column, bits) in pf_render::font_glyph(c).iter().enumerate() {
                for row in 0..8u32 {
                    if bits >> row & 1 == 1 {
                        for dy in 0..DOT {
                            for dx in 0..DOT {
                                self.set(left + column as u32 * DOT + dx, y + row * DOT + dy, color);
                            }
                        }
                    }
                }
            }
        }
    }

    fn frame(&mut self, x: u32, y: u32, canvas: &Canvas) {
        for row in 0..canvas.height {
            for column in 0..canvas.width {
                self.set(x + column, y + row, canvas.pixel(column, row));
            }
        }
    }
}

/// Draws `stills` of `job`'s sequence on one sheet, `options.columns` across, and writes it to
/// `path` as a PNG.
pub fn contact_sheet(
    job: &SequenceExport,
    stills: &[Still],
    options: &SheetOptions,
    path: &Path,
) -> Result<SheetSummary, VideoError> {
    let look = Look {
        width: options.frame_width.max(16),
        height: options.frame_height.max(16),
        pixel_size: options.pixel_size,
        glow: 0.0,
        hide_unlit: false,
        boost: 0,
    };
    let scene = Scene::new(&preview_props_of(job.show()), None, &look).ok_or(VideoError::NothingToShow)?;
    let (fw, fh) = (scene.width(), scene.height());
    let columns = options.columns.clamp(1, stills.len().max(1) as u32);
    let rows = (stills.len() as u32).div_ceil(columns).max(1);
    let lines = stills.iter().map(|s| s.caption.len()).max().unwrap_or(0) as u32;
    let tile = fh + 6 + lines * LINE;
    let top = GAP + options.title.len() as u32 * LINE + if options.title.is_empty() { 0 } else { GAP };
    let width = GAP + columns * (fw + GAP);
    let height = top + rows * (tile + GAP);
    let mut sheet = Sheet::new(width, height);
    for (n, line) in options.title.iter().enumerate() {
        let color = if n == 0 { BRIGHT } else { DIM };
        sheet.words(GAP, GAP + n as u32 * LINE, line, width - 2 * GAP, color);
    }
    let sequence = job.sequence();
    let frame_ms = u64::from(sequence.frame_ms.max(1));
    let mut renderer = DraftRenderer::new(job.show());
    renderer.set_audio(job.audio_source());
    renderer.set_pictures(job.pictures());
    let mut canvas = Canvas::new(fw, fh);
    for (n, still) in stills.iter().enumerate() {
        let (column, row) = (n as u32 % columns, n as u32 / columns);
        let (x, y) = (GAP + column * (fw + GAP), top + row * (tile + GAP));
        // The frame playback shows at that moment.
        let at = still.time_ms.min(sequence.duration_ms.saturating_sub(1)) / frame_ms * frame_ms;
        scene.draw(&renderer.frame(sequence, at), &mut canvas);
        sheet.frame(x, y, &canvas);
        for (i, line) in still.caption.iter().enumerate() {
            let color = if i == 0 { BRIGHT } else { DIM };
            sheet.words(x, y + fh + 6 + i as u32 * LINE, line, fw, color);
        }
    }
    let mut png = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut png),
        &sheet.rgb,
        width,
        height,
        image::ExtendedColorType::Rgb8,
    )
    .map_err(|e| VideoError::Encode(e.to_string()))?;
    pf_engine::write_atomic(path, &png).map_err(|e| VideoError::Write {
        path: path.display().to_string(),
        source: std::io::Error::other(e.to_string()),
    })?;
    Ok(SheetSummary {
        width,
        height,
        frames: stills.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_engine::{Edit, Engine};
    use pf_model::{Generator, Prop, Rgb, ShapeSource};
    use pf_sequence::{Effect, EffectKind, Row, Target};

    #[test]
    fn a_sheet_has_a_captioned_frame_for_each_still_drawn_at_its_time() {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = Engine::new(dir.path());
        let prop = Prop::new(
            "Line",
            ShapeSource::Generator(Generator::Line {
                nodes: 10,
                length: 2.0,
            }),
        );
        engine.apply(vec![Edit::AddProp { prop: prop.clone() }]).unwrap();
        let mut row = Row::new(Target::Prop(prop.id));
        row.layers[0]
            .effects
            .push(Effect::new(EffectKind::On, 1000, 2000).with_palette(vec![Rgb::new(255, 0, 0)]));
        engine
            .new_sequence_doc_with_rows("Song", 3000, None, vec![row])
            .unwrap();
        let job = engine.sequence_export().unwrap();
        let path = dir.path().join("sheet.png");
        let stills = [
            Still {
                time_ms: 500,
                caption: vec!["Dark".into()],
            },
            Still {
                time_ms: 1500,
                caption: vec!["Red".into(), "on".into()],
            },
        ];
        let options = SheetOptions {
            frame_width: 160,
            frame_height: 90,
            columns: 2,
            pixel_size: 4.0,
            title: vec!["A test".into()],
        };
        let summary = contact_sheet(&job, &stills, &options, &path).unwrap();
        assert_eq!(summary.frames, 2);
        assert_eq!(summary.width, GAP + 2 * (160 + GAP));
        let picture = image::open(&path).unwrap().to_rgb8();
        assert_eq!(picture.dimensions(), (summary.width, summary.height));
        // The second frame has red dots, the first none.
        let top = GAP + LINE + GAP;
        let red_in = |left: u32| {
            (0..90).any(|y| {
                (0..160).any(|x| {
                    let [r, g, _] = picture.get_pixel(left + x, top + y).0;
                    r > 150 && g < 60
                })
            })
        };
        assert!(!red_in(GAP));
        assert!(red_in(GAP + 160 + GAP));
        // The captions are drawn: bright dots under the first frame.
        assert!((0..LINE).any(|y| (0..160).any(|x| picture.get_pixel(GAP + x, top + 96 + y).0 == BRIGHT)));
    }
}
