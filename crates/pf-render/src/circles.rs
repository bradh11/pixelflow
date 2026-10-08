//! The Circles effect, as xLights draws it (`CirclesEffect` in `src-core/effects/CirclesEffect.cpp`
//! and `.h`): balls moving across the target's grid, or rings spreading from a point.
//!
//! Each ball starts at a random cell with a random heading and one of three speeds, and moves in
//! a straight line, wrapping around the grid's edges or bouncing off them. xLights steps the balls
//! frame by frame; here each frame works out where they are from the time alone (the same
//! straight lines and reflections), so any frame can be drawn on its own. The randomness is keyed
//! to the effect's seed and each ball.
//!
//! The rings (xLights' Radial and Radial 3D) are worked out from the time alone in xLights too.

use crate::color::{Colors, Rgba, from_hsv, to_hsv};
use crate::effects::{Canvas, EffectTime, Shade, hash, hash01};
use crate::geometry::Pixel;
use crate::raster::{Raster, cell_of, grid_size};
use pf_sequence::{CirclesLook, CirclesParams};

/// Most balls drawn (xLights' `MAX_RGB_BALLS`).
const MAX_BALLS: u32 = 20;

/// One ball this frame: where it is (cells, with fractions), its size, and its color.
#[derive(Debug, Clone, Copy)]
struct Ball {
    x: f32,
    y: f32,
    radius: f32,
    color: [f32; 3],
}

pub struct Circles {
    look: Look,
}

enum Look {
    /// Balls, rings, or bubbles drawn on the grid.
    Drawn(Raster),
    /// Plasma: each cell glows by how near it is to the balls.
    Plasma {
        balls: Vec<Ball>,
        width: i32,
        height: i32,
    },
}

/// `randInt(lo, hi)`: a whole number from `lo` to `hi`, for ball `k`'s draw number `n`.
fn rand_int(seed: u64, k: u64, n: u64, lo: i32, hi: i32) -> i32 {
    let span = (hi - lo + 1).max(1) as f32;
    lo + ((hash01(seed, k, n) * span) as i32).min(hi - lo)
}

/// Where something moving `travel` cells from `start` is on a side `size` cells long: wrapped
/// around, or bouncing between `margin` from each end.
fn along(start: f32, travel: f32, size: f32, margin: f32, bounce: bool) -> f32 {
    let at = start + travel;
    if !bounce {
        return at.rem_euclid(size.max(1.0));
    }
    let (lo, hi) = (margin, size - margin);
    if hi <= lo {
        return size / 2.0;
    }
    let span = hi - lo;
    let m = (at - lo).rem_euclid(2.0 * span);
    lo + if m <= span { m } else { 2.0 * span - m }
}

impl Circles {
    pub fn new(p: &CirclesParams, time: &EffectTime, colors: Colors, seed: u64, canvas: Canvas) -> Self {
        let (width, height) = grid_size(canvas);
        let count = p.count.min(MAX_BALLS);
        match p.look {
            CirclesLook::Radial | CirclesLook::RainbowRadial => {
                let mut raster = Raster::new(canvas);
                radial(&mut raster, p, time, colors);
                return Self {
                    look: Look::Drawn(raster),
                };
            }
            _ => {}
        }
        let bubbles = p.look == CirclesLook::Bubbles;
        let radius = p.size as f32;
        // xLights moves each ball `speed × frame / 200` of its step each frame.
        let travel = f64::from(p.speed) * time.elapsed_ms as f64 / 200.0;
        let balls: Vec<Ball> = (0..count)
            .map(|k| {
                let k = u64::from(k);
                let x0 = rand_int(seed, k, 0, 0, width - 1) as f32;
                let y0 = rand_int(seed, k, 1, 0, height - 1) as f32;
                let steps = rand_int(seed, k, 2, 0, 2) + 1;
                // xLights picks a whole number of degrees and uses it as radians.
                let turn = rand_int(seed, k, 3, 0, 89) as f32;
                let mut heading = if hash(seed, k, 4) & 1 == 0 { turn } else { -turn };
                if bubbles {
                    heading = (90.0 + rand_int(seed, k, 5, 0, 44) as f32 - 22.5) * 2.0 * std::f32::consts::PI
                        / 180.0;
                }
                let (mut dx, mut dy) = (steps as f32 * heading.cos(), steps as f32 * heading.sin());
                if p.bounce {
                    // Bouncing never lets a ball creep along an edge.
                    dx = dx.signum() * dx.abs().max(0.2);
                    dy = dy.signum() * dy.abs().max(0.2);
                }
                Ball {
                    x: along(
                        x0,
                        (f64::from(dx) * travel) as f32,
                        width as f32,
                        radius,
                        p.bounce,
                    ),
                    y: along(
                        y0,
                        (f64::from(dy) * travel) as f32,
                        height as f32,
                        radius,
                        p.bounce,
                    ),
                    radius,
                    color: colors.get(k),
                }
            })
            .collect();
        if p.look == CirclesLook::Plasma {
            return Self {
                look: Look::Plasma { balls, width, height },
            };
        }
        let mut raster = Raster::new(canvas);
        let wrap = !p.bounce;
        for ball in &balls {
            let (x, y, r) = (ball.x as i32, ball.y as i32, ball.radius as i32);
            match p.look {
                CirclesLook::Fading => raster.fading_circle(x, y, r, ball.color, wrap),
                _ => raster.circle(x, y, r, Rgba::opaque(ball.color), !bubbles, wrap),
            }
        }
        Self {
            look: Look::Drawn(raster),
        }
    }
}

/// Rings of the palette colors (or a rainbow) filling out from a point, as `RenderRadial` draws
/// them: bands `rows / (size + 1)` cells wide, moving outward as they go.
fn radial(raster: &mut Raster, p: &CirclesParams, time: &EffectTime, colors: Colors) {
    let (width, height) = (raster.width, raster.height);
    let state = (time.elapsed_ms as f64 * f64::from(p.speed) / 50.0).min(1e9) as i64;
    let (half_w, half_h) = (width / 2, height / 2);
    let x = (half_w as f32 + p.center_x / 50.0 * half_w as f32) as i32;
    let y = (half_h as f32 + p.center_y / 50.0 * half_h as f32) as i32;
    let thickness = p.size as i64;
    let band = (i64::from(height) / (thickness + 1)).max(1);
    let max_radius = if state > i64::from(height) {
        i64::from(height)
    } else {
        state / 2 + thickness
    };
    let block = colors.len() as i64 * band;
    let offset = state / 4 % (block + 1);
    let rainbow = p.look == CirclesLook::RainbowRadial;
    let rings = f64::from(p.count.max(1));
    let mut last = None;
    for ring in (0..=max_radius).rev() {
        let n = ring - offset + block;
        let mut color = colors.get((n.rem_euclid(block) / band) as u64);
        if rainbow {
            let hue = if max_radius > 0 {
                (ring + state) as f64 / (max_radius as f64 / rings)
            } else {
                0.0
            };
            color = from_hsv([hue.fract() as f32, 1.0, 1.0]);
        }
        if last != Some(color) {
            raster.circle(x, y, ring as i32, Rgba::opaque(color), true, false);
            last = Some(color);
        }
    }
}

impl Shade for Circles {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        match &self.look {
            Look::Drawn(raster) => raster.at(px),
            Look::Plasma { balls, width, height } => {
                // Each ball adds its color where its pull (radius over distance) passes 0.3, at
                // that strength; cells pulled 0.9 or more in all light.
                let (x, y) = cell_of(px, *width, *height);
                let (x, y) = (x as f32, y as f32);
                let mut sum = 0.0;
                let mut glow = [0.0f32; 3];
                for ball in balls {
                    let pull = if x == ball.x && y == ball.y {
                        1.0
                    } else {
                        ball.radius / (x - ball.x).hypot(y - ball.y)
                    };
                    sum += pull;
                    if pull > 0.3 {
                        let [h, s, _] = to_hsv(ball.color);
                        let c = from_hsv([h, s, pull.min(1.0)]);
                        glow = [0, 1, 2].map(|i| (glow[i] + c[i]).min(1.0));
                    }
                }
                if sum >= 0.9 {
                    Rgba::opaque(glow)
                } else {
                    Rgba::CLEAR
                }
            }
        }
    }
}
