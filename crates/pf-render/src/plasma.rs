//! The Plasma effect, as xLights draws it (`PlasmaEffect::Render` in `src-core/effects/PlasmaEffect.cpp`
//! and `ispc/PlasmaFunctions.ispc`): seven sine waves over the target's grid, some moving in
//! circles, summed into one value per cell that picks its color.
//!
//! xLights moves the waves a step a frame (faster with a higher speed), so the waves flow at a
//! pace set by the sequence's frame time, as they do there.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use crate::raster::{cell_of, grid_size};
use pf_sequence::{PlasmaColors, PlasmaParams};
use std::f32::consts::PI;

pub struct Plasma {
    colors: Colors,
    scheme: PlasmaColors,
    width: i32,
    height: i32,
    waves: Waves,
    /// The waves that depend only on the column, or only on the row, worked out once a frame.
    columns: Vec<f32>,
    rows: Vec<f32>,
}

/// Where cell `i` of `n` is across the grid (0–1).
#[inline]
fn across(i: i32, n: i32) -> f32 {
    if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 }
}

/// The waves at one moment (`plasmaCalc_vldpi`'s inputs).
#[derive(Debug, Clone, Copy)]
struct Waves {
    /// Frames so far, scaled by the speed.
    time: f32,
    sin_time_5: f32,
    cos_time_3: f32,
    sin_time_2: f32,
    /// How tightly the circular wave rings (xLights' Style × 50).
    twist: f32,
    density: f32,
}

impl Waves {
    /// The waves that depend only on the column at `rx`.
    fn column(&self, rx: f32) -> f32 {
        (rx * 10.0 + self.time).sin() + (rx + self.time).sin()
    }

    /// The wave that depends only on the row at `ry`.
    fn row(&self, ry: f32) -> f32 {
        ((ry + self.time) / 2.0).sin()
    }

    /// The summed waves at (rx, ry) (each 0–1 across the grid), times the line density and π,
    /// given the column's and row's own waves.
    #[inline]
    fn at(&self, rx: f32, ry: f32, column: f32, row: f32) -> f32 {
        let time = self.time;
        let cx = rx + 0.5 * self.sin_time_5;
        let cy = ry + 0.5 * self.cos_time_3;
        let mut v = column + row;
        v += (10.0 * (rx * self.sin_time_2 + ry * self.cos_time_3) + time).sin();
        v += (self.twist * (cx * cx + cy * cy) + time).sqrt().sin();
        v += ((rx + ry + time) / 2.0).sin();
        v += ((rx * rx + ry * ry).sqrt() + time).sin();
        v / 2.0 * self.density * PI
    }
}

impl Plasma {
    pub fn new(p: &PlasmaParams, time: &EffectTime, colors: Colors, canvas: Canvas) -> Self {
        let (width, height) = grid_size(canvas);
        let speed = (p.speed as i32).clamp(0, 100);
        let t = (time.frame() as f64 + 1.0) / f64::from((101 - speed) * 3);
        let waves = Waves {
            time: t as f32,
            sin_time_5: (t / 5.0).sin() as f32,
            cos_time_3: (t / 3.0).cos() as f32,
            sin_time_2: (t / 2.0).sin() as f32,
            twist: p.twist.clamp(1, 10) as f32 * 50.0,
            density: p.density.clamp(1, 10) as f32,
        };
        Self {
            colors,
            scheme: p.colors,
            width,
            height,
            columns: (0..width).map(|x| waves.column(across(x, width))).collect(),
            rows: (0..height).map(|y| waves.row(across(y, height))).collect(),
            waves,
        }
    }
}

/// `(sin(v) + 1) / 2`, as xLights turns it into a channel.
#[inline]
fn level(v: f32) -> f32 {
    (v.sin() + 1.0) / 2.0
}

impl Shade for Plasma {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let (x, y) = cell_of(px, self.width, self.height);
        let v = self.waves.at(
            across(x, self.width),
            across(y, self.height),
            self.columns[x as usize],
            self.rows[y as usize],
        );
        let third = 2.0 * PI / 3.0;
        let color = match self.scheme {
            PlasmaColors::Palette => self.colors.ramp(level(v + third).min(0.999_999)),
            PlasmaColors::RedGreen => [level(v), (v.cos() + 1.0) / 2.0, 0.0],
            PlasmaColors::BlueGreen => [1.0 / 255.0, (v.cos() + 1.0) / 2.0, level(v)],
            PlasmaColors::Rainbow => [level(v), level(v + third), level(v + 2.0 * third)],
            PlasmaColors::White => [level(v); 3],
        };
        Rgba::opaque(color)
    }
}
