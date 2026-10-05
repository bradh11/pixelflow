//! The effects. Each is a pure function of the effect's time, one pixel, its settings, and its
//! palette (plus the effect id as a random seed), so any frame can be rendered on its own:
//! seeking and export give the same picture every time.
//!
//! Time-dependent values are worked out once per frame (in `new`, in double precision so long
//! effects stay smooth), then [`Shade::shade`] runs for every pixel.

use crate::color::{Colors, Rgba, unit};
use crate::geometry::Pixel;
use pf_sequence::{
    Axis, BarsParams, ChaseParams, ColorWashParams, Direction, EffectParams, FadeDirection, FadeParams,
    FireParams, Gradient, MeteorDirection, MeteorsParams, OnParams, RippleParams, ShimmerParams,
    SpiralParams, StrobeParams, TwinkleParams, WaveParams,
};
use std::f32::consts::TAU;

/// Most meteors drawn at once on one target.
pub const MAX_METEORS: u32 = 100;
/// Most bands, bars, or stripes drawn on one target.
const MAX_REPEATS: u32 = 10_000;

/// Where an effect is in its own time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectTime {
    /// 0.0 at the start of the effect, approaching 1.0 at its end.
    pub t_norm: f32,
    /// Milliseconds since the effect started.
    pub elapsed_ms: u64,
}

impl EffectTime {
    /// The time `t_ms` within an effect running from `start_ms` to `end_ms`.
    pub fn within(start_ms: u64, end_ms: u64, t_ms: u64) -> Self {
        let length = end_ms.saturating_sub(start_ms).max(1);
        let elapsed_ms = t_ms.saturating_sub(start_ms);
        Self {
            t_norm: (elapsed_ms as f64 / length as f64).clamp(0.0, 1.0) as f32,
            elapsed_ms,
        }
    }

    fn seconds(&self) -> f64 {
        self.elapsed_ms as f64 / 1000.0
    }

    /// How many times something happening `per_second` times a second has happened: whole
    /// count and fraction of the current one.
    fn cycles(&self, per_second: f32) -> (u64, f32) {
        let x = self.seconds() * f64::from(sane(per_second, 0.0, 1e6)).abs();
        (x.floor() as u64, x.fract() as f32)
    }
}

/// The size of the target being drawn, for effects that work on a grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Canvas {
    pub columns: u32,
    pub rows: u32,
}

/// Turns NaN and infinities into `fallback`, and clamps to `lo..=hi`.
fn sane(v: f32, fallback: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(-hi, hi) } else { fallback }
}

/// Deterministic randomness: a well-mixed 64-bit hash of a seed and two numbers.
#[inline]
pub(crate) fn hash(seed: u64, a: u64, b: u64) -> u64 {
    let mut z = seed ^ a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// [`hash`] as a fraction in `0.0..1.0`.
#[inline]
pub(crate) fn hash01(seed: u64, a: u64, b: u64) -> f32 {
    (hash(seed, a, b) >> 40) as f32 / (1u64 << 24) as f32
}

/// Draws one effect, one pixel at a time.
pub trait Shade {
    fn shade(&self, px: &Pixel) -> Rgba;
}

/// Pixel position along the target in wiring order, at the pixel's center (0–1).
#[inline]
fn along(px: &Pixel) -> f32 {
    (px.index as f32 + 0.5) / px.count.max(1) as f32
}

fn flip(x: f32, direction: Direction) -> f32 {
    match direction {
        Direction::Forward => x,
        Direction::Reverse => 1.0 - x,
    }
}

fn gradient_position(px: &Pixel, gradient: Gradient) -> Option<f32> {
    match gradient {
        Gradient::None => None,
        Gradient::Horizontal => Some(px.u),
        Gradient::Vertical => Some(px.v),
    }
}

// ---------------------------------------------------------------------------------------------

pub struct On {
    colors: Colors,
    gradient: Gradient,
    level: f32,
}

impl On {
    pub fn new(p: &OnParams, time: &EffectTime, colors: Colors) -> Self {
        let (start, end) = (unit(p.start_level), unit(p.end_level));
        Self {
            colors,
            gradient: p.gradient,
            level: start + (end - start) * time.t_norm,
        }
    }
}

impl Shade for On {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let c = match gradient_position(px, self.gradient) {
            None => self.colors.get(0),
            Some(x) => self.colors.ramp(x),
        };
        Rgba::with_alpha(c, self.level)
    }
}

pub struct Off;

impl Shade for Off {
    #[inline]
    fn shade(&self, _px: &Pixel) -> Rgba {
        Rgba::BLACK
    }
}

pub struct ColorWash {
    colors: Colors,
    gradient: Gradient,
    position: f32,
}

impl ColorWash {
    pub fn new(p: &ColorWashParams, time: &EffectTime, colors: Colors) -> Self {
        let cycles = sane(p.cycles, 1.0, 1e4).abs();
        Self {
            colors,
            gradient: p.gradient,
            position: (f64::from(time.t_norm) * f64::from(cycles)).rem_euclid(2.0) as f32,
        }
    }
}

impl Shade for ColorWash {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let offset = gradient_position(px, self.gradient).unwrap_or(0.0);
        Rgba::opaque(self.colors.ping_pong(self.position + offset))
    }
}

pub struct Fade {
    color: [f32; 3],
    level: f32,
}

impl Fade {
    pub fn new(p: &FadeParams, time: &EffectTime, colors: Colors) -> Self {
        let level = match p.direction {
            FadeDirection::In => time.t_norm,
            FadeDirection::Out => 1.0 - time.t_norm,
        };
        Self {
            color: colors.get(0),
            level,
        }
    }
}

impl Shade for Fade {
    #[inline]
    fn shade(&self, _px: &Pixel) -> Rgba {
        Rgba::with_alpha(self.color, self.level)
    }
}

pub struct Chase {
    colors: Colors,
    head: f32,
    bands: f32,
    width: f32,
}

impl Chase {
    pub fn new(p: &ChaseParams, time: &EffectTime, colors: Colors) -> Self {
        let (whole, fraction) = time.cycles(p.speed);
        let head = if p.bounce {
            // Out on even trips, back on odd ones.
            if whole % 2 == 0 { fraction } else { 1.0 - fraction }
        } else {
            fraction
        };
        Self {
            colors,
            head: flip(head, p.direction),
            bands: p.bands.clamp(1, MAX_REPEATS) as f32,
            width: unit(p.width),
        }
    }
}

impl Shade for Chase {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        // In band units, measured from the head: band k covers [k, k + width).
        let s = (along(px) - self.head) * self.bands;
        let band = s.floor();
        if s - band < self.width {
            let k = band.rem_euclid(self.bands) as u64;
            Rgba::opaque(self.colors.get(k))
        } else {
            Rgba::CLEAR
        }
    }
}

pub struct Bars {
    colors: Colors,
    axis: Axis,
    direction: Direction,
    offset: f32,
    count: f32,
}

impl Bars {
    pub fn new(p: &BarsParams, time: &EffectTime, colors: Colors) -> Self {
        let (_, offset) = time.cycles(p.speed);
        Self {
            colors,
            axis: p.axis,
            direction: p.direction,
            offset,
            count: p.count.clamp(1, MAX_REPEATS) as f32,
        }
    }
}

impl Shade for Bars {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let x = match self.axis {
            Axis::Horizontal => px.u,
            Axis::Vertical => px.v,
        };
        // Bars move toward higher x (right or up) going forward.
        let s = (flip(x, self.direction) - self.offset) * self.count;
        let bar = s.floor();
        if s - bar < 0.5 {
            Rgba::opaque(self.colors.get(bar.rem_euclid(self.count) as u64))
        } else {
            Rgba::CLEAR
        }
    }
}

pub struct Wave {
    colors: Colors,
    direction: Direction,
    cycles: f32,
    phase: f32,
    amplitude: f32,
    half_thickness: f32,
}

impl Wave {
    pub fn new(p: &WaveParams, time: &EffectTime, colors: Colors) -> Self {
        let (_, phase) = time.cycles(p.speed);
        Self {
            colors,
            direction: p.direction,
            cycles: sane(p.cycles, 1.0, 1e4),
            phase,
            amplitude: unit(p.height) / 2.0,
            half_thickness: unit(p.thickness) / 2.0,
        }
    }
}

impl Shade for Wave {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let x = flip(px.u, self.direction);
        let center = 0.5 + self.amplitude * (TAU * (self.cycles * x - self.phase)).sin();
        if (px.v - center).abs() <= self.half_thickness {
            Rgba::opaque(self.colors.ramp(px.u))
        } else {
            Rgba::CLEAR
        }
    }
}

pub struct Twinkle {
    colors: Colors,
    seed: u64,
    density: f32,
    whole: u64,
    fraction: f32,
}

impl Twinkle {
    pub fn new(p: &TwinkleParams, time: &EffectTime, colors: Colors, seed: u64) -> Self {
        let (whole, fraction) = time.cycles(p.rate);
        Self {
            colors,
            seed,
            density: unit(p.density),
            whole,
            fraction,
        }
    }
}

impl Shade for Twinkle {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        // Each pixel twinkles on its own schedule (a random phase), and in each of its cycles it
        // lights up (or not) at random, rising and falling smoothly.
        let i = u64::from(px.index);
        let c = self.fraction + hash01(self.seed, i, u64::MAX);
        let cycle = self.whole + c as u64;
        let f = c.fract();
        let roll = hash(self.seed, i, cycle);
        if ((roll >> 40) as f32 / (1u64 << 24) as f32) >= self.density {
            return Rgba::CLEAR;
        }
        let envelope = 1.0 - (2.0 * f - 1.0).abs();
        Rgba::with_alpha(self.colors.get(roll & 0xFFFF), envelope)
    }
}

pub struct Shimmer {
    color: [f32; 3],
    on: bool,
}

impl Shimmer {
    pub fn new(p: &ShimmerParams, time: &EffectTime, colors: Colors) -> Self {
        let (whole, fraction) = time.cycles(p.rate);
        Self {
            color: colors.get(whole),
            on: fraction < unit(p.duty),
        }
    }
}

impl Shade for Shimmer {
    #[inline]
    fn shade(&self, _px: &Pixel) -> Rgba {
        if self.on {
            Rgba::opaque(self.color)
        } else {
            Rgba::CLEAR
        }
    }
}

pub struct Strobe {
    colors: Colors,
    seed: u64,
    density: f32,
    flash: u64,
    lit: bool,
}

impl Strobe {
    pub fn new(p: &StrobeParams, time: &EffectTime, colors: Colors, seed: u64) -> Self {
        let (flash, fraction) = time.cycles(p.rate);
        Self {
            colors,
            seed,
            density: unit(p.density),
            flash,
            // Each flash is short: the first half of its slot.
            lit: fraction < 0.5,
        }
    }
}

impl Shade for Strobe {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        if !self.lit {
            return Rgba::CLEAR;
        }
        let roll = hash(self.seed, u64::from(px.index), self.flash);
        if ((roll >> 40) as f32 / (1u64 << 24) as f32) < self.density {
            Rgba::opaque(self.colors.get(roll & 0xFFFF))
        } else {
            Rgba::CLEAR
        }
    }
}

pub struct Spiral {
    colors: Colors,
    count: f32,
    rotation: f32,
    thickness: f32,
    twist: f32,
}

impl Spiral {
    pub fn new(p: &SpiralParams, time: &EffectTime, colors: Colors) -> Self {
        let (_, rotation) = time.cycles(p.speed);
        Self {
            colors,
            count: p.count.clamp(1, MAX_REPEATS) as f32,
            rotation: flip(rotation, p.direction),
            thickness: unit(p.thickness),
            twist: sane(p.twist, 1.0, 1e3),
        }
    }
}

impl Shade for Spiral {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let s = (px.u + self.twist * px.v - self.rotation) * self.count;
        let stripe = s.floor();
        if s - stripe < self.thickness {
            Rgba::opaque(self.colors.get(stripe.rem_euclid(self.count) as u64))
        } else {
            Rgba::CLEAR
        }
    }
}

// ---------------------------------------------------------------------------------------------

/// Fire simulation grid size.
const FIRE_COLUMNS: usize = 32;
const FIRE_ROWS: usize = 32;
/// Simulation step (independent of the sequence's frame time, so fire looks the same at any rate).
const FIRE_STEP_MS: u64 = 25;
/// Steps simulated for each frame. Heat older than this has cooled away, so starting from a cold
/// grid this many steps back gives the same flames as simulating from the start of the effect.
const FIRE_WARMUP_STEPS: u64 = FIRE_ROWS as u64 + 24;

/// Classic heat-diffusion fire on a small grid: sparks heat the bottom row, heat rises and
/// spreads, and every cell cools a little each step. Frame N is simulated from a cold grid a fixed
/// number of steps earlier, with randomness keyed to the step number, so it depends only on N.
pub struct Fire {
    heat: Box<[[u8; FIRE_COLUMNS]; FIRE_ROWS]>,
    height: f32,
}

impl Fire {
    pub fn new(p: &FireParams, time: &EffectTime, seed: u64) -> Self {
        let mut heat = Box::new([[0u8; FIRE_COLUMNS]; FIRE_ROWS]);
        let now = time.elapsed_ms / FIRE_STEP_MS;
        let sparks = unit(p.sparks);
        let cooling = 55u32 * 10 / FIRE_ROWS as u32 + 2;
        for step in now.saturating_sub(FIRE_WARMUP_STEPS - 1)..=now {
            let mut next = [[0u8; FIRE_COLUMNS]; FIRE_ROWS];
            for x in 0..FIRE_COLUMNS {
                let left = x.saturating_sub(1);
                let right = (x + 1).min(FIRE_COLUMNS - 1);
                for y in (0..FIRE_ROWS).rev() {
                    // Heat rises: each cell takes from the cells below it (and a little sideways).
                    let rising = if y >= 2 {
                        (u32::from(heat[y - 1][left])
                            + 2 * u32::from(heat[y - 1][x])
                            + u32::from(heat[y - 1][right])
                            + 2 * u32::from(heat[y - 2][x]))
                            / 6
                    } else if y == 1 {
                        (u32::from(heat[0][x]) * 2 + u32::from(heat[1][x])) / 3
                    } else {
                        u32::from(heat[0][x])
                    };
                    let cool = (hash(seed, step, (y * FIRE_COLUMNS + x) as u64) % u64::from(cooling)) as u32;
                    next[y][x] = rising.saturating_sub(cool).min(255) as u8;
                }
                // New sparks near the bottom.
                let roll = hash(seed ^ 0x5EED, step, x as u64);
                if ((roll >> 40) as f32 / (1u64 << 24) as f32) < sparks {
                    let y = ((roll >> 8) % 3) as usize;
                    let boost = 160 + (roll >> 16) % 96;
                    next[y][x] = (u64::from(next[y][x]) + boost).min(255) as u8;
                }
            }
            *heat = next;
        }
        Self {
            heat,
            height: unit(p.height),
        }
    }

    /// The fire's heat (0–255) at a grid cell, bottom row first; for tests.
    pub fn heat(&self, column: usize, row: usize) -> u8 {
        self.heat[row][column]
    }
}

/// Black → red → yellow → white.
fn heat_color(heat: u8) -> Rgba {
    let t = f32::from(heat) / 255.0;
    let (r, g, b) = (unit(3.0 * t), unit(3.0 * t - 1.0), unit(3.0 * t - 2.0));
    let a = r.max(g).max(b);
    if a <= 0.0 {
        Rgba::CLEAR
    } else {
        Rgba::new(r / a, g / a, b / a, a)
    }
}

impl Shade for Fire {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        if self.height <= 0.0 || px.v > self.height {
            return Rgba::CLEAR;
        }
        let row = ((px.v / self.height) * FIRE_ROWS as f32) as usize;
        let column = (px.u * FIRE_COLUMNS as f32) as usize;
        heat_color(self.heat[row.min(FIRE_ROWS - 1)][column.min(FIRE_COLUMNS - 1)])
    }
}

/// One meteor's place this frame (in travel/cross coordinates).
#[derive(Debug, Clone, Copy)]
struct Meteor {
    head: f32,
    lane: f32,
    color: [f32; 3],
}

pub struct Meteors {
    meteors: Vec<Meteor>,
    direction: MeteorDirection,
    length: f32,
    half_lane: f32,
}

impl Meteors {
    pub fn new(p: &MeteorsParams, time: &EffectTime, colors: Colors, seed: u64, canvas: Canvas) -> Self {
        // On a flat target (a horizontal line), fall along it instead of across it.
        let direction = match (p.direction, canvas.rows <= 1 && canvas.columns > 1) {
            (MeteorDirection::Down, true) => MeteorDirection::Left,
            (MeteorDirection::Up, true) => MeteorDirection::Right,
            (MeteorDirection::Left, _) | (MeteorDirection::Right, _)
                if canvas.columns <= 1 && canvas.rows > 1 =>
            {
                if p.direction == MeteorDirection::Left {
                    MeteorDirection::Down
                } else {
                    MeteorDirection::Up
                }
            }
            (d, _) => d,
        };
        let lanes = match direction {
            MeteorDirection::Down | MeteorDirection::Up => canvas.columns,
            MeteorDirection::Left | MeteorDirection::Right => canvas.rows,
        }
        .max(1);
        let length = unit(p.length).max(0.01);
        let speed = sane(p.speed, 1.0, 1e4).abs().max(1e-3);
        // Each meteor crosses (plus its tail) once per period, starting at its own random time.
        let period = f64::from(1.0 + length) / f64::from(speed);
        let meteors = (0..p.count.min(MAX_METEORS))
            .map(|k| {
                let k = u64::from(k);
                let c = time.seconds() / period + f64::from(hash01(seed, k, u64::MAX));
                let trip = c.floor() as u64;
                let lane = (hash01(seed, k, trip) * lanes as f32).floor();
                Meteor {
                    head: c.fract() as f32 * (1.0 + length),
                    lane: (lane + 0.5) / lanes as f32,
                    color: colors.get(k),
                }
            })
            .collect();
        Self {
            meteors,
            direction,
            length,
            half_lane: 0.5 / lanes as f32,
        }
    }
}

impl Shade for Meteors {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let (travel, cross) = match self.direction {
            MeteorDirection::Down => (1.0 - px.v, px.u),
            MeteorDirection::Up => (px.v, px.u),
            MeteorDirection::Left => (1.0 - px.u, px.v),
            MeteorDirection::Right => (px.u, px.v),
        };
        let mut best = Rgba::CLEAR;
        for m in &self.meteors {
            if (cross - m.lane).abs() > self.half_lane {
                continue;
            }
            // Behind the head, within the tail: brightest at the head.
            let behind = m.head - travel;
            if (0.0..=self.length).contains(&behind) {
                let level = 1.0 - behind / self.length;
                if level > best.a {
                    best = Rgba::with_alpha(m.color, level);
                }
            }
        }
        best
    }
}

pub struct Ripple {
    colors: Colors,
    front: f32,
    spacing: f32,
    half_thickness: f32,
}

impl Ripple {
    pub fn new(p: &RippleParams, time: &EffectTime, colors: Colors) -> Self {
        let speed = sane(p.speed, 0.5, 1e4).abs();
        Self {
            colors,
            front: (time.seconds() * f64::from(speed)) as f32,
            spacing: sane(p.spacing, 0.4, 1e4).abs().max(0.01),
            half_thickness: unit(p.thickness).max(0.001) / 2.0,
        }
    }
}

impl Shade for Ripple {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        // Distance from the center, 1.0 at the corners.
        let (dx, dy) = (px.u - 0.5, px.v - 0.5);
        let r = (dx * dx + dy * dy).sqrt() / std::f32::consts::FRAC_1_SQRT_2;
        // Ring j was born j * spacing ago (in distance) and is now at front - j * spacing.
        let x = self.front - r;
        if x < -self.half_thickness {
            return Rgba::CLEAR;
        }
        let ring = (x / self.spacing).round().max(0.0);
        let off = (x - ring * self.spacing).abs();
        if off <= self.half_thickness {
            Rgba::with_alpha(
                self.colors.get(ring as u64),
                1.0 - off / self.half_thickness * 0.5,
            )
        } else {
            Rgba::CLEAR
        }
    }
}

// ---------------------------------------------------------------------------------------------

/// Any effect, ready to shade pixels for one frame.
pub enum Shader {
    On(On),
    Off(Off),
    ColorWash(ColorWash),
    Fade(Fade),
    Chase(Chase),
    Bars(Bars),
    Wave(Wave),
    Twinkle(Twinkle),
    Shimmer(Shimmer),
    Strobe(Strobe),
    Spiral(Spiral),
    Fire(Fire),
    Meteors(Meteors),
    Ripple(Ripple),
}

impl Shader {
    /// Prepares `params` for one frame.
    pub fn new(params: &EffectParams, time: &EffectTime, colors: Colors, seed: u64, canvas: Canvas) -> Self {
        match params {
            EffectParams::On(p) => Shader::On(On::new(p, time, colors)),
            EffectParams::Off(_) => Shader::Off(Off),
            EffectParams::ColorWash(p) => Shader::ColorWash(ColorWash::new(p, time, colors)),
            EffectParams::Fade(p) => Shader::Fade(Fade::new(p, time, colors)),
            EffectParams::Chase(p) => Shader::Chase(Chase::new(p, time, colors)),
            EffectParams::Bars(p) => Shader::Bars(Bars::new(p, time, colors)),
            EffectParams::Wave(p) => Shader::Wave(Wave::new(p, time, colors)),
            EffectParams::Twinkle(p) => Shader::Twinkle(Twinkle::new(p, time, colors, seed)),
            EffectParams::Shimmer(p) => Shader::Shimmer(Shimmer::new(p, time, colors)),
            EffectParams::Strobe(p) => Shader::Strobe(Strobe::new(p, time, colors, seed)),
            EffectParams::Spiral(p) => Shader::Spiral(Spiral::new(p, time, colors)),
            EffectParams::Fire(p) => Shader::Fire(Fire::new(p, time, seed)),
            EffectParams::Meteors(p) => Shader::Meteors(Meteors::new(p, time, colors, seed, canvas)),
            EffectParams::Ripple(p) => Shader::Ripple(Ripple::new(p, time, colors)),
        }
    }

    /// Runs `each` with a concrete shader, so per-pixel loops are compiled for each effect.
    #[inline]
    pub(crate) fn with<R>(&self, each: impl ShaderVisitor<R>) -> R {
        match self {
            Shader::On(s) => each.visit(s),
            Shader::Off(s) => each.visit(s),
            Shader::ColorWash(s) => each.visit(s),
            Shader::Fade(s) => each.visit(s),
            Shader::Chase(s) => each.visit(s),
            Shader::Bars(s) => each.visit(s),
            Shader::Wave(s) => each.visit(s),
            Shader::Twinkle(s) => each.visit(s),
            Shader::Shimmer(s) => each.visit(s),
            Shader::Strobe(s) => each.visit(s),
            Shader::Spiral(s) => each.visit(s),
            Shader::Fire(s) => each.visit(s),
            Shader::Meteors(s) => each.visit(s),
            Shader::Ripple(s) => each.visit(s),
        }
    }
}

impl Shade for Shader {
    fn shade(&self, px: &Pixel) -> Rgba {
        struct One<'a>(&'a Pixel);
        impl ShaderVisitor<Rgba> for One<'_> {
            fn visit<S: Shade>(self, shader: &S) -> Rgba {
                shader.shade(self.0)
            }
        }
        self.with(One(px))
    }
}

/// Something done with a concrete shader type.
pub(crate) trait ShaderVisitor<R> {
    fn visit<S: Shade>(self, shader: &S) -> R;
}

/// One effect's color at one pixel: `(t_norm, elapsed_ms, pixel, params, palette) -> Rgba`,
/// with the effect id's `seed` for randomness and the target's `canvas` size.
pub fn shade_pixel(
    params: &EffectParams,
    time: EffectTime,
    px: &Pixel,
    palette: &[pf_sequence::Rgb],
    seed: u64,
    canvas: Canvas,
) -> Rgba {
    Shader::new(params, &time, Colors::new(palette), seed, canvas).shade(px)
}
