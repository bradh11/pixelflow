//! The Lines effect, as xLights draws it (`LinesEffect` in `src-core/effects/LinesEffect.cpp`):
//! each line joins a few points that move in straight lines across the target's grid, bouncing
//! off its edges, `speed` cells a frame. Each line keeps copies of where it was the frames
//! before, drawn behind it as trails (dimmer the older they are, with fading trails).
//!
//! Points move a step a frame, so the lines are worked out frame by frame from the start (see
//! `sim.rs`). Where new lines start is random, keyed to the effect's seed and the frame.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, Rng, Shade, hash};
use crate::geometry::Pixel;
use crate::raster::{Raster, grid_size};
use pf_sequence::LinesParams;
use std::collections::VecDeque;
use std::f64::consts::TAU;

const CREATE: u64 = 0x11_4E5;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    x: f32,
    y: f32,
    angle: f64,
}

impl Point {
    fn flip_x(&mut self) {
        self.angle = (3.0 * std::f64::consts::PI) - self.angle;
        if self.angle >= TAU {
            self.angle -= TAU;
        }
    }

    fn flip_y(&mut self) {
        self.angle = TAU - self.angle;
    }
}

/// One line: where its points are now, then where they were each frame before (`LineObject`).
#[derive(Debug, Clone)]
struct Line {
    trails: VecDeque<Vec<Point>>,
}

/// The lines between frames.
#[derive(Debug, Clone)]
pub(crate) struct Moving {
    width: i32,
    height: i32,
    lines: Vec<Line>,
}

impl Moving {
    pub fn new(canvas: Canvas) -> Self {
        let (width, height) = grid_size(canvas);
        Self {
            width,
            height,
            lines: Vec::new(),
        }
    }

    /// One frame on (frame `frame` of the effect), with the settings then: lines added or taken
    /// away to make the count, then every point moved.
    pub fn step(&mut self, p: &LinesParams, seed: u64, frame: u64) {
        let count = p.count.clamp(1, 20) as usize;
        self.lines.truncate(count);
        while self.lines.len() < count {
            let mut rng = Rng::new(hash(seed ^ CREATE, frame, self.lines.len() as u64));
            let points = (0..p.points.clamp(2, 6))
                .map(|_| Point {
                    x: (rng.unit() * f64::from(self.width)) as f32,
                    y: (rng.unit() * f64::from(self.height)) as f32,
                    angle: rng.unit() * TAU,
                })
                .collect();
            self.lines.push(Line {
                trails: VecDeque::from([points]),
            });
        }
        let speed = f64::from(p.speed.clamp(0.0, 10.0));
        let keep = p.trails.min(10) as usize + 1;
        let (w, h) = (self.width as f32, self.height as f32);
        for line in &mut self.lines {
            line.trails.truncate(keep);
            let last = line.trails.back().cloned();
            for trail in &mut line.trails {
                for pt in trail.iter_mut() {
                    let mut x = pt.x + (pt.angle.cos() * speed) as f32;
                    let mut y = pt.y + (pt.angle.sin() * speed) as f32;
                    if x < 0.0 {
                        x = x.abs();
                        pt.flip_x();
                    }
                    if x >= w {
                        x = 2.0 * w - x;
                        pt.flip_x();
                    }
                    if y < 0.0 {
                        y = y.abs();
                        pt.flip_y();
                    }
                    if y >= h {
                        y = 2.0 * h - y;
                        pt.flip_y();
                    }
                    pt.x = x;
                    pt.y = y;
                }
            }
            if line.trails.len() < keep
                && let Some(last) = last
            {
                line.trails.push_back(last);
            }
        }
    }
}

pub struct Lines {
    raster: Raster,
}

impl Lines {
    /// The lines as they are, each in the next palette color, oldest trail first.
    pub(crate) fn new(moving: &Moving, p: &LinesParams, colors: Colors, canvas: Canvas) -> Self {
        let mut raster = Raster::new(canvas);
        let thickness = p.thickness.clamp(1, 10) as i32;
        let fade = p.fade_trails && p.trails > 0;
        for (k, line) in moving.lines.iter().enumerate() {
            let color = colors.get(k as u64);
            let n = line.trails.len();
            for (i, trail) in line.trails.iter().rev().enumerate() {
                let level = if fade {
                    (255 * (i + 1) / n) as f32 / 255.0
                } else {
                    1.0
                };
                let c = Rgba::with_alpha(color, level);
                let ends = (trail.first(), trail.last());
                if let (Some(a), Some(b)) = ends {
                    thick_line(&mut raster, a, b, c, thickness);
                }
                if trail.len() > 2 {
                    for pair in trail.windows(2) {
                        thick_line(&mut raster, &pair[0], &pair[1], c, thickness);
                    }
                }
            }
        }
        Self { raster }
    }
}

/// `DrawThickLine`: a line, or for thicker ones round ends and parallel lines side by side.
fn thick_line(raster: &mut Raster, a: &Point, b: &Point, color: Rgba, thickness: i32) {
    let (x1, y1, x2, y2) = (a.x as i32, a.y as i32, b.x as i32, b.y as i32);
    if thickness == 1 {
        line(raster, x1, y1, x2, y2, color);
        return;
    }
    raster.circle(x1, y1, thickness / 2, color, true, false);
    raster.circle(x2, y2, thickness / 2, color, true, false);
    for i in 0..thickness {
        let adjust = i - thickness / 2;
        line(raster, x1 + adjust, y1, x2 + adjust, y2, color);
        line(raster, x1, y1 + adjust, x2, y2 + adjust, color);
        line(raster, x1 + adjust, y1 + adjust, x2 + adjust, y2 + adjust, color);
    }
}

/// `DrawLine` with alpha: Bresenham's line, partly covering what's there when not opaque.
fn line(raster: &mut Raster, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba) {
    if color.a >= 1.0 {
        raster.line(x0, y0, x1, y1, color);
        return;
    }
    let (dx, sx) = ((x1 - x0).abs(), if x0 < x1 { 1 } else { -1 });
    let (dy, sy) = ((y1 - y0).abs(), if y0 < y1 { 1 } else { -1 });
    let mut err = (if dx > dy { dx } else { -dy }) / 2;
    let (mut x, mut y) = (x0, y0);
    loop {
        raster.cover(x, y, color);
        if x == x1 && y == y1 {
            break;
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

impl Shade for Lines {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        self.raster.at(px)
    }
}
