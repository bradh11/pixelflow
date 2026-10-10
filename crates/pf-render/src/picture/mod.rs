//! Picture: the user's own artwork on a prop, a still picture or an animated GIF. PixelFlow's
//! own, with xLights' Pictures effect's sizing and scrolling (`PicturesEffect.cpp`) where they
//! overlap.
//!
//! - **The file:** the effect names a file; whoever made the renderer's [`Pictures`] reads it
//!   (see `library.rs`). Until it's there, and when it can't be read, the effect draws nothing.
//! - **The size:** fitted inside the target's grid, filling it, stretched to it, or a picture
//!   pixel per cell, then scaled and turned; resized by averaging, so a large picture keeps its
//!   thin lines on a small matrix (see `resample.rs`).
//! - **The frame:** the one showing at the effect's time, by the animation's own frame times: a
//!   pure function of the time, so seeking and export show the same frame.
//! - **The place:** in the middle (where xLights puts it, to the cell), moved by the offsets, and
//!   scrolled, zoomed, or panned by the movement.
//!
//! Clear parts of the picture stay clear, so the layers below show through them.

mod decode;
mod library;
mod resample;

pub use library::{Pictures, ReadPicture};

use crate::audio::RenderContext;
use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use crate::raster::{cell_of, grid_size};
use decode::Look;
use library::{Drawn, Need, Want};
use pf_sequence::{PictureFit, PictureMovement, PictureParams, PictureTiming, PictureTurn};
use resample::Bitmap;
use std::sync::Arc;

/// The most cells a picture is drawn at (a larger one is drawn smaller).
const MAX_DRAWN_CELLS: f64 = (1u32 << 22) as f64;

/// The frame of an animation showing `elapsed_ms` into an effect `length_ms` long. `delays` is
/// how long each frame shows; `start` is the frame it begins on (the first is 1).
pub(crate) fn frame_at(
    delays: &[u32],
    timing: PictureTiming,
    speed: f32,
    start: u32,
    elapsed_ms: u64,
    length_ms: u64,
) -> usize {
    if delays.len() < 2 {
        return 0;
    }
    let total: f64 = delays.iter().map(|&d| f64::from(d.max(1))).sum();
    let begin = (start.max(1) as usize - 1).min(delays.len() - 1);
    let skipped: f64 = delays[..begin].iter().map(|&d| f64::from(d.max(1))).sum();
    let speed = f64::from(if speed.is_finite() { speed.max(0.0) } else { 1.0 });
    let t = match timing {
        PictureTiming::Loop => (elapsed_ms as f64 * speed + skipped) % total,
        PictureTiming::Once => (elapsed_ms as f64 * speed + skipped).min(total - 0.5),
        PictureTiming::Stretch => {
            (elapsed_ms as f64 * speed * total / length_ms.max(1) as f64 + skipped) % total
        }
    };
    let mut end = 0.0;
    for (i, &delay) in delays.iter().enumerate() {
        end += f64::from(delay.max(1));
        if t < end {
            return i;
        }
    }
    delays.len() - 1
}

/// A picture `native` cells in size on a grid: how large it's drawn.
fn fitted(fit: PictureFit, native: (f64, f64), grid: (f64, f64)) -> (f64, f64) {
    let (across, down) = (grid.0 / native.0, grid.1 / native.1);
    match fit {
        PictureFit::Fit => (native.0 * across.min(down), native.1 * across.min(down)),
        PictureFit::Fill => (native.0 * across.max(down), native.1 * across.max(down)),
        PictureFit::Stretch => grid,
        PictureFit::Actual => native,
    }
}

/// What the effect wants of its picture on a target's grid (see `library.rs`), and how the
/// picture is turned: quarter turns to the right, and whether that lays it on its side.
pub(crate) fn wanted(p: &PictureParams, canvas: Canvas) -> (Want, u8, bool) {
    let (width, height) = grid_size(canvas);
    let (quarters, sideways) = match p.turn {
        PictureTurn::None => (0, false),
        PictureTurn::Right => (1, true),
        PictureTurn::Half => (2, false),
        PictureTurn::Left => (3, true),
    };
    // The grid as the picture sees it before it's turned.
    let (columns, rows) = if sideways {
        (height, width)
    } else {
        (width, height)
    };
    let scale = f64::from(p.scale.max(1.0)) / 100.0;
    let want = Want {
        columns: columns as u32,
        rows: rows as u32,
        need: match p.fit {
            PictureFit::Fit => Need::Fit,
            PictureFit::Fill | PictureFit::Stretch => Need::Fill,
            PictureFit::Actual => Need::Full,
        },
        doublings: scale.max(1.0).log2().ceil() as u8,
        look: Look {
            crisp: p.crisp,
            // Of full white, as red + green + blue.
            black: p
                .black_transparent
                .then(|| (f64::from(p.black_level.clamp(0.0, 100.0)) * 7.65).round() as u16),
        },
    };
    (want, quarters, sideways)
}

pub struct Picture {
    /// The frame at the size it's drawn; none when there's nothing to draw.
    frame: Option<Arc<Bitmap>>,
    width: i32,
    height: i32,
    /// The frame's left edge in the grid, and its top edge counted down from the grid's top.
    left: i32,
    top: i32,
    /// The cells after which the picture comes round again, across and down (0: it doesn't).
    wrap: (i32, i32),
    tint: [f32; 3],
}

impl Picture {
    const NOTHING: Picture = Picture {
        frame: None,
        width: 1,
        height: 1,
        left: 0,
        top: 0,
        wrap: (0, 0),
        tint: [1.0; 3],
    };

    pub fn new(
        p: &PictureParams,
        time: &EffectTime,
        colors: Colors,
        canvas: Canvas,
        cx: &RenderContext,
    ) -> Self {
        let Some(pictures) = cx.pictures else {
            return Self::NOTHING;
        };
        let (width, height) = grid_size(canvas);
        let (want, quarters, sideways) = wanted(p, canvas);
        let scale = f64::from(p.scale.max(1.0)) / 100.0;
        let Some(frames) = pictures.frames(&p.file, want) else {
            return Self::NOTHING;
        };
        let native = if sideways {
            (f64::from(frames.native.1), f64::from(frames.native.0))
        } else {
            (f64::from(frames.native.0), f64::from(frames.native.1))
        };
        if native.0 < 1.0 || native.1 < 1.0 {
            return Self::NOTHING;
        }

        // How large it's drawn. Zooming, it grows from (or shrinks to) nothing.
        let zoom = match p.movement {
            PictureMovement::ZoomIn => time.cycle_position(1.0),
            PictureMovement::ZoomOut => 1.0 - time.cycle_position(1.0),
            _ => 1.0,
        };
        let (mut w, mut h) = fitted(p.fit, native, (f64::from(width), f64::from(height)));
        w *= scale * zoom;
        h *= scale * zoom;
        if w * h > MAX_DRAWN_CELLS {
            let shrink = (MAX_DRAWN_CELLS / (w * h)).sqrt();
            w *= shrink;
            h *= shrink;
        }
        let zooming = zoom < 1.0;
        let (w, h) = (w.round() as i32, h.round() as i32);
        if zooming && (w < 1 || h < 1) {
            return Self::NOTHING;
        }
        let (w, h) = (w.max(1), h.max(1));

        let frame = frame_at(
            &frames.delays,
            p.timing,
            p.play_speed,
            p.start_frame,
            time.elapsed_ms,
            time.length_ms,
        );
        let at = Drawn {
            frame: frame as u32,
            width: w as u32,
            height: h as u32,
            quarters,
            crisp: p.crisp,
        };
        let Some(bitmap) = pictures.drawn(&frames, at) else {
            return Self::NOTHING;
        };

        // Where it sits: in the middle, as xLights centers it (to the cell), then offset.
        let offset = |value: f32, cells: i32| -> i32 {
            if p.pixel_offsets {
                value as i32
            } else {
                (f64::from(value) * f64::from(cells) / 100.0) as i32
            }
        };
        let (dx, dy) = (offset(p.x_offset, width), offset(p.y_offset, height));
        let rest_left = (width - w) / 2 + dx;
        let rest_top = height - (height + h) / 2 - dy;
        // How far a scrolling picture has gone: each trip takes it from just off one side to
        // just off the other. Wrapping, it keeps going and comes round.
        let travelled = |trip: i32| -> i64 {
            let trips = time.seconds() * f64::from(p.move_speed.max(0.0));
            let trips = if p.wrap { trips } else { trips.fract() };
            (trips * f64::from(trip)) as i64
        };
        let round = |cells: i64, period: i32| -> i32 {
            if period > 0 {
                cells.rem_euclid(i64::from(period)) as i32
            } else {
                cells.clamp(-(1 << 30), 1 << 30) as i32
            }
        };
        let across = if p.wrap { w.max(width) } else { 0 };
        let down = if p.wrap { h.max(height) } else { 0 };
        let (left, top, wrap) = match p.movement {
            PictureMovement::Left => {
                let x = i64::from(width) - travelled(w + width) + i64::from(dx);
                (round(x, across), rest_top, (across, 0))
            }
            PictureMovement::Right => {
                let x = travelled(w + width) - i64::from(w) + i64::from(dx);
                (round(x, across), rest_top, (across, 0))
            }
            PictureMovement::Up => {
                let y = i64::from(height) - travelled(h + height) - i64::from(dy);
                (rest_left, round(y, down), (0, down))
            }
            PictureMovement::Down => {
                let y = travelled(h + height) - i64::from(h) - i64::from(dy);
                (rest_left, round(y, down), (0, down))
            }
            PictureMovement::Pan => {
                // Along the side it overhangs most, from one end to the other over the effect.
                let through = time.cycle_position(1.0);
                let (over_x, over_y) = (w - width, h - height);
                if over_x > 0 && over_x >= over_y {
                    (
                        dx - (f64::from(over_x) * through).round() as i32,
                        rest_top,
                        (0, 0),
                    )
                } else if over_y > 0 {
                    (
                        rest_left,
                        -(f64::from(over_y) * through).round() as i32 - dy,
                        (0, 0),
                    )
                } else {
                    (rest_left, rest_top, (0, 0))
                }
            }
            // A still picture pushed off one side comes back on the other, when it wraps.
            PictureMovement::None => (rest_left, rest_top, (across, 0)),
            PictureMovement::ZoomIn | PictureMovement::ZoomOut => (rest_left, rest_top, (0, 0)),
        };
        Self {
            frame: Some(bitmap),
            width,
            height,
            left,
            top,
            wrap,
            tint: if p.tint { colors.get(0) } else { [1.0; 3] },
        }
    }
}

impl Shade for Picture {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let Some(frame) = &self.frame else {
            return Rgba::CLEAR;
        };
        let (x, y) = cell_of(px, self.width, self.height);
        // Rows count down from the top, as pictures are stored.
        let (mut col, mut row) = (x - self.left, self.height - 1 - y - self.top);
        if self.wrap.0 > 0 {
            col = col.rem_euclid(self.wrap.0);
        }
        if self.wrap.1 > 0 {
            row = row.rem_euclid(self.wrap.1);
        }
        if col < 0 || row < 0 || col >= frame.width as i32 || row >= frame.height as i32 {
            return Rgba::CLEAR;
        }
        let [r, g, b, a] = frame.get(col as u32, row as u32);
        if a == 0 {
            return Rgba::CLEAR;
        }
        let level = |v: u8, tint: f32| f32::from(v) / 255.0 * tint;
        Rgba::new(
            level(r, self.tint[0]),
            level(g, self.tint[1]),
            level(b, self.tint[2]),
            f32::from(a) / 255.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_looping_animation_shows_each_frame_for_its_own_time() {
        // Frames of 100, 300, and 50 ms: 450 ms a pass.
        let delays = [100, 300, 50];
        let at = |ms| frame_at(&delays, PictureTiming::Loop, 1.0, 1, ms, 10_000);
        assert_eq!(
            [0, 99, 100, 399, 400, 449].map(at),
            [0, 0, 1, 1, 2, 2],
            "each frame for as long as it says"
        );
        assert_eq!(
            [450, 549, 550, 850, 900].map(at),
            [0, 0, 1, 2, 0],
            "and round again"
        );
        // Twice as fast, and half as fast.
        let fast = |ms| frame_at(&delays, PictureTiming::Loop, 2.0, 1, ms, 10_000);
        assert_eq!([0, 49, 50, 199, 200, 225].map(fast), [0, 0, 1, 1, 2, 0]);
        let slow = |ms| frame_at(&delays, PictureTiming::Loop, 0.5, 1, ms, 10_000);
        assert_eq!([199, 200, 799, 800].map(slow), [0, 1, 1, 2]);
        // Starting on the second frame: its whole time first.
        let later = |ms| frame_at(&delays, PictureTiming::Loop, 1.0, 2, ms, 10_000);
        assert_eq!(
            [0, 299, 300, 349, 350, 449, 450].map(later),
            [1, 1, 2, 2, 0, 0, 1]
        );
        // A start past the end is the last frame; a still picture has one frame whatever's asked.
        assert_eq!(frame_at(&delays, PictureTiming::Loop, 1.0, 99, 0, 10_000), 2);
        assert_eq!(frame_at(&[100], PictureTiming::Loop, 1.0, 5, 12_345, 10_000), 0);
        assert_eq!(frame_at(&[], PictureTiming::Once, 1.0, 1, 12_345, 10_000), 0);
    }

    #[test]
    fn played_once_it_holds_its_last_frame() {
        let delays = [100, 300, 50];
        let at = |ms| frame_at(&delays, PictureTiming::Once, 1.0, 1, ms, 10_000);
        assert_eq!([0, 100, 399, 400, 450, 451, 9_999].map(at), [0, 1, 1, 2, 2, 2, 2]);
        let fast = |ms| frame_at(&delays, PictureTiming::Once, 2.0, 1, ms, 10_000);
        assert_eq!([49, 50, 200, 5_000].map(fast), [0, 1, 2, 2]);
    }

    #[test]
    fn stretched_one_pass_takes_the_whole_effect_whatever_its_length() {
        // Frames keep their share of the pass: 100, 300, and 50 of 450.
        let delays = [100, 300, 50];
        let at = |ms, length| frame_at(&delays, PictureTiming::Stretch, 1.0, 1, ms, length);
        assert_eq!(
            [0, 999, 1000, 3999, 4000, 4499].map(|ms| at(ms, 4500)),
            [0, 0, 1, 1, 2, 2]
        );
        assert_eq!([0, 19, 20, 79, 80, 89].map(|ms| at(ms, 90)), [0, 0, 1, 1, 2, 2]);
        // Twice over the effect.
        let twice = |ms| frame_at(&delays, PictureTiming::Stretch, 2.0, 1, ms, 9000);
        assert_eq!(
            [0, 999, 1000, 3999, 4000, 4499, 4500, 5500, 8999].map(twice),
            [0, 0, 1, 1, 2, 2, 0, 1, 2]
        );
        // The same time always gives the same frame.
        assert_eq!(at(1234, 4500), at(1234, 4500));
    }

    #[test]
    fn pictures_are_sized_to_the_grid_each_way() {
        let grid = (64.0, 32.0);
        // A square picture: fitted it's as tall as the grid, filling it's as wide.
        assert_eq!(fitted(PictureFit::Fit, (100.0, 100.0), grid), (32.0, 32.0));
        assert_eq!(fitted(PictureFit::Fill, (100.0, 100.0), grid), (64.0, 64.0));
        assert_eq!(fitted(PictureFit::Stretch, (100.0, 100.0), grid), (64.0, 32.0));
        assert_eq!(fitted(PictureFit::Actual, (100.0, 100.0), grid), (100.0, 100.0));
        // A small one is enlarged to fit; a banner fits across.
        assert_eq!(fitted(PictureFit::Fit, (4.0, 4.0), grid), (32.0, 32.0));
        assert_eq!(fitted(PictureFit::Fit, (640.0, 32.0), grid), (64.0, 3.2));
        assert_eq!(fitted(PictureFit::Fill, (640.0, 32.0), grid), (640.0, 32.0));
    }
}
