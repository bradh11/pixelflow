//! The Tendril effect, as xLights draws it (`TendrilEffect` in `src-core/effects/TendrilEffect.cpp`):
//! springy chains of joints, the first pulled toward a point that moves around the target's grid
//! and each following the one before, drawn as smooth curves through the joints.
//!
//! The point moves (and the joints swing) a step every so many frames, so tendrils are worked out
//! frame by frame from the start (see `sim.rs`). Randomness is keyed to the effect's seed and the
//! frame.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Rng, Shade, hash};
use crate::geometry::Pixel;
use crate::raster::{Raster, grid_size};
use pf_sequence::{TendrilMovement, TendrilParams};
use std::f64::consts::PI;

const MOVE: u64 = 0x7E_2D;
const MAKE: u64 = 0x7E_2E;

#[derive(Debug, Clone, Copy)]
struct Joint {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

/// One tendril (`ATendril`).
#[derive(Debug, Clone)]
struct Strand {
    friction: f32,
    dampening: f32,
    tension: f32,
    spring: f32,
    joints: Vec<Joint>,
}

impl Strand {
    /// Pulls the first joint toward `target`; each joint after it follows the one before.
    fn update(&mut self, (tx, ty): (i32, i32), width: i32, height: i32) {
        let mut spring = self.spring;
        if let Some(first) = self.joints.first_mut() {
            first.vx += (tx as f32 - first.x) * spring;
            first.vy += (ty as f32 - first.y) * spring;
        }
        let (w, h) = (width as f32, height as f32);
        for i in 0..self.joints.len() {
            if i > 0 {
                let prev = self.joints[i - 1];
                let j = &mut self.joints[i];
                j.vx += (prev.x - j.x) * spring;
                j.vy += (prev.y - j.y) * spring;
                j.vx += prev.vx * self.dampening;
                j.vy += prev.vy * self.dampening;
            }
            let j = &mut self.joints[i];
            j.vx *= self.friction;
            j.vy *= self.friction;
            j.x = (j.x + j.vx).clamp(-w, 2.0 * w);
            j.y = (j.y + j.vy).clamp(-h, 2.0 * h);
            spring *= self.tension;
        }
    }

    /// Where the last joint is, to the nearest cell.
    fn end(&self) -> (i32, i32) {
        self.joints.last().map_or((0, 0), |j| {
            (j.x.round_ties_even() as i32, j.y.round_ties_even() as i32)
        })
    }
}

/// The tendrils between frames, and where the point they follow is heading (xLights' `_mv1` to
/// `_mv4`).
#[derive(Debug, Clone)]
pub(crate) struct Tendrils {
    width: i32,
    height: i32,
    strands: Vec<Strand>,
    mv: [i32; 4],
}

/// Xlights' friction, dampening, and tension from the panel's 0–20, 0–20, and 0–39.
fn physics(p: &TendrilParams) -> (f32, f32, f32) {
    let friction = (p.friction as f32 / 20.0 * 0.2 + 0.4).clamp(0.4, 0.6);
    let dampening = (p.dampening as f32 / 20.0 * 0.5).clamp(0.0, 0.5);
    let tension = (p.tension as f32 / 39.0 * 0.039 + 0.96).clamp(0.96, 0.999);
    (friction, dampening, tension)
}

impl Tendrils {
    /// The tendrils as the effect starts, gathered at the point's starting place.
    pub fn new(p: &TendrilParams, seed: u64, canvas: Canvas) -> Self {
        let (width, height) = grid_size(canvas);
        let (w, h) = (width, height);
        let tune = p.movement_size as i32;
        let (tx, ty) = (p.offset_x as i32 * w / 100, p.offset_y as i32 * h / 100);
        let middle = (w / 2 + tx / 2, h / 2 + ty / 2);
        let middle_bottom = (w / 2 + tx / 2, ty);
        let middle_left = (tx, h / 2 + ty / 2);
        use TendrilMovement as M;
        let (start, mv) = match p.movement {
            M::Random => (middle, [0; 4]),
            M::Square => ((tx, ty), [tx, ty, 0, tune.max(1)]),
            M::Circle => (middle, [0, w.min(h) / 2, (tune * 3).max(1), 0]),
            M::HorizontalZigZag => (middle_bottom, [ty, zig(tune).max(1), 1, 0]),
            M::VerticalZigZag => (middle_left, [tx, zig(tune), 1, 0]),
            M::HorizontalZigZagReturn => (middle_bottom, [0, zig(tune).max(1), 1, 0]),
            M::VerticalZigZagReturn => (middle_left, [0, zig(tune), 1, 0]),
            M::Manual => ((p.manual_x as i32 * w / 100, p.manual_y as i32 * h / 100), [0; 4]),
        };
        let (friction, dampening, tension) = physics(p);
        let count = p.tendrils.clamp(1, 20);
        let mut rng = Rng::new(hash(seed, MAKE, 0));
        let strands = (0..count)
            .map(|i| Strand {
                friction: friction + rng.unit() as f32 * 0.01 - 0.005,
                dampening,
                tension,
                spring: 0.45 + 0.025 * (i as f32 / count as f32),
                joints: vec![
                    Joint {
                        x: start.0 as f32,
                        y: start.1 as f32,
                        vx: 0.0,
                        vy: 0.0,
                    };
                    p.length.clamp(5, 100) as usize
                ],
            })
            .collect();
        Self {
            width,
            height,
            strands,
            mv,
        }
    }

    /// Frame `frame` of the effect (0 the first), `absolute` frames from the start of the
    /// sequence, with the settings then: the point moves on when the speed says so.
    pub fn step(&mut self, p: &TendrilParams, seed: u64, frame: u64, absolute: u64) {
        let (w, h) = (self.width, self.height);
        let tune = p.movement_size as i32;
        let (tx, ty) = (p.offset_x as i32 * w / 100, p.offset_y as i32 * h / 100);
        let every = 10 - p.speed.clamp(1, 10) as i64;
        if every > 0 && absolute as i64 % every != 0 {
            return;
        }
        let [mv1, mv2, mv3, mv4] = &mut self.mv;
        let (wf, hf) = (f64::from(w), f64::from(h));
        use TendrilMovement as M;
        let target = match p.movement {
            M::Random => {
                let mut rng = Rng::new(hash(seed, MOVE, frame));
                self.random_target(tune, &mut rng)
            }
            M::Square => {
                *mv4 = tune.max(1);
                match *mv3 {
                    0 => {
                        *mv1 += (w / *mv4).max(1);
                        if *mv1 >= w + tx - w / *mv4 {
                            *mv3 += 1;
                        }
                    }
                    1 => {
                        *mv2 += (h / *mv4).max(1);
                        if *mv2 >= h + ty - h / *mv4 {
                            *mv3 += 1;
                        }
                    }
                    2 => {
                        *mv1 -= (w / *mv4).max(1);
                        if *mv1 <= tx + w / *mv4 {
                            *mv3 += 1;
                        }
                    }
                    _ => {
                        *mv2 -= (h / *mv4).max(1);
                        if *mv2 <= ty + h / *mv4 {
                            *mv3 = 0;
                        }
                    }
                }
                (*mv1, *mv2)
            }
            M::Circle => {
                *mv2 = w.min(h) / 2;
                *mv3 = tune * 3;
                *mv1 += *mv3;
                let a = f64::from(*mv1) / 360.0 * PI * 2.0;
                (
                    (a.sin() * f64::from(*mv2) + wf / 2.0 + f64::from(tx / 2)) as i32,
                    (a.cos() * f64::from(*mv2) + hf / 2.0 + f64::from(ty / 2)) as i32,
                )
            }
            M::HorizontalZigZag => {
                *mv2 = zig(tune).max(1);
                *mv1 += *mv3;
                let mut x = (f64::from(tx)
                    + (wave(hf, *mv2) * PI * f64::from(*mv1) / hf).sin() * wf / 2.0
                    + wf / 2.0) as i32;
                if *mv1 >= ty + h || *mv1 <= ty {
                    *mv3 = -*mv3;
                }
                if *mv3 < 0 {
                    x = w + tx + tx - x;
                }
                (x, *mv1)
            }
            M::VerticalZigZag => {
                *mv2 = zig(tune);
                *mv1 += *mv3;
                let mut y = (f64::from(ty)
                    + (wave(wf, *mv2) * PI * f64::from(*mv1) / wf).sin() * hf / 2.0
                    + hf / 2.0) as i32;
                if *mv1 >= tx + w || *mv1 <= tx {
                    *mv3 = -*mv3;
                }
                if *mv3 < 0 {
                    y = h + ty + ty - y;
                }
                (*mv1, y)
            }
            M::HorizontalZigZagReturn => {
                *mv2 = zig(tune).max(1);
                *mv1 += *mv3;
                let mut x = w / 2 + tx / 2;
                if *mv3 > 0 {
                    x = (f64::from(tx)
                        + (wave(hf, *mv2) * PI * f64::from(*mv1) / hf).sin() * wf / 2.0
                        + wf / 2.0) as i32;
                }
                if *mv1 >= h || *mv1 <= 0 {
                    *mv3 = -*mv3;
                }
                (x, *mv1 + ty)
            }
            M::VerticalZigZagReturn => {
                *mv2 = zig(tune);
                *mv1 += *mv3;
                let mut y = h / 2 + ty / 2;
                if *mv3 > 0 {
                    y = (f64::from(ty)
                        + (wave(wf, *mv2) * PI * f64::from(*mv1) / wf).sin() * hf / 2.0
                        + hf / 2.0) as i32;
                }
                if *mv1 >= w || *mv1 <= 0 {
                    *mv3 = -*mv3;
                }
                (*mv1 + tx, y)
            }
            M::Manual => (p.manual_x as i32 * w / 100 + tx, p.manual_y as i32 * h / 100 + ty),
        };
        for strand in &mut self.strands {
            strand.update(target, w, h);
        }
    }

    /// A random step from where the first tendril ends, kept near the grid (`UpdateRandomMove`).
    fn random_target(&self, tune: i32, rng: &mut Rng) -> (i32, i32) {
        let (w, h) = (self.width, self.height);
        let tune = tune.max(1);
        let (min_x, min_y, max_x, max_y) = (-w / 4, -h / 4, w + w / 4, h + h / 4);
        let (reach_x, reach_y) = (w * 2 * tune / 20, h * 2 * tune / 20);
        let (cx, cy) = self.strands.first().map_or((0, 0), Strand::end);
        let span = |current: i32, reach: i32, max: i32| -> (i32, i32) {
            let lo = if reach > 0 { -current.min(reach) } else { -reach };
            let hi = if reach > 0 {
                (max - current).min(reach)
            } else {
                reach
            };
            (lo, hi)
        };
        let (lo_x, hi_x) = span(cx, reach_x, max_x);
        let (lo_y, hi_y) = span(cy, reach_y, max_y);
        let (moves_x, moves_y) = (hi_x - lo_x, hi_y - lo_y);
        let dx = if moves_x > 0 {
            rng.int(0, moves_x - 1) + lo_x
        } else {
            0
        };
        let dy = if moves_y > 0 {
            rng.int(0, moves_y - 1) + lo_y
        } else {
            0
        };
        (
            (cx + dx).clamp(min_x, max_x.max(min_x)),
            (cy + dy).clamp(min_y, max_y.max(min_y)),
        )
    }
}

/// The zig zag's wave count setting (movement size × 1.5, as a whole number).
fn zig(tune: i32) -> i32 {
    (f64::from(tune) * 1.5) as i32
}

/// Waves across a side `side` cells long: `side / zig`, at least half a wave (never dividing by
/// zero, where xLights would).
fn wave(side: f64, zig: i32) -> f64 {
    (side / f64::from(zig.max(1))).max(0.5)
}

pub struct Tendril {
    raster: Raster,
}

impl Tendril {
    /// The tendrils as they are, in the palette blended over the effect.
    pub(crate) fn new(
        tendrils: &Tendrils,
        p: &TendrilParams,
        time: &EffectTime,
        colors: Colors,
        canvas: Canvas,
    ) -> Self {
        let mut raster = Raster::new(canvas);
        let frames = time.frames();
        let at = if frames > 1 {
            time.frame() as f32 / (frames - 1) as f32
        } else {
            0.0
        };
        let color = colors.ramp(at.min(1.0));
        let thickness = p.thickness as i32;
        for strand in &tendrils.strands {
            draw_strand(&mut raster, strand, color, thickness);
        }
        Self { raster }
    }
}

/// A point on a quadratic Bézier curve.
fn bezier(t: f32, p0: f32, p1: f32, p2: f32) -> f32 {
    let mt = 1.0 - t;
    mt * mt * p0 + 2.0 * mt * t * p1 + t * t * p2
}

/// A tendril as a chain of curves through its joints' midpoints, stamped with soft circles (or
/// soft lines, one cell thick).
fn draw_strand(raster: &mut Raster, strand: &Strand, color: [f32; 3], thickness: i32) {
    let joints = &strand.joints;
    let n = joints.len();
    if n < 3 {
        return;
    }
    let radius = thickness as f32 * 0.5;
    let segment = |raster: &mut Raster,
                   (x0, y0): (f32, f32),
                   (cx, cy): (f32, f32),
                   (x1, y1): (f32, f32),
                   skip_first: bool| {
        let steps = 4.max(((x1 - x0).abs().max((y1 - y0).abs()) * 2.0 + 1.0) as i32);
        let (mut px, mut py) = (x0, y0);
        for i in i32::from(skip_first)..=steps {
            let t = i as f32 / steps as f32;
            let (nx, ny) = (bezier(t, x0, cx, x1), bezier(t, y0, cy, y1));
            if thickness <= 1 {
                if i > 0 {
                    soft_line(raster, px, py, nx, ny, color);
                }
            } else {
                soft_circle(raster, nx, ny, radius, color);
            }
            (px, py) = (nx, ny);
        }
    };
    let mut from = (joints[0].x, joints[0].y);
    for i in 1..n - 2 {
        let (a, b) = (joints[i], joints[i + 1]);
        let mid = ((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        segment(raster, from, (a.x, a.y), mid, i > 1);
        from = mid;
    }
    let (a, b) = (joints[n - 2], joints[n - 1]);
    segment(raster, from, (a.x, a.y), (b.x, b.y), true);
}

/// Mixes `color` into a cell by `coverage`, from the color there (black where nothing is), as
/// xLights' anti-aliased drawing does; the cell ends up fully covered.
fn plot(raster: &mut Raster, x: i32, y: i32, color: [f32; 3], coverage: f32) {
    if coverage <= 0.0 {
        return;
    }
    let coverage = coverage.min(1.0);
    let below = raster.get(x, y);
    let bg = [below.r * below.a, below.g * below.a, below.b * below.a];
    let mix = |i: usize| bg[i] + (color[i] - bg[i]) * coverage;
    raster.set(x, y, Rgba::opaque([mix(0), mix(1), mix(2)]));
}

/// `DrawAACircle`: a filled circle with a soft edge.
fn soft_circle(raster: &mut Raster, cx: f32, cy: f32, radius: f32, color: [f32; 3]) {
    let r = (radius + 1.0).ceil() as i32;
    let (ixc, iyc) = (cx.round() as i32, cy.round() as i32);
    let (x_min, x_max) = ((ixc - r).max(0), (ixc + r).min(raster.width - 1));
    let (y_min, y_max) = ((iyc - r).max(0), (iyc + r).min(raster.height - 1));
    let outer = radius + 0.75;
    let inner = (radius - 0.75).max(0.0);
    for y in y_min..=y_max {
        let dy = y as f32 - cy;
        for x in x_min..=x_max {
            let dx = x as f32 - cx;
            let d2 = dx * dx + dy * dy;
            if d2 > outer * outer {
                continue;
            }
            if d2 <= inner * inner {
                raster.set(x, y, Rgba::opaque(color));
            } else {
                let coverage = ((outer - d2.sqrt()) / (outer - inner)).clamp(0.0, 1.0);
                plot(raster, x, y, color, coverage);
            }
        }
    }
}

/// `DrawAALine`: Xiaolin Wu's anti-aliased line.
fn soft_line(raster: &mut Raster, x0: f32, y0: f32, x1: f32, y1: f32, color: [f32; 3]) {
    let steep = (y1 - y0).abs() > (x1 - x0).abs();
    let (mut ax0, mut ay0, mut ax1, mut ay1) = if steep { (y0, x0, y1, x1) } else { (x0, y0, x1, y1) };
    if ax0 > ax1 {
        std::mem::swap(&mut ax0, &mut ax1);
        std::mem::swap(&mut ay0, &mut ay1);
    }
    let mut put = |a: i32, b: i32, coverage: f32| {
        if steep {
            plot(raster, b, a, color, coverage);
        } else {
            plot(raster, a, b, color, coverage);
        }
    };
    let dx = ax1 - ax0;
    let gradient = if dx < 0.001 { 1.0 } else { (ay1 - ay0) / dx };
    let x_end = ax0.round();
    let y_end = ay0 + gradient * (x_end - ax0);
    let gap = 1.0 - (ax0 + 0.5 - (ax0 + 0.5).floor());
    let (xp1, yp1) = (x_end as i32, y_end.floor() as i32);
    put(xp1, yp1, (1.0 - (y_end - yp1 as f32)) * gap);
    put(xp1, yp1 + 1, (y_end - yp1 as f32) * gap);
    let mut inter_y = y_end + gradient;
    let x_end = ax1.round();
    let y_end = ay1 + gradient * (x_end - ax1);
    let gap = ax1 + 0.5 - (ax1 + 0.5).floor();
    let (xp2, yp2) = (x_end as i32, y_end.floor() as i32);
    put(xp2, yp2, (1.0 - (y_end - yp2 as f32)) * gap);
    put(xp2, yp2 + 1, (y_end - yp2 as f32) * gap);
    for x in xp1 + 1..xp2 {
        let iy = inter_y.floor() as i32;
        let f = inter_y - iy as f32;
        put(x, iy, 1.0 - f);
        put(x, iy + 1, f);
        inter_y += gradient;
    }
}

impl Shade for Tendril {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        self.raster.at(px)
    }
}
