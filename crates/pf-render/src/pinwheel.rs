//! The Pinwheel effect, as xLights draws it (`PinwheelEffect` in `src-core/effects/PinwheelEffect.cpp`
//! and `ispc/PinwheelFunctions.ispc`): arms of the palette colors turning around a center point.
//!
//! The smooth style (xLights' new render method) lights every cell whose angle around the center
//! falls within an arm, over a single line down each arm so thin arms never vanish. The spokes
//! style (the old method) draws each arm as a bundle of lines, one a degree. Both turn the arms
//! `speed / 50` degrees a millisecond, counted in whole frames as xLights does.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use crate::raster::{Raster, cell_of};
use pf_sequence::{PinwheelParams, PinwheelShading, PinwheelStyle};

/// The rounded π xLights turns angles into degrees with, kept so the arms land where they do there.
#[allow(clippy::approx_constant)]
const XL_PI: f32 = 3.14159;
/// xLights' Speed slider maximum, which its speed is divided by.
const SPEED_MAX: f64 = 50.0;

pub struct Pinwheel {
    /// The lines drawn first (the smooth style's center lines; all of the spokes style).
    raster: Raster,
    /// The smooth style's arms, worked out cell by cell; `None` for spokes.
    arms: Option<Arms>,
}

/// What the smooth style needs for each cell.
struct Arms {
    /// Arm `k`'s color.
    colors: Vec<[f32; 3]>,
    shading: PinwheelShading,
    counterclockwise: bool,
    /// The center, in cells from the grid's middle.
    xc_adj: i32,
    yc_adj: i32,
    max_radius: i32,
    degrees_per_arm: i32,
    twist: f32,
    /// The lit part of each arm's slice, in degrees.
    tmax: f32,
    pos: f32,
    offset: f32,
}

/// How much of a shaded arm shows at `round` across it (0 at one edge, 1 at the other), for the
/// spokes style (`adjustColor`).
fn spoke_level(shading: PinwheelShading, round: f32) -> f32 {
    match shading {
        PinwheelShading::Flat => 1.0,
        PinwheelShading::Raised => 1.0 - (round - 0.5).abs() / 0.5,
        PinwheelShading::Sunken => (round - 0.5).abs() / 0.5,
        PinwheelShading::Sweep => round,
    }
}

impl Pinwheel {
    pub fn new(p: &PinwheelParams, time: &EffectTime, colors: Colors, canvas: Canvas) -> Self {
        let mut raster = Raster::new(canvas);
        let (width, height) = (raster.width, raster.height);
        // Settings xLights reads as whole numbers.
        let twist = p.twist as i32;
        let thickness = p.thickness as i32;
        let arm_size = p.arm_size as i32;
        let speed = p.speed as i32;
        let offset = p.offset as i32;
        let (xc_adj, yc_adj) = (p.center_x as i32, p.center_y as i32);
        let arms = p.arms.clamp(1, 20) as i32;
        let degrees_per_arm = 360 / arms;
        let armsize = f64::from(arm_size) / 100.0;
        // Turned so far: whole frames' worth.
        let elapsed = time.frame() * u64::from(time.frame_ms);
        let pos = (elapsed as f64 * f64::from(speed) / SPEED_MAX).min(1e12);

        if p.style == PinwheelStyle::Spokes {
            let xc = width.max(height) / 2;
            let max_radius = (f64::from(xc) * armsize) as i32;
            let tmax = f64::from(thickness) / 100.0 * f64::from(degrees_per_arm) / 2.0;
            let (cx, cy) = (
                width / 2 + xc_adj * (width / 2) / 100,
                height / 2 + yc_adj * (height / 2) / 100,
            );
            let reach = reach((cx, cy), width, height);
            let circle = Circle::new();
            for a in 1..=arms {
                let color = colors.get(a as u64);
                let spin = if p.counterclockwise { pos } else { -pos };
                let base = (f64::from((a - 1) * degrees_per_arm) + spin + f64::from(offset)) as i64;
                let mut t = base as f64 - tmax;
                while t <= base as f64 + tmax {
                    let round = ((t - base as f64 + tmax) / (2.0 * tmax + 1.0)) as f32;
                    let level = spoke_level(p.shading, round);
                    spoke(
                        &mut raster,
                        &circle,
                        t as i64,
                        max_radius,
                        reach,
                        twist,
                        (cx, cy),
                        Rgba::with_alpha(color, level.clamp(0.0, 1.0)),
                    );
                    t += 1.0;
                }
            }
            return Self { raster, arms: None };
        }

        let xc = (f64::from(width).hypot(f64::from(height)) / 2.0).ceil() as i32;
        let xc_adj = xc_adj * width / 200;
        let yc_adj = yc_adj * height / 200;
        let max_radius = (f64::from(xc) * armsize) as i32;
        let tmax = f64::from(thickness.max(1)) / 100.0 * f64::from(degrees_per_arm);
        let arm_colors: Vec<[f32; 3]> = (0..arms).map(|i| colors.get((i + 1) as u64)).collect();
        if max_radius != 0 {
            // A single line down each arm, so arms narrower than a cell still show.
            let reach = reach((xc_adj + width / 2, yc_adj + height / 2), width, height);
            let circle = Circle::new();
            for (a, &color) in arm_colors.iter().enumerate() {
                let angle = a as i32 * degrees_per_arm;
                let angle = if p.counterclockwise {
                    (f64::from(270 - angle) + pos + f64::from(offset)) as i64
                } else {
                    (f64::from(angle - 90) - pos - f64::from(offset)) as i64
                };
                let mut r = 0.0f64;
                while r <= f64::from(max_radius).min(reach) {
                    let bend = (r / f64::from(max_radius) * f64::from(twist)) as i64;
                    let (cos, sin) = circle.at(angle + bend);
                    let x = (r as f32 * cos) as i32 + xc_adj + width / 2;
                    let y = (r as f32 * sin) as i32 + yc_adj + height / 2;
                    raster.set(x, y, Rgba::opaque(color));
                    r += 0.5;
                }
            }
        }
        Self {
            raster,
            arms: (max_radius != 0).then(|| Arms {
                colors: arm_colors,
                shading: p.shading,
                counterclockwise: p.counterclockwise,
                xc_adj,
                yc_adj,
                max_radius,
                degrees_per_arm: degrees_per_arm.max(1),
                twist: twist as f32,
                tmax: tmax as f32,
                pos: wound(pos, degrees_per_arm.max(1) * arms),
                offset: offset as f32,
            }),
        }
    }
}

/// How far the arms have turned, in degrees, as an f32 that stays exact: past a few turns it
/// only matters modulo `period` (the arms repeat every `period` degrees), and two periods are
/// kept on so the angles xLights works out stay positive.
fn wound(pos: f64, period: i32) -> f32 {
    let period = f64::from(period.max(1));
    if pos < 4.0 * period {
        pos as f32
    } else {
        (pos.rem_euclid(period) + 2.0 * period) as f32
    }
}

/// Cosine and sine of every whole degree (the arms are drawn at whole degrees).
struct Circle([(f32, f32); 360]);

impl Circle {
    fn new() -> Self {
        Self(std::array::from_fn(|d| {
            let a = (d as f32).to_radians();
            (a.cos(), a.sin())
        }))
    }

    #[inline]
    fn at(&self, degrees: i64) -> (f32, f32) {
        self.0[degrees.rem_euclid(360) as usize]
    }
}

/// How far from `center` a point can be and still land on the grid (past the farthest corner,
/// with room for rounding): arms are drawn no further.
fn reach((cx, cy): (i32, i32), width: i32, height: i32) -> f64 {
    let far = |c: i32, side: i32| f64::from(c.abs().max((side - 1 - c).abs()));
    far(cx, width).hypot(far(cy, height)) + 2.0
}

/// One spoke (`Draw_arm`): a line from the center out to `max_radius` (or as far as the grid
/// reaches), bending by `twist` degrees along the way, a point every half cell.
#[allow(clippy::too_many_arguments)]
fn spoke(
    raster: &mut Raster,
    circle: &Circle,
    degrees: i64,
    max_radius: i32,
    reach: f64,
    twist: i32,
    (cx, cy): (i32, i32),
    color: Rgba,
) {
    if max_radius == 0 {
        return;
    }
    let mut r = 0.0f32;
    let end = (max_radius as f32).min(reach as f32);
    while r <= end {
        let bend = (r / max_radius as f32 * twist as f32) as i64;
        let (cos, sin) = circle.at(degrees + bend);
        let x = (r * cos + cx as f32) as i32;
        let y = (r * sin + cy as f32) as i32;
        raster.set(x, y, color);
        r += 0.5;
    }
}

impl Arms {
    /// The color at cell (x, y), if it falls on an arm (`PinwheelEffectStyle0`).
    #[inline]
    fn at(&self, x: i32, y: i32, width: i32, height: i32) -> Option<Rgba> {
        let y1 = y as f32 - self.yc_adj as f32 - height as f32 / 2.0;
        let x1 = x as f32 - self.xc_adj as f32 - width as f32 / 2.0;
        let r = x1.hypot(y1);
        if !(r <= self.max_radius as f32 && r > 0.0) {
            return None;
        }
        let bend = r / self.max_radius as f32 * self.twist;
        let mut theta = x1.atan2(y1) * 180.0 / XL_PI + bend;
        if theta.is_nan() {
            theta = 0.0;
        }
        theta = if self.counterclockwise {
            self.pos + theta + self.tmax / 2.0 + self.offset
        } else {
            self.pos - theta + self.tmax / 2.0 + self.offset
        } + 540.0;
        let t2 = (theta as i32) % self.degrees_per_arm;
        if t2 as f32 > self.tmax {
            return None;
        }
        let round = t2 as f32 / self.tmax;
        let across = ((t2 as f32 - self.tmax / 2.0).abs() * 2.0) as i32 as f32;
        let arm = ((theta / self.degrees_per_arm as f32) as i32).rem_euclid(self.colors.len() as i32);
        let color = self.colors[arm as usize];
        let level = match self.shading {
            PinwheelShading::Flat => 1.0,
            PinwheelShading::Raised => (self.tmax - across) / self.tmax,
            PinwheelShading::Sunken => across / self.tmax,
            PinwheelShading::Sweep => 1.0 - round,
        };
        Some(Rgba::with_alpha(color, level.clamp(0.0, 1.0)))
    }
}

impl Shade for Pinwheel {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        if let Some(arms) = &self.arms {
            let (x, y) = cell_of(px, self.raster.width, self.raster.height);
            if let Some(c) = arms.at(x, y, self.raster.width, self.raster.height) {
                return c;
            }
        }
        self.raster.at(px)
    }
}
