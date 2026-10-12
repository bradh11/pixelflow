//! The Plasma effect: soft blobs of color drifting and merging.
//!
//! The picture is seven waves added together. Each is the sine of an angle that depends on
//! where a cell is on the picture and on a clock, and their sum, times the line density, is an
//! angle round the color cycle. Every pixel is answered from its own cell with a fixed handful
//! of sums. Nothing is kept for a row, a column, or a frame, and nothing is allocated.
//!
//! **The picture is laid over the buffer.** Its left column is 0 and its right column 1, its
//! bottom row 0 and its top row 1, whatever the buffer's shape, and a pixel takes the color of
//! the cell it falls in (the cells blur works on: [`grid_size`] and [`cell_of`]).
//!
//! **The waves.** With `x` and `y` the cell's place and `t` the clock, in radians:
//!
//! - four swells, far longer than the buffer: `x + t` across the width, `y/2 + t/2` up the
//!   height, `x/2 + y/2 + t/2` along the diagonal, and `√(x² + y²) + t` out from the bottom-left
//!   corner;
//! - two ripples, a wave and a half to the buffer: `10x + t` across the width, and
//!   `10(gx + hy) + t`, whose tilt `(g, h) = (sin t/2, cos t/3)` swings round as the clock runs;
//! - one set of rings, `√(d²/l² + t)`, where `d` is the distance from a center that wanders
//!   about the bottom-left corner, at `(-½ sin t/5, -½ cos t/3)`, and `l² = 0.02 / twist`.
//!
//! **Angles are whole numbers.** A full turn is 2^64 steps, so an angle wraps by itself and
//! nothing has to be brought back into range. Five of the seven angles climb by the same amount
//! from each column to the next and from each row to the next, so they are the cell's column
//! and row multiplied by whole numbers and added to the clock's angle: exact, at any distance
//! from the corner and however long the effect has run. The other two take a square root first.
//!
//! **One sine, in two multiplications.** A turn is cut into 512 arcs. An angle's top bits name
//! its arc and the bits below say how far along the arc it is, and across one arc the sine is
//! very nearly a parabola in that: three numbers for each arc ([`ARCS_OF_SINE`]), from the
//! sine's own series about the middle of the arc. The result is within a hundred-millionth of
//! the true sine. On a channel that comes to a few hundred-thousandths of one step of 255 at
//! the settings shows use, and to four thousandths of a step at the very worst (all seven waves
//! off the same way, the top line density, and 32 colors that are black and white by turns).
//! No sine comes from the system's maths library, in a pixel or in a frame, so a pixel gets the
//! same bits on every machine.
//!
//! **Time is counted in frames.** The clock advances one tick a frame, `1 / (303 - 3 × speed)`
//! radians with only the whole part of the speed counted, and the first frame is already one
//! tick in. It is worked out once a frame in double precision, as is everything else that is the
//! same for every pixel: the tilt, the rings' center, and each angle's climb from cell to cell.
//! A frame depends on nothing but the settings, the palette, the target's size, and how many
//! frames have gone by.
//!
//! **Color.** Each unit of the sum is a quarter turn of color, times the line density. A
//! channel is the raised sine `(1 + sin a) / 2` of that angle, a quarter or a third of a turn
//! ahead for the second and third channels of the fixed colorings. The palette is run there and
//! back along the raised sine a third of a turn ahead, its colors at equal steps with a
//! straight-line blend between neighbors; a palette of one color has nothing to run between and
//! is a flat fill.

use crate::color::{Colors, Rgba, unit};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use crate::raster::{cell_of, grid_size};
use pf_sequence::{MAX_PALETTE_COLORS, PlasmaColors, PlasmaParams};
use std::f64::consts::TAU;

/// Steps of angle in a full turn: an angle is a `u64` that wraps by itself.
const TURN: f64 = 18_446_744_073_709_551_616.0;
const HALF_TURN: u64 = 1 << 63;
const QUARTER_TURN: u64 = 1 << 62;
const EIGHTH_TURN: u64 = 1 << 61;
/// A third of a turn, to the nearest step.
const THIRD_TURN: u64 = 0x5555_5555_5555_5555;
const RADIANS_PER_STEP: f64 = TAU / TURN;

/// A turn is cut into `2^ARC_BITS` arcs for [`sine`]: an angle's top bits name its arc.
const ARC_BITS: u32 = 9;
const ARCS: usize = 1 << ARC_BITS;
/// How far along its arc an angle is, is read off the next `ALONG_BITS` bits: as many as a
/// double holds exactly, so the whole arc is this many.
const ALONG_BITS: u32 = 53;
const WHOLE_ARC: f64 = (1u64 << ALONG_BITS) as f64;

/// An angle that comes out of a square root is counted `2^COARSE_BITS` steps at a time, so that
/// it may be many turns long. That is far finer than anything that shows (a turn is 2^40 of
/// them), and the angle may reach a hundred million radians, which the clock passes after
/// longer than any sequence lasts.
const COARSE_BITS: u32 = 24;
const COARSE_PER_RADIAN: f64 = TURN / TAU / (1u64 << COARSE_BITS) as f64;

/// The sum of the waves reaches 7 either way and the line density 10: seventy quarter turns.
/// The color's angle is counted `2^SUM_BITS` steps at a time so that many fit.
const SUM_BITS: u32 = 6;
const SUM_PER_QUARTER_TURN: f64 = (QUARTER_TURN >> SUM_BITS) as f64;

/// The clock takes this many frames to advance a radian at speed 0, and three fewer for each
/// unit of speed.
const SLOWEST: u32 = 303;
const TOP_SPEED: f32 = 100.0;

/// The ripples' angle climbs this many radians across the picture.
const RIPPLE: f64 = 10.0;
/// One over the square of the rings' length scale, at twist 1.
const RINGS: f64 = 50.0;

/// The sine across one arc: `a + along × (b + along × c)` for its three numbers, with `along`
/// how far along the arc the angle is, counted as [`sine`] counts it. Kept to 32 bytes, so an
/// arc is found with one shift and lies in one line of the cache.
#[derive(Clone, Copy)]
#[repr(align(32))]
struct Parabola([f64; 3]);

static ARCS_OF_SINE: [Parabola; ARCS] = arcs();

/// For each arc, the sine's series about the middle of the arc, as far as the square of the
/// distance from the middle: `s + c d - s d²/2`, with `s` and `c` the sine and cosine there.
/// The term left out, `-c d³/6`, grows toward the arc's two ends. Easing the slope by an eighth
/// of the half-arc squared spreads it evenly along the arc instead, a quarter the size:
/// `(π / 512)³ / 24`, under a hundred-millionth. The result is then written out in terms of how
/// far along the arc the angle is, from its start.
const fn arcs() -> [Parabola; ARCS] {
    // Half an arc, in steps and in radians.
    let half_steps = 1u64 << (63 - ARC_BITS);
    let half = half_steps as f64 * RADIANS_PER_STEP;
    let mut arcs = [Parabola([0.0; 3]); ARCS];
    let mut k = 0;
    while k < ARCS {
        let middle = (2 * k as u64 + 1) * half_steps;
        let (sine, cosine) = (true_sine(middle), true_cosine(middle));
        let slope = cosine * (1.0 - half * half / 8.0);
        // With `d = 2 × half × along - half`, and `along` from 0 to 1 counted as `WHOLE_ARC`.
        arcs[k] = Parabola([
            sine * (1.0 - half * half / 2.0) - slope * half,
            (slope + sine * half) * (2.0 * half) / WHOLE_ARC,
            -sine * (2.0 * half * half) / (WHOLE_ARC * WHOLE_ARC),
        ]);
        k += 1;
    }
    arcs
}

/// The sine of an angle in steps, within a hundred-millionth: the parabola of the arc the angle
/// is on, at how far along the arc it is.
#[inline(always)]
fn sine(angle: u64) -> f64 {
    let Parabola(arc) = &ARCS_OF_SINE[(angle >> (64 - ARC_BITS)) as usize];
    let along = ((angle >> (64 - ARC_BITS - ALONG_BITS)) & ((1 << ALONG_BITS) - 1)) as i64 as f64;
    arc[0] + along * (arc[1] + along * arc[2])
}

/// The terms of the sine's series, `z - z³/3! + z⁵/5! - …`, as what multiplies `z` by powers of
/// `z²`, and of the cosine's, `1 - z²/2! + z⁴/4! - …`. Within an eighth of a turn, eleven terms
/// carry either past the last bit of a double.
const TERMS: usize = 11;
const SERIES: [[f64; TERMS]; 2] = {
    let mut series = [[1.0; TERMS]; 2];
    let mut n = 1;
    while n < TERMS {
        // Each term is the one before over the next two whole numbers, with the sign turned.
        let (odd, even) = (2 * n + 1, 2 * n);
        series[0][n] = -series[0][n - 1] / (even * odd) as f64;
        series[1][n] = -series[1][n - 1] / ((even - 1) * even) as f64;
        n += 1;
    }
    series
};

/// The sine of an angle in steps, to the last bit or two of double precision: for what is worked
/// out once a frame, where an error would lean on every pixel alike, and for the arcs. The
/// angle is folded onto the first eighth of a turn in whole steps, so the result is exactly as
/// symmetric as the sine is, then summed from the series.
const fn true_sine(angle: u64) -> f64 {
    // Past half a turn the sine is the negative of the one half a turn back; past a quarter it
    // mirrors about the quarter; past an eighth it is the cosine of what is left to the quarter.
    let within = angle % HALF_TURN;
    let within = if within > QUARTER_TURN {
        HALF_TURN - within
    } else {
        within
    };
    let (z, terms) = if within <= EIGHTH_TURN {
        (within as f64 * RADIANS_PER_STEP, &SERIES[0])
    } else {
        ((QUARTER_TURN - within) as f64 * RADIANS_PER_STEP, &SERIES[1])
    };
    let squared = z * z;
    let mut sum = 0.0;
    let mut n = TERMS;
    while n > 0 {
        n -= 1;
        sum = terms[n] + squared * sum;
    }
    let value = if within <= EIGHTH_TURN { z * sum } else { sum };
    if angle > HALF_TURN { -value } else { value }
}

/// The cosine of an angle in steps: the sine a quarter turn on.
const fn true_cosine(angle: u64) -> f64 {
    true_sine(angle.wrapping_add(QUARTER_TURN))
}

/// An angle in radians as steps, any number of turns either way. An angle the other way is the
/// same number of steps exactly, taken back.
fn steps(radians: f64) -> u64 {
    let turns = (radians / TAU).abs();
    let steps = ((turns - turns.floor()) * TURN) as u64;
    if radians < 0.0 {
        steps.wrapping_neg()
    } else {
        steps
    }
}

/// An angle counted `2^COARSE_BITS` steps at a time (and never negative) as steps, to the whole
/// number of them below.
#[inline(always)]
fn coarse_steps(coarse: f64) -> u64 {
    (coarse as u64) << COARSE_BITS
}

/// What a pixel is painted with.
enum Paint {
    /// One color everywhere: a palette of a single color.
    Flat,
    /// The palette, there and back.
    Palette,
    RedGreen,
    BlueGreen,
    Rainbow,
    White,
}

pub struct Plasma {
    columns: i32,
    rows: i32,
    /// The clock as an angle: the whole of it, and half.
    turned: u64,
    half_turned: u64,
    /// How far each wave's angle climbs from one column to the next (and, where it does, from
    /// one row to the next), in steps.
    width_swell: u64,
    half_swell: [u64; 2],
    width_ripple: u64,
    swinging_ripple: [u64; 2],
    /// The two waves that go by distance, with every length counted so that the distance comes
    /// out of the square root already in coarse steps of angle. For the corner swell: what one
    /// cell across, and one cell up, is.
    corner_cell: [f64; 2],
    /// For the rings: the same, where their center is, and the clock that goes under the root
    /// with the distance squared.
    rings_cell: [f64; 2],
    rings_center: [f64; 2],
    rings_clock: f64,
    /// What one unit of the waves' sum is of the color cycle, as [`Plasma::color_angle`] counts
    /// it.
    bands: f64,
    paint: Paint,
    /// The palette's colors, at equal steps from 0 to `2 × half_palette`, with the last color
    /// again after that.
    palette: [[f32; 3]; MAX_PALETTE_COLORS],
    half_palette: f64,
}

impl Plasma {
    /// Everything that is the same for every pixel of one frame.
    pub fn new(p: &PlasmaParams, time: &EffectTime, colors: Colors, canvas: Canvas) -> Self {
        let (columns, rows) = grid_size(canvas);
        // The first cell is at 0 and the last at 1; a single cell is at 0.
        let cell = |cells: i32| {
            if cells > 1 {
                1.0 / f64::from(cells - 1)
            } else {
                0.0
            }
        };
        let cell = [cell(columns), cell(rows)];

        // Only the whole part of the speed counts, and a speed that is not a number counts as
        // none (the cast takes both).
        let speed = p.speed.clamp(0.0, TOP_SPEED) as u32;
        let clock = (time.frame() as f64 + 1.0) / f64::from(SLOWEST - 3 * speed);

        let tilt = [true_sine(steps(clock / 2.0)), true_cosine(steps(clock / 3.0))];
        let center = [-0.5 * true_sine(steps(clock / 5.0)), -0.5 * tilt[1]];

        // An angle that climbs `radians` over the picture's width (or height) climbs this many
        // steps from one cell to the next.
        let climb = |radians: f64, cell: f64| steps(radians * cell);
        // The rings' angle is the root of the distance squared over the length scale squared,
        // plus the clock: lengths are counted in length scales, then in coarse steps.
        let per_span = (RINGS * f64::from(p.twist.clamp(1, 10))).sqrt() * COARSE_PER_RADIAN;

        let last = colors.len() - 1;
        let mut palette = [colors.get(last as u64); MAX_PALETTE_COLORS];
        for (slot, k) in palette.iter_mut().zip(0..last as u64) {
            *slot = colors.get(k);
        }

        Self {
            columns,
            rows,
            turned: steps(clock),
            half_turned: steps(clock / 2.0),
            width_swell: climb(1.0, cell[0]),
            half_swell: [climb(0.5, cell[0]), climb(0.5, cell[1])],
            width_ripple: climb(RIPPLE, cell[0]),
            swinging_ripple: [climb(RIPPLE * tilt[0], cell[0]), climb(RIPPLE * tilt[1], cell[1])],
            corner_cell: cell.map(|cell| cell * COARSE_PER_RADIAN),
            rings_cell: cell.map(|cell| cell * per_span),
            rings_center: center.map(|at| at * per_span),
            rings_clock: clock * COARSE_PER_RADIAN * COARSE_PER_RADIAN,
            bands: f64::from(p.density.clamp(1, 10)) * SUM_PER_QUARTER_TURN,
            paint: match p.colors {
                PlasmaColors::Palette if last == 0 => Paint::Flat,
                PlasmaColors::Palette => Paint::Palette,
                PlasmaColors::RedGreen => Paint::RedGreen,
                PlasmaColors::BlueGreen => Paint::BlueGreen,
                PlasmaColors::Rainbow => Paint::Rainbow,
                PlasmaColors::White => Paint::White,
            },
            palette,
            half_palette: last as f64 / 2.0,
        }
    }

    /// Where the pixel's cell is on the color cycle: the seven waves there, added up, times the
    /// line density, a quarter turn for each unit.
    #[inline(always)]
    fn color_angle(&self, px: &Pixel) -> u64 {
        let (column, row) = cell_of(px, self.columns, self.rows);
        let (i, j) = (column as u64, row as u64);
        // The angle that climbs `by` steps a cell along one side.
        let along = |cells: u64, by: u64| cells.wrapping_mul(by);

        let width_swell = sine(along(i, self.width_swell).wrapping_add(self.turned));
        let up_half = along(j, self.half_swell[1]).wrapping_add(self.half_turned);
        let height_swell = sine(up_half);
        let diagonal_swell = sine(up_half.wrapping_add(along(i, self.half_swell[0])));
        let width_ripple = sine(along(i, self.width_ripple).wrapping_add(self.turned));
        let swinging_ripple = sine(
            along(i, self.swinging_ripple[0])
                .wrapping_add(along(j, self.swinging_ripple[1]))
                .wrapping_add(self.turned),
        );

        // The two that go by distance: from the bottom-left corner, and from the rings' center
        // (with the clock under the root, so the rings close in on their center as it runs).
        let (i, j) = (f64::from(column), f64::from(row));
        let (x, y) = (i * self.corner_cell[0], j * self.corner_cell[1]);
        let corner_swell = sine(coarse_steps((x * x + y * y).sqrt()).wrapping_add(self.turned));
        let (x, y) = (
            i * self.rings_cell[0] - self.rings_center[0],
            j * self.rings_cell[1] - self.rings_center[1],
        );
        let rings = sine(coarse_steps((x * x + (y * y + self.rings_clock)).sqrt()));

        // The two that take longest are added last, so the rest is not kept waiting for them.
        let sum = ((width_swell + height_swell) + (diagonal_swell + width_ripple) + swinging_ripple)
            + corner_swell
            + rings;
        ((sum * self.bands) as i64 as u64) << SUM_BITS
    }
}

/// The raised sine of an angle `ahead` steps on from `angle`: from 0 to 1, a half at no angle.
#[inline(always)]
fn raised(angle: u64, ahead: u64) -> f32 {
    unit((0.5 + 0.5 * sine(angle.wrapping_add(ahead))) as f32)
}

impl Shade for Plasma {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        Rgba::opaque(match self.paint {
            Paint::Flat => self.palette[0],
            Paint::Palette => {
                // Where along the palette, counting its colors from 0: the raised sine a third
                // of a turn ahead, times the steps from the first color to the last. Then the
                // color below that place, the next one, and how far from one to the other (at
                // the very top, the last color and none of what follows).
                let half = self.half_palette;
                let place = half + half * sine(self.color_angle(px).wrapping_add(THIRD_TURN));
                let below = (place as usize).min(MAX_PALETTE_COLORS - 2);
                let part = unit((place - below as f64) as f32);
                let (from, to) = (self.palette[below], self.palette[below + 1]);
                [
                    from[0] + (to[0] - from[0]) * part,
                    from[1] + (to[1] - from[1]) * part,
                    from[2] + (to[2] - from[2]) * part,
                ]
            }
            Paint::RedGreen => {
                let angle = self.color_angle(px);
                [raised(angle, 0), raised(angle, QUARTER_TURN), 0.0]
            }
            Paint::BlueGreen => {
                let angle = self.color_angle(px);
                [1.0 / 255.0, raised(angle, QUARTER_TURN), raised(angle, 0)]
            }
            Paint::Rainbow => {
                let angle = self.color_angle(px);
                [
                    raised(angle, 0),
                    raised(angle, THIRD_TURN),
                    raised(angle, 2 * THIRD_TURN),
                ]
            }
            Paint::White => [raised(self.color_angle(px), 0); 3],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Angles to try: the start, the middle and the end of every arc, a step either side of
    /// each, and a spread of others.
    fn angles() -> impl Iterator<Item = u64> {
        let arc = 1u64 << (64 - ARC_BITS);
        let at_arcs = (0..ARCS as u64).flat_map(move |k| {
            let (start, middle) = (k * arc, k * arc + arc / 2);
            [
                start.wrapping_sub(1),
                start,
                start + 1,
                middle - 1,
                middle,
                middle + 1,
            ]
        });
        let spread = (0..400_000u64).map(|k| k.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        at_arcs.chain(spread)
    }

    /// What the system's library makes of the angle (itself only good to the last few bits: the
    /// angle it is handed in radians is already rounded).
    fn library(angle: u64) -> (f64, f64) {
        (angle as f64 / TURN * TAU).sin_cos()
    }

    #[test]
    fn the_sine_is_within_a_hundred_millionth() {
        let mut worst = 0.0f64;
        for angle in angles() {
            worst = worst.max((sine(angle) - library(angle).0).abs());
        }
        assert!(worst < 1e-8, "the sine is off by {worst:e}");
        // And it is the parabola's own limit that is reached, not a slip in the arcs: what is
        // left of the series' next term once it is spread along an arc.
        let limit = (TAU / (2 * ARCS) as f64).powi(3) / 24.0;
        assert!(
            worst > 0.95 * limit,
            "the sine is off by only {worst:e}, not {limit:e}"
        );
    }

    /// The arcs are worked out by the compiler; the machine the effect runs on would have made
    /// the same of them, to the bit.
    #[test]
    fn the_arcs_are_the_same_worked_out_on_any_machine() {
        let here = std::hint::black_box(arcs as fn() -> [Parabola; ARCS])();
        for (k, (built, here)) in ARCS_OF_SINE.iter().zip(&here).enumerate() {
            assert_eq!(built.0.map(f64::to_bits), here.0.map(f64::to_bits), "arc {k}");
        }
    }

    #[test]
    fn the_sine_runs_on_from_each_arc_to_the_next() {
        let arc = 1u64 << (64 - ARC_BITS);
        for k in 0..ARCS as u64 {
            let (before, after) = (sine((k * arc).wrapping_sub(1)), sine(k * arc));
            assert!((before - after).abs() < 2e-8, "arc {k}: {before} then {after}");
        }
    }

    #[test]
    fn the_frames_sine_and_cosine_are_true_to_the_last_bits() {
        let mut worst = 0.0f64;
        for angle in angles() {
            let (sine, cosine) = library(angle);
            worst = worst.max((true_sine(angle) - sine).abs());
            worst = worst.max((true_cosine(angle) - cosine).abs());
        }
        assert!(worst < 3e-15, "off by {worst:e}");
    }

    #[test]
    fn the_true_sine_is_exactly_as_symmetric_as_the_sine() {
        assert_eq!(true_sine(0), 0.0);
        assert_eq!(true_sine(QUARTER_TURN), 1.0);
        assert_eq!(true_sine(HALF_TURN), 0.0);
        assert_eq!(true_sine(HALF_TURN + QUARTER_TURN), -1.0);
        assert_eq!(true_cosine(0), 1.0);
        assert_eq!(true_cosine(QUARTER_TURN), 0.0);
        for angle in angles() {
            let sine = true_sine(angle);
            assert_eq!(
                sine,
                true_sine(HALF_TURN.wrapping_sub(angle)),
                "{angle:#x} mirrored"
            );
            assert_eq!(
                sine,
                -true_sine(angle.wrapping_add(HALF_TURN)),
                "{angle:#x} half a turn on"
            );
            assert_eq!(sine, -true_sine(angle.wrapping_neg()), "{angle:#x} the other way");
        }
    }

    #[test]
    fn radians_become_steps_whichever_way_they_turn() {
        let quarter = TAU / 4.0;
        for (radians, turns) in [
            (0.0, 0.0),
            (quarter, 0.25),
            (-quarter, 0.75),
            (5.0 * quarter, 0.25),
            (-7.0 * quarter, 0.25),
        ] {
            let off = (steps(radians) as f64 / TURN - turns).abs();
            assert!(off < 1e-15, "{radians} radians: {off:e} of a turn off");
        }
        for radians in [0.1, 1.0, 3.0, 47.5, 1e6] {
            assert_eq!(
                steps(-radians),
                steps(radians).wrapping_neg(),
                "{radians} radians"
            );
        }
        // The square roots' angles agree with it, a coarse step at most below.
        for radians in [0.0, 0.3, 1.0, 1.5, 47.0, 700.0] {
            let fine = steps(radians) as f64 / TURN;
            let coarse = coarse_steps(radians * COARSE_PER_RADIAN) as f64 / TURN;
            let off = (fine - coarse + 0.5).rem_euclid(1.0) - 0.5;
            assert!(
                (-1e-13..1e-11).contains(&off),
                "{radians} radians: {off:e} of a turn off"
            );
        }
    }
}
