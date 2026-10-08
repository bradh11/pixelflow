//! The Shape effect, as xLights draws it (`ShapeEffect` in `src-core/effects/ShapeEffect.cpp`):
//! outlines of circles, stars, hearts, trees, and the rest, each growing (or shrinking) and
//! fading over its lifetime, drawn on the target's grid with xLights' own drawing code.
//!
//! xLights keeps `count` shapes alive: each lives for the lifetime and is replaced as it goes,
//! somewhere new. It steps them frame by frame; here each shape slot repeats its life on a fixed
//! cycle (starting part way through when the start is staggered), so any frame can be worked out
//! on its own. With a timing track, a shape appears at each mark instead. The randomness (places,
//! drift, random shapes) is keyed to the effect's seed, the slot, and which life it's on.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Shade, hash01};
use crate::geometry::Pixel;
use crate::raster::Raster;
use pf_sequence::{ShapeObject, ShapeParams};
use std::f64::consts::PI;

/// The fastest random drift, in pixels per second (xLights' 20 pixels a frame at 20 frames a
/// second).
const RANDOM_DRIFT_MAX: f64 = 400.0;
/// What a random shape can be: everything but the ellipse, in xLights' order.
const RANDOM_SHAPES: [ShapeObject; 13] = [
    ShapeObject::Circle,
    ShapeObject::Square,
    ShapeObject::Triangle,
    ShapeObject::Star,
    ShapeObject::Pentagon,
    ShapeObject::Hexagon,
    ShapeObject::Octagon,
    ShapeObject::Heart,
    ShapeObject::Tree,
    ShapeObject::CandyCane,
    ShapeObject::Snowflake,
    ShapeObject::Crucifix,
    ShapeObject::Present,
];

pub struct Shape {
    raster: Raster,
}

/// One shape this frame.
struct Drawn {
    age: f64,
    x: i32,
    y: i32,
    size: f64,
    object: ShapeObject,
    color: Rgba,
}

/// Which random number is which, for one shape's life.
#[derive(Clone, Copy)]
enum Roll {
    X = 0,
    Y,
    Object,
    Speed,
    Heading,
}

impl Shape {
    /// `marks`: with a timing track, when its marks fall, in milliseconds from the effect's start
    /// (`None` keeps `count` shapes shown instead).
    pub fn new(
        p: &ShapeParams,
        time: &EffectTime,
        colors: Colors,
        seed: u64,
        canvas: Canvas,
        marks: Option<&[u64]>,
    ) -> Self {
        let mut raster = Raster::new(canvas);
        let (width, height) = (raster.width, raster.height);
        let life = (time.length_ms as f64 * f64::from(p.lifetime) / 100.0).max(1.0);
        let now = time.elapsed_ms as f64;
        let center = (
            (f64::from(p.center_x) * f64::from(width) / 100.0) as i32,
            (f64::from(p.center_y) * f64::from(height) / 100.0) as i32,
        );
        // Shape `n` on its life `k`: where it is, what it is, and how it looks at `age`.
        let make = |n: u64, k: u64, age: f64, color_index: u64| -> Drawn {
            let key = n.wrapping_mul(0x1_0000_0001).wrapping_add(k);
            let roll = |r: Roll| f64::from(hash01(seed, key, r as u64));
            let (x, y) = if p.random_location {
                (
                    (roll(Roll::X) * f64::from(width)) as i32,
                    (roll(Roll::Y) * f64::from(height)) as i32,
                )
            } else {
                center
            };
            let (speed, heading) = if p.random_movement {
                (roll(Roll::Speed) * RANDOM_DRIFT_MAX, roll(Roll::Heading) * 359.0)
            } else {
                (f64::from(p.speed), f64::from(p.direction))
            };
            let drift = speed * age / 1000.0;
            let heading = heading.to_radians();
            let object = if p.shape == ShapeObject::Random {
                RANDOM_SHAPES[((roll(Roll::Object) * 13.0) as usize).min(12)]
            } else {
                p.shape
            };
            let level = if p.fade { (1.0 - age / life).max(0.0) } else { 1.0 };
            Drawn {
                age,
                x: x + (drift * heading.cos()).round() as i32,
                y: y + (drift * heading.sin()).round() as i32,
                size: (f64::from(p.start_size) + f64::from(p.growth) * age / life).max(0.0),
                object,
                color: Rgba::with_alpha(colors.get(color_index), level as f32),
            }
        };
        let mut shapes: Vec<Drawn> = match marks {
            Some(marks) => marks
                .iter()
                .enumerate()
                .filter(|&(_, &at)| at as f64 <= now && now - (at as f64) < life)
                .map(|(k, &at)| make(u64::MAX, k as u64, now - at as f64, k as u64))
                .collect(),
            None => (0..u64::from(p.count))
                .map(|n| {
                    let offset = if p.random_start {
                        f64::from(hash01(seed, n, u64::MAX)) * life
                    } else {
                        0.0
                    };
                    let lives = (now + offset) / life;
                    let k = lives.floor();
                    let age = (lives - k) * life;
                    // Colors go round the palette as shapes appear.
                    let k = k as u64;
                    make(n, k, age, n + k * u64::from(p.count))
                })
                .collect(),
        };
        // Oldest first, so newer shapes draw over them.
        shapes.sort_by(|a, b| b.age.total_cmp(&a.age));
        let rotation = f64::from(p.rotation);
        for s in &shapes {
            draw(&mut raster, s, p, rotation);
        }
        Self { raster }
    }
}

fn draw(raster: &mut Raster, s: &Drawn, p: &ShapeParams, rotation: f64) {
    let (xc, yc, r, c) = (s.x, s.y, s.size, s.color);
    let thickness = f64::from(p.thickness.max(1));
    match s.object {
        ShapeObject::Square => polygon(raster, xc, yc, r, 4, c, thickness, rotation + 45.0),
        ShapeObject::Circle => circle(raster, xc, yc, r, c, thickness),
        ShapeObject::Star => star(raster, xc, yc, r, p.points.max(2), c, thickness, rotation),
        ShapeObject::Triangle => polygon(raster, xc, yc, r, 3, c, thickness, rotation + 90.0),
        ShapeObject::Pentagon => polygon(raster, xc, yc, r, 5, c, thickness, rotation + 90.0),
        ShapeObject::Hexagon => polygon(raster, xc, yc, r, 6, c, thickness, rotation),
        ShapeObject::Octagon => polygon(raster, xc, yc, r, 8, c, thickness, rotation + 22.5),
        ShapeObject::Tree => outline(raster, xc, yc, r, c, thickness, rotation, &TREE),
        ShapeObject::Crucifix => outline(raster, xc, yc, r, c, thickness, rotation, &CROSS),
        ShapeObject::Present => outline(raster, xc, yc, r, c, thickness, rotation, &PRESENT),
        ShapeObject::CandyCane => candy_cane(raster, xc, yc, r, c, thickness),
        ShapeObject::Snowflake => snowflake(raster, xc, yc, r, 3, c, rotation + 30.0),
        ShapeObject::Heart => heart(raster, xc, yc, r, c, thickness, rotation),
        ShapeObject::Ellipse => ellipse(raster, xc, yc, r, p.points, c, thickness, rotation),
        // Never drawn: a random shape is picked when it appears.
        ShapeObject::Random => {}
    }
}

/// How many outlines a thickness draws, each `step` smaller than the last: xLights' `for (i = 0;
/// i < thickness - 1 + step; i += step)`, stopping when the radius goes below zero.
fn rings(radius: f64, thickness: f64, step: f64) -> impl Iterator<Item = f64> {
    let limit = thickness - 1.0 + step;
    let mut i = 0.0;
    let mut r = radius;
    std::iter::from_fn(move || {
        if i >= limit || r < 0.0 {
            return None;
        }
        let now = r;
        i += step;
        r -= step;
        Some(now)
    })
}

/// xLights' `for (degrees = 0; degrees < 361; degrees += increment)`, which ends on exactly 360.
fn around(increment: f64) -> impl Iterator<Item = f64> {
    let mut degrees = 0.0;
    std::iter::from_fn(move || {
        if degrees >= 361.0 || increment <= 0.0 {
            return None;
        }
        if degrees > 360.0 {
            degrees = 360.0;
        }
        let now = degrees;
        degrees = if now == 360.0 { 361.0 } else { now + increment };
        Some(now)
    })
}

fn round(v: f64) -> i32 {
    v.round() as i32
}

fn circle(raster: &mut Raster, xc: i32, yc: i32, radius: f64, c: Rgba, thickness: f64) {
    for r in rings(radius, thickness, 0.75) {
        for degrees in 0..360 {
            let a = f64::from(degrees).to_radians();
            raster.set(round(r * a.cos()) + xc, round(r * a.sin()) + yc, c);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn ellipse(
    raster: &mut Raster,
    xc: i32,
    yc: i32,
    radius: f64,
    ratio: u32,
    c: Rgba,
    thickness: f64,
    rotation: f64,
) {
    let (sin, cos) = rotation.to_radians().sin_cos();
    let tall = f64::from(ratio) / 10.0;
    for r in rings(radius, thickness, 0.75) {
        for degrees in 0..360 {
            let a = f64::from(degrees).to_radians();
            let (x, y) = (r * a.cos(), r * tall * a.sin());
            raster.set(round(x * cos - y * sin) + xc, round(y * cos + x * sin) + yc, c);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn star(
    raster: &mut Raster,
    xc: i32,
    yc: i32,
    radius: f64,
    points: u32,
    c: Rgba,
    thickness: f64,
    rotation: f64,
) {
    let offset = match points {
        5 => 90.0 - 360.0 / 5.0,
        6 => 30.0,
        7 => 90.0 - 360.0 / 7.0,
        _ => 0.0,
    };
    let increment = 360.0 / f64::from(points);
    let at = |r: f64, degrees: f64| {
        let a = (rotation + offset + degrees).to_radians();
        (round(r * a.cos()) + xc, round(r * a.sin()) + yc)
    };
    for r in rings(radius, thickness, 0.6) {
        // The inner points sit at the outer radius over the golden ratio squared.
        let inner = r / 2.618034;
        for degrees in around(increment) {
            let (xo, yo) = at(r, degrees);
            let (xi, yi) = at(inner, degrees + increment / 2.0);
            raster.line(xi, yi, xo, yo, c);
            let (xi, yi) = at(inner, degrees - increment / 2.0);
            raster.line(xi, yi, xo, yo, c);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn polygon(
    raster: &mut Raster,
    xc: i32,
    yc: i32,
    radius: f64,
    sides: u32,
    c: Rgba,
    thickness: f64,
    rotation: f64,
) {
    let increment = 360.0 / f64::from(sides);
    let mut last: Vec<(i32, i32, i32, i32)> = Vec::new();
    for r in rings(radius, thickness, 0.05) {
        let edges: Vec<(i32, i32, i32, i32)> = around(increment)
            .map(|degrees| {
                let (a, b) = (
                    (rotation + degrees).to_radians(),
                    (rotation + degrees + increment).to_radians(),
                );
                (
                    round(r * a.cos()) + xc,
                    round(r * a.sin()) + yc,
                    round(r * b.cos()) + xc,
                    round(r * b.sin()) + yc,
                )
            })
            .collect();
        // Thick outlines step in by a twentieth of a pixel at a time; the same edges again
        // would draw the same cells.
        if edges == last {
            continue;
        }
        for &(x1, y1, x2, y2) in &edges {
            raster.line(x1, y1, x2, y2, c);
        }
        last = edges;
    }
}

fn snowflake(raster: &mut Raster, xc: i32, yc: i32, radius: f64, sides: u32, c: Rgba, rotation: f64) {
    if radius < 0.0 {
        return;
    }
    let increment = 360.0 / f64::from(sides * 2);
    let mut angle = rotation;
    for _ in 0..sides * 2 {
        let (a, b) = (angle.to_radians(), (180.0 + angle).to_radians());
        raster.line(
            round(radius * a.cos()) + xc,
            round(radius * a.sin()) + yc,
            round(radius * b.cos()) + xc,
            round(radius * b.sin()) + yc,
            c,
        );
        angle += increment;
    }
}

fn heart(raster: &mut Raster, xc: i32, yc: i32, radius: f64, c: Rgba, thickness: f64, rotation: f64) {
    let (sin, cos) = rotation.to_radians().sin_cos();
    let turn = |x: f64, y: f64| {
        let (rx, ry) = (
            x * cos - y * sin + f64::from(xc),
            y * cos + x * sin + f64::from(yc),
        );
        (rx.is_finite() && ry.is_finite()).then(|| (round(rx), round(ry)))
    };
    let step = 0.01;
    let mut x: f64 = -2.0;
    while x <= 2.0 {
        let y1 = (1.0 - (x.abs() - 1.0) * (x.abs() - 1.0)).sqrt();
        let y2 = (1.0 - x.abs()).acos() - PI;
        for r in rings(radius, thickness, 0.75) {
            let xx = x * r / 2.0;
            let (mut top, mut bottom) = (y1 * r / 2.0, y2 * r / 2.0);
            for y in [top, bottom] {
                if let Some((px, py)) = turn(xx, y) {
                    raster.set(px, py, c);
                }
            }
            // Close the point at each side.
            if x + step > 2.0 || x == -2.0 + step {
                if top > bottom {
                    std::mem::swap(&mut top, &mut bottom);
                }
                let mut z = top;
                while z < bottom {
                    if let Some((px, py)) = turn(xx, z) {
                        raster.set(px, py, c);
                    }
                    z += 0.5;
                }
            }
        }
        x += step;
    }
}

/// A line between two points on a small grid.
type Segment = ((i32, i32), (i32, i32));

/// A shape made of lines between points on a small grid: the points, and the grid's center and
/// size (a radius spans `size` grid steps).
struct Outline {
    lines: &'static [Segment],
    center: (f64, f64),
    size: (f64, f64),
}

const TREE: Outline = Outline {
    lines: &[
        ((3, 0), (5, 0)),
        ((5, 0), (5, 3)),
        ((3, 0), (3, 3)),
        ((0, 3), (8, 3)),
        ((0, 3), (2, 6)),
        ((8, 3), (6, 6)),
        ((1, 6), (2, 6)),
        ((6, 6), (7, 6)),
        ((1, 6), (3, 9)),
        ((7, 6), (5, 9)),
        ((2, 9), (3, 9)),
        ((5, 9), (6, 9)),
        ((6, 9), (4, 11)),
        ((2, 9), (4, 11)),
    ],
    center: (4.0, 4.0),
    size: (11.0, 11.0),
};

const CROSS: Outline = Outline {
    lines: &[
        ((2, 0), (2, 6)),
        ((2, 6), (0, 6)),
        ((0, 6), (0, 7)),
        ((0, 7), (2, 7)),
        ((2, 7), (2, 10)),
        ((2, 10), (3, 10)),
        ((3, 10), (3, 7)),
        ((3, 7), (5, 7)),
        ((5, 7), (5, 6)),
        ((5, 6), (3, 6)),
        ((3, 6), (3, 0)),
        ((3, 0), (2, 0)),
    ],
    center: (2.5, 6.5),
    size: (7.0, 10.0),
};

const PRESENT: Outline = Outline {
    lines: &[
        ((0, 0), (0, 9)),
        ((0, 9), (10, 9)),
        ((10, 9), (10, 0)),
        ((10, 0), (0, 0)),
        ((5, 0), (5, 9)),
        ((5, 9), (2, 11)),
        ((2, 11), (2, 9)),
        ((5, 9), (8, 11)),
        ((8, 11), (8, 9)),
    ],
    center: (5.0, 5.5),
    size: (7.0, 10.0),
};

#[allow(clippy::too_many_arguments)]
fn outline(
    raster: &mut Raster,
    xc: i32,
    yc: i32,
    radius: f64,
    c: Rgba,
    thickness: f64,
    rotation: f64,
    shape: &Outline,
) {
    let (sin, cos) = rotation.to_radians().sin_cos();
    for r in rings(radius, thickness, 0.75) {
        // Each point snaps to a whole cell, then turns (xLights takes the turned point toward
        // zero).
        let at = |(x, y): (i32, i32)| {
            let px = f64::from(round((f64::from(x) - shape.center.0) / shape.size.0 * r));
            let py = f64::from(round((f64::from(y) - shape.center.1) / shape.size.1 * r));
            (
                (f64::from(xc) + px * cos - py * sin) as i32,
                (f64::from(yc) + py * cos + px * sin) as i32,
            )
        };
        for &(from, to) in shape.lines {
            let ((x1, y1), (x2, y2)) = (at(from), at(to));
            raster.line(x1, y1, x2, y2, c);
        }
    }
}

fn candy_cane(raster: &mut Raster, xc: i32, yc: i32, radius: f64, c: Rgba, thickness: f64) {
    let full = radius;
    for r in rings(radius, thickness, 0.75) {
        // The stick.
        let y1 = round(f64::from(yc) + full / 6.0);
        let y2 = round(f64::from(yc) - full / 2.0);
        let x = round(f64::from(xc) + r / 2.0);
        raster.line(x, y1, x, y2, c);
        // The hook.
        let hook = r / 3.0 - 0.75;
        for degrees in 0..180 {
            let a = f64::from(degrees).to_radians();
            raster.set(
                round(hook * a.cos() + f64::from(xc) + full / 6.0),
                round(hook * a.sin() + f64::from(y1)),
                c,
            );
        }
    }
}

impl Shade for Shape {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        self.raster.at(px)
    }
}
