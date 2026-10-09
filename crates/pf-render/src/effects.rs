//! The effects. Each is a pure function of the effect's time, one pixel, its settings, and its
//! palette (plus the effect id as a random seed), so any frame can be rendered on its own:
//! seeking and export give the same picture every time.
//!
//! Time-dependent values are worked out once per frame (in `new`, in double precision so long
//! effects stay smooth), then [`Shade::shade`] runs for every pixel.
//!
//! Settings are clamped to the ranges in each kind's settings table (`pf_sequence`, see
//! `EffectParams::sanitize`) before an effect is drawn, so the settings panel, file loading, and
//! the renderer all agree on what a setting can be. The constructors below rely on that.

use crate::audio::{Audio, RenderContext};
pub use crate::butterfly::Butterfly;
pub use crate::circles::Circles;
use crate::color::{Colors, Rgba, unit};
pub use crate::fan::Fan;
pub use crate::garlands::Garlands;
use crate::geometry::Pixel;
pub use crate::life::Life;
pub use crate::lines::Lines;
pub use crate::morph::Morph;
pub use crate::pinwheel::Pinwheel;
pub use crate::plasma::Plasma;
pub use crate::shape::Shape;
pub use crate::snowflakes::Snowflakes;
pub use crate::tendril::Tendril;
pub use crate::text::Text;
pub use crate::vumeter::VuMeter;
use pf_sequence::{
    Axis, BarsParams, ChaseParams, ColorWashParams, Direction, EffectParams, FadeDirection, FadeParams,
    FireParams, Gradient, MeteorDirection, MeteorsParams, OnParams, RippleParams, ShapeParams, ShimmerParams,
    SpiralParams, StrobeParams, TwinkleParams, WaveParams,
};
use std::f32::consts::TAU;

/// The frame time assumed when none is given (xLights' usual 50 ms, 20 frames a second).
pub const DEFAULT_FRAME_MS: u32 = 50;

/// Most meteors drawn at once on one target (the top of the Meteors "count" setting's range).
pub const MAX_METEORS: u32 = 100;

/// Where an effect is in its own time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectTime {
    /// 0.0 at the start of the effect, approaching 1.0 at its end.
    pub t_norm: f32,
    /// Milliseconds since the effect started.
    pub elapsed_ms: u64,
    /// The effect's length in milliseconds (at least 1).
    pub length_ms: u64,
    /// The sequence's frame time, for effects xLights moves a step a frame (Plasma, Life).
    pub frame_ms: u32,
    /// When the effect started, in milliseconds from the start of the sequence.
    pub start_ms: u64,
}

impl EffectTime {
    /// The time `t_ms` within an effect running from `start_ms` to `end_ms`.
    pub fn within(start_ms: u64, end_ms: u64, t_ms: u64) -> Self {
        let length = end_ms.saturating_sub(start_ms).max(1);
        let elapsed_ms = t_ms.saturating_sub(start_ms);
        Self {
            t_norm: (elapsed_ms as f64 / length as f64).clamp(0.0, 1.0) as f32,
            elapsed_ms,
            length_ms: length,
            frame_ms: DEFAULT_FRAME_MS,
            start_ms,
        }
    }

    /// The same time in a sequence with `frame_ms` frames.
    pub fn with_frame_ms(self, frame_ms: u32) -> Self {
        Self {
            frame_ms: frame_ms.max(1),
            ..self
        }
    }

    /// Whole frames since the effect started (xLights' `curPeriod - curEffStartPer`).
    pub(crate) fn frame(&self) -> u64 {
        self.elapsed_ms / u64::from(self.frame_ms.max(1))
    }

    /// Frames the effect lasts, counting its first and last (xLights' `curEffEndPer -
    /// curEffStartPer + 1`).
    pub(crate) fn frames(&self) -> u64 {
        (self.length_ms.max(1) - 1) / u64::from(self.frame_ms.max(1)) + 1
    }

    /// xLights' `GetEffectTimeIntervalPosition(cycles)`: where the effect is in its current cycle
    /// (0–1), counting in frames, with `cycles` cycles over the effect.
    pub(crate) fn cycle_position(&self, cycles: f64) -> f64 {
        let periods = self.frames() as f64;
        if periods <= 1.0 {
            return 0.0;
        }
        let per_cycle = periods / cycles;
        if per_cycle.is_nan() || per_cycle <= 1.0 || per_cycle.is_infinite() {
            return 0.0;
        }
        let at = (self.frame() as f64).rem_euclid(per_cycle);
        (at / (per_cycle - 1.0)).min(1.0)
    }

    pub(crate) fn seconds(&self) -> f64 {
        self.elapsed_ms as f64 / 1000.0
    }

    /// How many times something happening `per_second` times a second has happened: whole
    /// count and fraction of the current one.
    fn cycles(&self, per_second: f32) -> (u64, f32) {
        let x = self.seconds() * f64::from(per_second.max(0.0));
        (x.floor() as u64, x.fract() as f32)
    }
}

/// The size of the target being drawn, for effects that work on a grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Canvas {
    pub columns: u32,
    pub rows: u32,
}

/// Deterministic randomness: a well-mixed 64-bit hash of a seed and two numbers.
#[inline]
pub(crate) fn hash(seed: u64, a: u64, b: u64) -> u64 {
    let mut z = seed ^ a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A deterministic stream of random numbers (splitmix64), for effects that draw many in turn
/// (xLights' `randInt` and `rand01`). Seed it from [`hash`] of the effect's seed and what the
/// numbers are for, so each frame's numbers depend only on the document.
#[derive(Debug, Clone)]
pub(crate) struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A fraction in `0.0..1.0` (`rand01`).
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A whole number from `lo` to `hi`, both included (`randInt`).
    pub fn int(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        let span = (i64::from(hi) - i64::from(lo) + 1) as u64;
        (i64::from(lo) + (self.next() % span) as i64) as i32
    }
}

/// [`hash`] as a fraction in `0.0..1.0`.
#[inline]
pub(crate) fn hash01(seed: u64, a: u64, b: u64) -> f32 {
    (hash(seed, a, b) >> 40) as f32 / (1u64 << 24) as f32
}

/// xLights' acceleration (`RenderBuffer::calcAccel`): bends progress through an effect (0–1) so
/// it speeds up (positive, up to 10) or slows down (negative).
pub(crate) fn accelerate(ratio: f64, accel: f64) -> f64 {
    if accel == 0.0 || !accel.is_finite() {
        return ratio;
    }
    let pct = (accel.abs() - 1.0) / 9.0;
    let a1 = pct * 5.0 + (1.0 - pct) * 1.5;
    let a2 = 1.5 + ratio * a1;
    let exponent = pct * a2 + (1.0 - pct) * a1;
    if accel > 0.0 {
        ratio.powf(exponent)
    } else {
        1.0 - (1.0 - ratio).powf(a1)
    }
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
        Self {
            colors,
            gradient: p.gradient,
            position: (f64::from(time.t_norm) * f64::from(p.cycles)).rem_euclid(2.0) as f32,
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
            bands: p.bands.max(1) as f32,
            width: p.width,
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
            count: p.count.max(1) as f32,
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
            cycles: p.cycles,
            phase,
            amplitude: p.height / 2.0,
            half_thickness: p.thickness / 2.0,
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
            count: p.count.max(1) as f32,
            rotation: flip(rotation, p.direction),
            thickness: p.thickness,
            twist: p.twist,
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
        let length = p.length.max(0.01);
        let speed = p.speed.max(0.01);
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
    /// Looks at every meteor for every pixel: at most [`MAX_METEORS`] cheap lane checks (about
    /// 0.3 ms per 100k pixels in the benchmark), so lanes aren't bucketed.
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
    /// How far the first ring has travelled, capped (beyond the corners every pixel is reached).
    front: f32,
    /// Rings born so far, and where the newest one is: worked out in double precision, so rings
    /// stay crisp hours into an effect (an f32 front loses the ring thickness after a while).
    born: u64,
    phase: f32,
    spacing: f32,
    half_thickness: f32,
}

impl Ripple {
    pub fn new(p: &RippleParams, time: &EffectTime, colors: Colors) -> Self {
        let front = time.seconds() * f64::from(p.speed.max(0.0));
        let spacing = f64::from(p.spacing.max(0.01));
        let born = (front / spacing).floor();
        Self {
            colors,
            front: front.min(8.0) as f32,
            born: born as u64,
            phase: (front - born * spacing) as f32,
            spacing: spacing as f32,
            half_thickness: p.thickness.max(0.001) / 2.0,
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
        if self.front - r < -self.half_thickness {
            return Rgba::CLEAR;
        }
        // The nearest ring j sits at front - j * spacing = phase - k * spacing, j = born + k.
        let k = ((self.phase - r) / self.spacing).round();
        let j = self.born as i64 + k as i64;
        let (ring, off) = if j < 0 {
            // Early on (`front` is exact then): the first ring is the nearest.
            (0, (self.front - r).abs())
        } else {
            (j as u64, (self.phase - r - k * self.spacing).abs())
        };
        if off <= self.half_thickness {
            Rgba::with_alpha(self.colors.get(ring), 1.0 - off / self.half_thickness * 0.5)
        } else {
            Rgba::CLEAR
        }
    }
}

// ---------------------------------------------------------------------------------------------

/// The Faces effect for one frame: the color of each lit pixel, by its place in the target's
/// buffer. The renderer works it out (it knows the face and the timing track, see `faces.rs`);
/// made from the settings alone it lights nothing.
#[derive(Debug, Clone, Default)]
pub struct Faces {
    lit: Vec<Option<Rgba>>,
}

impl Faces {
    pub(crate) fn new(lit: Vec<Option<Rgba>>) -> Self {
        Self { lit }
    }

    /// The lit pixels back, so the renderer can reuse the memory next frame.
    pub(crate) fn into_lit(self) -> Vec<Option<Rgba>> {
        self.lit
    }
}

impl Shade for Faces {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        self.lit
            .get(px.index as usize)
            .copied()
            .flatten()
            .unwrap_or(Rgba::CLEAR)
    }
}

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
    Shape(Shape),
    Fan(Fan),
    Morph(Morph),
    Circles(Circles),
    Pinwheel(Pinwheel),
    Snowflakes(Snowflakes),
    Plasma(Plasma),
    Butterfly(Butterfly),
    Garlands(Garlands),
    Lines(Lines),
    Life(Life),
    Tendril(Tendril),
    Text(Text),
    Faces(Faces),
    VuMeter(VuMeter),
}

/// When a Shape fires its shapes (ms from the effect's start, while it plays): at each mark on its
/// timing track, or with the music; `None` when it keeps `count` shapes shown.
fn shape_marks(p: &ShapeParams, time: &EffectTime, cx: &RenderContext) -> Option<Vec<u64>> {
    let (start, end) = (time.start_ms, time.start_ms + time.length_ms);
    if let Some(track) = p.timing_track {
        let marks = cx.marks(Some(track)).unwrap_or_default();
        return Some(
            marks
                .iter()
                .filter(|m| (start..end).contains(&m.start_ms))
                .map(|m| m.start_ms - start)
                .collect(),
        );
    }
    p.fire_on_music.then(|| {
        cx.audio
            .map(|audio| music_marks(p, time, audio))
            .unwrap_or_default()
    })
}

/// xLights' Shape "Fire with music": a shape when the music's peak passes the trigger level, and
/// again every 21 frames while it stays above (`REPEATTRIGGER`), up to the frame playing.
fn music_marks(p: &ShapeParams, time: &EffectTime, audio: Audio) -> Vec<u64> {
    let frame_ms = u64::from(time.frame_ms.max(1));
    let trigger = p.trigger_level / 100.0;
    let mut since = 0;
    let mut marks = Vec::new();
    for frame in time.start_ms / frame_ms..=(time.start_ms + time.elapsed_ms) / frame_ms {
        if audio.peak(frame) > trigger {
            if since == 0 || since > 20 {
                marks.push((frame * frame_ms).saturating_sub(time.start_ms));
            }
            since += 1;
            if since > 20 {
                since = 0;
            }
        } else {
            since = 0;
        }
    }
    marks
}

impl Shader {
    /// Prepares `params` for one frame, clamped to their kind's settings table first, without
    /// the music or timing tracks (see [`Shader::in_context`]).
    pub fn new(params: &EffectParams, time: &EffectTime, colors: Colors, seed: u64, canvas: Canvas) -> Self {
        Self::in_context(params, time, colors, seed, canvas, &RenderContext::default())
    }

    /// [`Shader::new`] reading the music and timing tracks the effect follows from `cx`.
    pub fn in_context(
        params: &EffectParams,
        time: &EffectTime,
        colors: Colors,
        seed: u64,
        canvas: Canvas,
        cx: &RenderContext,
    ) -> Self {
        match &params.sanitized() {
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
            // Shapes on a timing track or fired by the music appear when it says (none without
            // it).
            EffectParams::Shape(p) => {
                let marks = shape_marks(p, time, cx);
                Shader::Shape(Shape::new(p, time, colors, seed, canvas, marks.as_deref()))
            }
            EffectParams::Fan(p) => Shader::Fan(Fan::new(p, time, colors, canvas)),
            EffectParams::Morph(p) => Shader::Morph(Morph::new(p, time, colors, canvas)),
            EffectParams::Circles(p) => Shader::Circles(Circles::new(p, time, colors, seed, canvas)),
            EffectParams::Pinwheel(p) => Shader::Pinwheel(Pinwheel::new(p, time, colors, canvas)),
            EffectParams::Snowflakes(p) if p.motion == pf_sequence::SnowflakesMotion::Blowing => {
                Shader::Snowflakes(Snowflakes::blowing(p, time, colors, seed, canvas))
            }
            EffectParams::Plasma(p) => Shader::Plasma(Plasma::new(p, time, colors, canvas)),
            EffectParams::Butterfly(p) => Shader::Butterfly(Butterfly::new(p, time, colors, canvas)),
            EffectParams::Garlands(p) => Shader::Garlands(Garlands::new(p, time, colors, canvas)),
            EffectParams::Text(p) => Shader::Text(Text::new(p, time, colors, canvas)),
            // Worked out frame by frame from the first; the renderer keeps their state between
            // frames instead (see `sim.rs`).
            p @ (EffectParams::Snowflakes(_)
            | EffectParams::Lines(_)
            | EffectParams::Life(_)
            | EffectParams::Tendril(_)
            | EffectParams::VuMeter(_)) => {
                crate::sim::run(p, time, colors, seed, canvas, cx).unwrap_or(Shader::Off(Off))
            }
            EffectParams::Faces(_) => Shader::Faces(Faces::default()),
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
            Shader::Shape(s) => each.visit(s),
            Shader::Fan(s) => each.visit(s),
            Shader::Morph(s) => each.visit(s),
            Shader::Circles(s) => each.visit(s),
            Shader::Pinwheel(s) => each.visit(s),
            Shader::Snowflakes(s) => each.visit(s),
            Shader::Plasma(s) => each.visit(s),
            Shader::Butterfly(s) => each.visit(s),
            Shader::Garlands(s) => each.visit(s),
            Shader::Lines(s) => each.visit(s),
            Shader::Life(s) => each.visit(s),
            Shader::Tendril(s) => each.visit(s),
            Shader::Text(s) => each.visit(s),
            Shader::Faces(s) => each.visit(s),
            Shader::VuMeter(s) => each.visit(s),
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
