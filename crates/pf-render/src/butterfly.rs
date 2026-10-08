//! The Butterfly effect, as xLights draws it (`ButterflyEffect::Render` in
//! `src-core/effects/ButterflyEffect.cpp` and `ispc/ButterflyFunctions.ispc`): a value worked out
//! from each cell's place on the target's grid and the time picks its color from a rainbow or the
//! palette. Patterns 1 to 5 are xLights' butterfly styles; 6 to 10 its plasma styles.
//!
//! With more than one chunk, the value is split into bands and every `skip`th band is left unlit.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use crate::raster::{cell_of, grid_size};
use pf_sequence::{ButterflyColors, ButterflyParams, Direction};

/// The rounded π xLights uses for this effect, kept so the patterns land where they do there.
#[allow(clippy::approx_constant)]
const XL_PI: f32 = 3.14159;

pub struct Butterfly {
    colors: Colors,
    rainbow: bool,
    pattern: u32,
    width: i32,
    height: i32,
    chunks: i32,
    skip: i32,
    /// How far the wings have shifted (patterns 1, 4, 5).
    offset: f32,
    /// The pulsing size (patterns 2 and 3).
    frame: i64,
    /// The plasma's time (patterns 6 to 10).
    plasma_time: f32,
    /// The plasma's moving centers and slant, worked out once a frame: sin(t/2), cos(t/3), and
    /// sin(t/5).
    plasma_turns: [f32; 3],
}

/// A fully saturated, full-brightness hue (0–1), as xLights' `h2rgb` makes it.
#[inline]
fn hue(h: f32) -> [f32; 3] {
    let h = h.clamp(0.0, 1.0);
    let sector = h * 6.0;
    let i = sector.floor() as i32;
    let f = sector - i as f32;
    match i {
        6 | 0 => [1.0, f, 0.0],
        1 => [1.0 - f, 1.0, 0.0],
        2 => [0.0, 1.0, f],
        3 => [0.0, 1.0 - f, 1.0],
        4 => [f, 0.0, 1.0],
        _ => [1.0, 0.0, 1.0 - f],
    }
}

/// `(x + 1) × 128`, as an 8-bit channel (xLights' plasma styles), 0–1.
#[inline]
fn channel(x: f32) -> f32 {
    ((x + 1.0) * 128.0).floor().clamp(0.0, 255.0) / 255.0
}

impl Butterfly {
    pub fn new(p: &ButterflyParams, time: &EffectTime, colors: Colors, canvas: Canvas) -> Self {
        let (width, height) = grid_size(canvas);
        let speed = (p.speed as i64).clamp(0, 100);
        let frames = time.frame() as i64;
        let state = frames
            .saturating_mul(speed)
            .saturating_mul(i64::from(time.frame_ms))
            / 50;
        let sign = if p.direction == Direction::Reverse {
            -1.0
        } else {
            1.0
        };
        // The shift only matters modulo 2π (it goes into a sine).
        let offset = (sign * state as f64 / 200.0).rem_euclid(std::f64::consts::TAU) as f32;
        let max_frame = i64::from(height) * 2;
        let frame = (i64::from(height).saturating_mul(state) / 200).rem_euclid(max_frame.max(1));
        let pattern = p.pattern.clamp(1, 10);
        let plasma_speed = if pattern == 10 {
            (101 - speed) * 3
        } else {
            (101 - speed) * 5
        };
        Self {
            colors,
            rainbow: p.colors == ButterflyColors::Rainbow,
            pattern,
            width,
            height,
            chunks: p.chunks.clamp(1, 10) as i32,
            skip: p.skip.clamp(2, 10) as i32,
            offset,
            frame,
            plasma_time: ((frames as f64 + 1.0) / plasma_speed as f64) as f32,
            plasma_turns: {
                let t = ((frames as f64 + 1.0) / plasma_speed as f64) as f32;
                [(t / 2.0).sin(), (t / 3.0).cos(), (t / 5.0).sin()]
            },
        }
    }

    /// The color for value `h` (0–1) on a lit band; `None` on a band left dark.
    #[inline]
    fn color(&self, h: f32) -> Option<Rgba> {
        if self.chunks > 1 && (h * self.chunks as f32) as i32 % self.skip == 0 {
            return None;
        }
        Some(Rgba::opaque(if self.rainbow {
            hue(h)
        } else {
            self.colors.ramp(h.clamp(0.0, 0.99999))
        }))
    }

    /// Patterns 6 to 10: a plasma.
    fn plasma(&self, x: f32, y: f32) -> Rgba {
        let time = self.plasma_time;
        let rx = x / self.width as f32 - 0.5;
        let ry = y / self.height as f32 - 0.5;
        let mut v = (rx * 10.0 + time).sin();
        let [sin_half, cos_third, sin_fifth] = self.plasma_turns;
        v += (10.0 * (rx * sin_half + ry * cos_third) + time).sin();
        let cx = rx + 0.5 * sin_fifth;
        let cy = ry + 0.5 * cos_third;
        v += (100.0 * (cx * cx + cy * cy) + 1.0 + time).sqrt().sin();
        v += (rx + time).sin();
        v += ((ry + time) / 2.0).sin();
        v += ((rx + ry + time) / 2.0).sin();
        v += ((rx * rx + ry * ry + 1.0).sqrt() + time).sin();
        v /= 2.0;
        let a = v * self.chunks as f32 * XL_PI;
        let third = 2.0 * XL_PI / 3.0;
        let rgb = match self.pattern {
            6 => [channel(a.sin()), channel(a.cos()), 0.0],
            7 => [1.0 / 255.0, channel(a.cos()), channel(a.sin())],
            8 => [
                channel(a.sin()),
                channel((a + third).sin()),
                channel((a + 2.0 * third).sin()),
            ],
            9 => [channel(a.sin()); 3],
            _ => {
                if self.colors.len() >= 2 {
                    self.colors.ramp(((a + third).sin() + 0.5).clamp(0.0, 0.99999))
                } else {
                    [0.0; 3]
                }
            }
        };
        Rgba::opaque(rgb)
    }
}

impl Shade for Butterfly {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let (xi, yi) = cell_of(px, self.width, self.height);
        let (mut x, mut y) = (xi as f32, yi as f32);
        let (w, h) = (self.width as f32, self.height as f32);
        let h_value = match self.pattern {
            1 => {
                let rsz = 2.0 * XL_PI / (h + w);
                let n = ((x * x - y * y) * (self.offset + (x + y) * rsz).sin()).abs();
                let d = x * x + y * y;
                if d > 0.001 { (n / d).clamp(0.0, 1.0) } else { 0.0 }
            }
            2 => {
                let f = if self.frame < i64::from(self.height) {
                    self.frame + 1
                } else {
                    i64::from(self.height) * 2 - self.frame
                } as f32;
                let x1 = (x - w / 2.0) / f;
                let y1 = (y - h / 2.0) / f;
                (x1 * x1 + y1 * y1).sqrt()
            }
            3 => {
                let max_frame = i64::from(self.height) * 2;
                let f = if self.frame < max_frame / 2 {
                    self.frame + 1
                } else {
                    max_frame - self.frame
                } as f32;
                let f = f * 0.1 + h / 60.0;
                ((x - w / 2.0) / f).sin() * ((y - h / 2.0) / f).cos()
            }
            4 => {
                let rsz = 2.0 * XL_PI / (h + w);
                let n = (x * x - y * y) * (self.offset + (x + y) * rsz).sin();
                let d = x * x + y * y;
                let v = if d > 0.001 { n / d } else { 0.0 };
                let fract = v - v.floor();
                if fract < 0.0 { 1.0 + fract } else { fract }
            }
            5 => {
                // xLights' fix for the colors of the pixels at (0, 1) and (1, 0).
                if xi == 0 && yi == 1 {
                    y += 1.0;
                }
                if xi == 1 && yi == 0 {
                    x += 1.0;
                }
                let n = ((x * x - y * y) * (self.offset + (x + y) * 2.0 * XL_PI / (h * w)).sin()).abs();
                let d = x * x + y * y;
                if d > 0.001 { n / d } else { 0.0 }
            }
            _ => return self.plasma(x, y),
        };
        self.color(h_value).unwrap_or(Rgba::CLEAR)
    }
}
