//! The Morph effect, as xLights draws it (`MorphEffect::Render` in `src-core/effects/MorphEffect.cpp`).
//!
//! Two lines are laid across the target's grid: side A from the start line's first end to the end
//! line's first end, side B between the second ends. Lines drawn from a point on A to the matching
//! point on B fill the space between, a tenth of a step at a time: the head (the first palette
//! colors) sweeps from the start line to the end line over the head time, and the tail (the rest)
//! follows it out. Repeats draw copies side by side, each starting later with stagger.
//!
//! Colors by palette size, as in xLights: one color for everything; two, a head and a tail; three,
//! a head and a tail going from the second to the third; four or more, a head going from the first
//! to the second and a tail through the rest.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Shade, accelerate};
use crate::geometry::Pixel;
use crate::raster::Raster;
use pf_sequence::MorphParams;

/// How far the head and tail move per line drawn (along side A, in cells).
const STEP: f64 = 0.1;

pub struct Morph {
    raster: Raster,
}

/// A position 0–100 as a cell on a side `base` cells long (`calcPosition`).
fn place(value: f32, base: i32) -> i32 {
    if value >= 100.0 {
        return base - 1;
    }
    let band = 100.0 / f64::from(base.max(1));
    (f64::from(value.max(0.0)) / band) as i32
}

/// The cells of Bresenham's line from one point to another, in order (`StoreLine`).
fn cells(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(i32, i32)> {
    let (dx, sx) = ((x1 - x0).abs(), if x0 < x1 { 1 } else { -1 });
    let (dy, sy) = ((y1 - y0).abs(), if y0 < y1 { 1 } else { -1 });
    let mut err = (if dx > dy { dx } else { -dy }) / 2;
    let (mut x, mut y) = (x0, y0);
    let mut out = Vec::with_capacity((dx.max(dy) + 1) as usize);
    loop {
        out.push((x, y));
        if x == x1 && y == y1 {
            return out;
        }
        let e2 = err;
        if e2 > -dx {
            err -= dy;
            x += sx;
        }
        if e2 < dy {
            err += dx;
            y += sy;
        }
    }
}

/// Palette colors `a` and `b` mixed, `ratio` of the way to `b` (`Get2ColorBlend`).
fn mix(colors: &Colors, a: usize, b: usize, ratio: f64) -> [f32; 3] {
    let (a, b) = (colors.get(a as u64), colors.get(b as u64));
    let r = ratio as f32;
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * r)
}

impl Morph {
    pub fn new(p: &MorphParams, time: &EffectTime, colors: Colors, canvas: Canvas) -> Self {
        let mut raster = Raster::new(canvas);
        let (width, height) = (raster.width, raster.height);
        let (x1a, y1a) = (place(p.start_x1, width), place(p.start_y1, height));
        let (x2a, y2a) = (place(p.end_x1, width), place(p.end_y1, height));
        let (x1b, y1b) = (place(p.start_x2, width), place(p.start_y2, height));
        let (x2b, y2b) = (place(p.end_x2, width), place(p.end_y2, height));

        let (dxa, dxb, dya, dyb) = (x2a - x1a, x2b - x1b, y2a - y1a, y2b - y1b);
        let direction = dxa + dxb + dya + dyb >= 0;
        let mut repeats = i64::from(p.repeats);
        let (mut repeat_x, mut repeat_y) = (0, 0);
        let (mut share, mut stagger_share) = (1.0, 0.0);
        if repeats > 0 || p.auto_repeat {
            let skip = p.repeat_spacing.max(1) as i32;
            // Copies go across when the line sweeps up and down, and the other way round.
            let across = dxa.abs() + dxb.abs() < dya.abs() + dyb.abs();
            let room = if across {
                repeat_x = skip;
                width
            } else {
                repeat_y = skip;
                height
            };
            if p.auto_repeat {
                let span = |a: f32, b: f32, cells: i32| (((a - b).abs() as i32) * cells / 100).max(1);
                let narrowest = span(p.start_x1, p.start_x2, width)
                    .min(span(p.start_y1, p.start_y2, height))
                    .min(span(p.end_x1, p.end_x2, width))
                    .min(span(p.end_y1, p.end_y2, height));
                repeats = i64::from(room / (narrowest + skip - 1).max(1) - 1);
            }
            let stagger = f64::from(p.stagger.abs()) / 200.0;
            share = 1.0 / (1.0 + stagger * repeats as f64);
            stagger_share = share * stagger;
        }

        let side_a = cells(x1a, y1a, x2a, y2a);
        let side_b = cells(x1b, y1b, x2b, y2b);
        let (long, short) = if side_a.len() > side_b.len() {
            (&side_a, &side_b)
        } else {
            (&side_b, &side_a)
        };
        let total = long.len() as f64;
        let head_time = f64::from(p.head_duration) / 100.0;
        let (start_length, end_length) = (f64::from(p.start_length), f64::from(p.end_length));

        let n = colors.len();
        let (head_from, head_to, tail_from, tail_to, tail_colors) = match n {
            1 => (0, 0, 0, 0, 2),
            2 => (0, 0, 1, 1, 2),
            3 => (0, 0, 1, 2, 2),
            _ => (0, 1, 2, 3, n - 2),
        };

        // xLights keeps these from one copy to the next when a copy doesn't set them.
        let mut head_front = total + 1.0;
        let mut head_back = total + 1.0;
        let mut head_color = colors.get(head_from as u64);
        let progress = accelerate(f64::from(time.t_norm), f64::from(p.acceleration));
        for repeat in 0..=repeats.max(-1) {
            let starts = if p.stagger >= 0.0 {
                stagger_share * repeat as f64
            } else {
                stagger_share * (repeats - repeat) as f64
            };
            let at = (progress - starts) / share;
            let (tail_front, tail_back, tail_length);
            if at < 0.0 {
                head_front = -1.0;
                head_back = -1.0;
                tail_front = -1.0;
                tail_back = -1.0;
                tail_length = 1.0;
                if p.head_at_start {
                    head_front = start_length;
                }
            } else if head_time > 0.0 {
                let head_at = at / head_time;
                let length = end_length * head_at + start_length * (1.0 - head_at);
                head_front = total * head_at + length * head_at * head_time;
                tail_length = total * (1.0 / head_time - 1.0);
                if p.head_at_start {
                    head_front += length * (1.0 - at);
                }
                head_back = head_front - length;
                tail_front = head_back - STEP;
                tail_back = tail_front - tail_length;
                head_color = mix(&colors, head_from, head_to, head_at.min(1.0));
            } else {
                tail_length = total;
                tail_front = total * 2.0 * at;
                tail_back = tail_front - tail_length;
            }
            let shift = (repeat_x * repeat as i32, repeat_y * repeat as i32);

            // The tail, from its front back.
            let mut lines = Vec::new();
            let mut i = tail_front.min(total - 1.0);
            while i >= tail_back && i >= 0.0 {
                let mut ratio = if tail_length > 0.0 {
                    (i - tail_back) / tail_length
                } else {
                    1.0
                };
                let (color, level) = if tail_colors > 2 {
                    let index = (tail_colors as f64 - 1.0) * (1.0 - ratio);
                    ratio = index.fract();
                    let from = index as usize + 2;
                    let level = if from + 1 == tail_colors + 1 {
                        1.0 - ratio
                    } else {
                        1.0
                    };
                    (mix(&colors, from, from + 1, ratio), level)
                } else {
                    let level = if ratio > 0.5 { 1.0 } else { ratio / 0.5 };
                    (mix(&colors, tail_to, tail_from, ratio), level)
                };
                lines.push((i, Rgba::with_alpha(color, level as f32)));
                i -= STEP;
            }
            draw(&mut raster, long, short, total, shift, direction, &lines);

            // The head, from its back forward.
            lines.clear();
            let mut i = head_back.max(0.0);
            while i <= head_front && i < total {
                lines.push((i, Rgba::opaque(head_color)));
                i += STEP;
            }
            draw(&mut raster, long, short, total, shift, direction, &lines);
        }
        Self { raster }
    }
}

/// Draws a line across from side A to side B at each place in `lines` (positions along the
/// longer side, with colors), in order. Neighbouring places often give the same line; only the
/// last of those is drawn, which leaves the same picture.
fn draw(
    raster: &mut Raster,
    long: &[(i32, i32)],
    short: &[(i32, i32)],
    total: f64,
    shift: (i32, i32),
    direction: bool,
    lines: &[(f64, Rgba)],
) {
    let ends = |i: f64| {
        let a = (i as usize).min(long.len() - 1);
        let along = if total == 0.0 { 0.0 } else { i / total };
        let b = ((short.len() as f64 * along) as usize).min(short.len() - 1);
        (long[a], short[b])
    };
    for (k, &(i, color)) in lines.iter().enumerate() {
        let here = ends(i);
        if lines.get(k + 1).is_some_and(|&(next, _)| ends(next) == here) {
            continue;
        }
        let ((xa, ya), (xb, yb)) = here;
        raster.line_without_gaps(
            xa + shift.0,
            ya + shift.1,
            xb + shift.0,
            yb + shift.1,
            color,
            direction,
        );
    }
}

impl Shade for Morph {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        self.raster.at(px)
    }
}
