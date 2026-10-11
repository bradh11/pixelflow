//! The Pinwheel effect: arms of the palette colors turning around a center point.
//!
//! Every pixel is answered from its own place, with a fixed handful of sums and no memory of
//! its own, in four steps.
//!
//! 1. **Where it is.** The pixel's distance from the center in cells, and its direction round
//!    the center.
//! 2. **Which arm.** The direction is taken back by how far the wheel has turned and by how far
//!    the arms have curled at that distance (twist grows with distance), which leaves the
//!    pixel's place on a wheel standing still with straight arms. Counted in sectors (a turn
//!    divided by the number of arms), its whole part names the nearest arm and its fraction
//!    says how far across that sector the pixel sits. That costs the same for one arm or
//!    twenty, thin or wide, straight or wound right round.
//! 3. **How much of it is covered.** The pixel's distance in cells from the arm's edge says how
//!    much of the pixel's cell the arm covers: all of it well inside, none well outside, and
//!    in between across a soft band centered on the edge, so an edge glides over a pixel
//!    instead of popping, and an arm sheds exactly the light its shape says. The band is a
//!    cell wide at most and never more than a quarter of the arm's own width: faint light
//!    counts for as much as bright to a sparkle or to a layer that shows only where this one
//!    is dark, so a thin arm keeps a crisp edge. Where the next arm's edge crosses the same
//!    cell (arms that touch, or all of them meeting at the center) the two shares add up.
//! 4. **How bright.** Shading is a straight-line profile across the arm, read at the pixel's
//!    own place across it, and scales the coverage. A thin arm is at full strength whatever
//!    the shading.
//!
//! The two drawing styles differ in step 3 and in a few conventions (`Layout`).
//!
//! - **Smooth** arms are solid wedges (`Wedges`), measured square across the arm so that a
//!   twisted arm is judged by its true width. An arm is never thinner than one cell, so a
//!   thin arm stays an unbroken line however it is twisted.
//! - **Spokes** are rays (`Rays`): light falls only along the whole-degree directions out
//!   from the center, each a cell wide, and an arm lights the rays within its width. Close to
//!   the center the rays overlap and the arm is solid; far from it they stand apart with dark
//!   between, and stay where they are while the arm's light passes from one to the next. A
//!   pixel finds the one lit ray nearest it by rounding, however many rays the arm has.
//!
//! **Directions are whole numbers.** A full turn is 2^32 steps, so going round the circle wraps
//! by itself, turning and twisting are additions, and multiplying by the number of arms (or by
//! 360, for the rays) folds the circle onto one sector (or one degree) exactly. Nothing drifts
//! however long the effect runs, and the picture is exactly as symmetric as the pixels it is
//! asked for: a smooth wheel with straight arms is the exact reflection of the one turning the
//! other way, and a wheel whose arm count is a multiple of four repeats exactly every quarter
//! turn. The arctangent is a short polynomial, so the same pixel gets the same bits on every
//! machine.
//!
//! **Time is the clock's.** The wheel turns 20 degrees a second for each unit of speed, by the
//! milliseconds since the effect started, worked out once a frame in double precision. A frame
//! depends on nothing but the settings, the palette, the target's size, and that time.

use crate::color::{Colors, Rgba, unit};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use pf_sequence::{PinwheelParams, PinwheelShading, PinwheelStyle};
use std::f64::consts::TAU;

/// Steps of direction in a full turn.
const TURN: f64 = 4_294_967_296.0;
const HALF_TURN: u32 = 1 << 31;
const QUARTER_TURN: u32 = 1 << 30;
const EIGHTH_TURN: u32 = 1 << 29;
const STEPS_PER_RADIAN: f64 = TURN / TAU;
/// One step, as a fraction of a turn (and of a sector or a degree, once folded onto one).
const STEP: f32 = (1.0 / TURN) as f32;

/// The most arms a wheel has (the top of the "arms" setting's range).
const MAX_ARMS: u32 = 20;

/// Colors kept for the places round the wheel: one for each arm, the first again where the
/// count wraps, and one more either side for a pixel's neighboring arm. A power of two, so a
/// place can be masked into it.
const PLACES: usize = 32;

/// The widest an edge's soft band gets, in cells: the distance over which coverage goes from
/// none to all as the edge crosses a pixel. At one cell that is the share of the pixel's cell
/// the arm covers.
const SOFT: f32 = 1.0;

/// The most of an arm's own width its soft band may take. A quarter keeps the pixels an arm
/// lights at all within a quarter more than a hard edge would light, which matters wherever
/// faint light counts for as much as bright (a sparkle flashes at full strength on any lit
/// pixel).
const SOFT_SHARE: f32 = 0.25;

/// Half the width of the line a thin arm is kept at, and of a ray, in cells.
const HALF_LINE: f32 = 0.5;

/// How sharp the side of a ray is once the rays stand apart: one over the width of its soft
/// band in cells. Rays stay where they are, so there is no movement for a softer edge to
/// smooth, and a ray lights the pixels within half a cell of it and hardly any other.
const RAY_SHARP: f32 = 16.0;

/// How far from the center neighboring rays, a degree apart, are a cell apart: closer in they
/// overlap, further out they stand apart.
const RAYS_TOUCH: f32 = 57.295_78;
const RADIANS_PER_DEGREE: f32 = 1.0 / RAYS_TOUCH;

/// A ray exactly on an arm's counterclockwise edge is lit and one exactly on its clockwise edge
/// is not: the stretch of directions an arm lights is taken this many degrees counterclockwise
/// of where it is, far more than rounding can move it and far less than can be seen. Arms often
/// sit on whole degrees, and without this the rays on their edges would come and go from pixel
/// to pixel.
const RAY_TIE: f32 = 1.0 / 1024.0;

/// An arm shorter than this is treated as this long where its twist is spread along it, so a
/// vanishing arm doesn't wind without limit.
const SHORTEST: f64 = 1e-3;

/// What the two drawing styles do differently, besides how an arm is drawn.
struct Layout {
    /// Where the first arm points before anything turns it, in turns from pointing right.
    start: f64,
    /// Whether the start angle turns the wheel counterclockwise.
    start_angle_counterclockwise: bool,
    /// Whether the arms are numbered counterclockwise from the first.
    numbered_counterclockwise: bool,
    /// Whether a sweep is bright at its counterclockwise edge.
    bright_counterclockwise: bool,
    /// What an arm length of 100 reaches, in cells.
    reference: f64,
    /// The least share of its sector an arm fills.
    thinnest: f32,
}

impl Layout {
    fn of(p: &PinwheelParams, columns: f64, rows: f64) -> Self {
        match p.style {
            // The first arm points down, and the picture turning clockwise is the mirror image,
            // left to right, of the one turning counterclockwise. An arm length of 100 reaches
            // the corners.
            PinwheelStyle::Smooth => Self {
                start: 0.75,
                start_angle_counterclockwise: p.counterclockwise,
                numbered_counterclockwise: !p.counterclockwise,
                bright_counterclockwise: p.counterclockwise,
                reference: columns.hypot(rows) / 2.0,
                thinnest: 0.01,
            },
            // The first arm points right, and only the turning reverses. An arm length of 100
            // reaches the middle of the longer side.
            PinwheelStyle::Spokes => Self {
                start: 0.0,
                start_angle_counterclockwise: true,
                numbered_counterclockwise: true,
                bright_counterclockwise: true,
                reference: columns.max(rows) / 2.0,
                thinnest: 0.0,
            },
        }
    }
}

pub struct Pinwheel {
    /// The center, across and up the target (0–1).
    center: [f32; 2],
    /// What a full step across or up the target is in cells: its columns and rows, less one.
    cells: [f32; 2],
    /// Where the arms end, in cells from the center.
    length: f32,
    /// Beyond this distance (squared) nothing is lit. Zero when nothing is drawn at all.
    reach_squared: f32,
    arms: u64,
    /// Where the first arm's center line leaves the center at this moment, in steps.
    first: u32,
    /// How far the arms have curled round for each cell out from the center, in steps.
    curl: f32,
    /// One sector, in radians.
    sector: f32,
    /// Half an arm's width, as a share of its sector.
    half: f32,
    /// Shading across an arm: `shade[0] + shade[1] * t + shade[2] * |t|`, with `t` from -1 at
    /// the arm's clockwise edge to 1 at its counterclockwise edge.
    shade: [f32; 3],
    /// The color of the arm at each place round the wheel, counting counterclockwise from the
    /// first arm, starting one place before it.
    colors: [[f32; 3]; PLACES],
    drawn: Drawn,
}

/// How the arms are drawn.
enum Drawn {
    Wedges(Wedges),
    Rays(Rays),
}

impl Pinwheel {
    /// Everything that is the same for every pixel of one frame.
    pub fn new(p: &PinwheelParams, time: &EffectTime, colors: Colors, canvas: Canvas) -> Self {
        let (columns, rows) = (canvas.columns.max(1), canvas.rows.max(1));
        let layout = Layout::of(p, f64::from(columns), f64::from(rows));
        let arms = p.arms.clamp(1, MAX_ARMS);

        // The wheel turns 20 degrees a second for each unit of speed: a turn every 18 seconds.
        let turned = f64::from(p.speed) * time.elapsed_ms as f64 / 18_000.0;
        let first = steps(layout.start)
            .wrapping_add(toward(
                steps(f64::from(p.offset) / 360.0),
                layout.start_angle_counterclockwise,
            ))
            .wrapping_add(toward(steps(turned), p.counterclockwise));

        let length = (layout.reference * f64::from(p.arm_size) / 100.0).max(0.0);
        // A pixel within half a cell of the center is on the wheel however short its arms.
        let reach = if length > 0.0 { length.max(0.5) as f32 } else { 0.0 };
        // Turns of twist for each cell out from the center.
        let twist = f64::from(p.twist) / 360.0 / length.max(SHORTEST);

        let half = (p.thickness / 100.0).clamp(layout.thinnest, 1.0) / 2.0;
        let slope = if layout.bright_counterclockwise { 0.5 } else { -0.5 };
        let shade = match p.shading {
            PinwheelShading::Flat => [1.0, 0.0, 0.0],
            PinwheelShading::Raised => [1.0, 0.0, -1.0],
            PinwheelShading::Sunken => [0.0, 0.0, 1.0],
            PinwheelShading::Sweep => [0.5, slope, 0.0],
        };

        // Arm 1 takes the second palette color, arm 2 the third, and so on round the palette.
        let mut places = [[0.0; 3]; PLACES];
        for (slot, place) in places.iter_mut().zip(-1i64..) {
            let round = if layout.numbered_counterclockwise {
                place
            } else {
                -place
            };
            let arm = round.rem_euclid(i64::from(arms)) as u64;
            *slot = colors.get(arm + 1);
        }

        Self {
            center: [0.5 + p.center_x / 200.0, 0.5 + p.center_y / 200.0],
            cells: [(columns - 1) as f32, (rows - 1) as f32],
            length: reach,
            reach_squared: if reach > 0.0 {
                (reach + SOFT / 2.0).powi(2)
            } else {
                0.0
            },
            arms: u64::from(arms),
            first,
            curl: (twist * TURN) as f32,
            sector: (TAU / f64::from(arms)) as f32,
            half,
            shade,
            colors: places,
            drawn: match p.style {
                PinwheelStyle::Smooth => Drawn::Wedges(Wedges::new(half, (twist * TAU) as f32, shade)),
                PinwheelStyle::Spokes => Drawn::Rays(Rays::new(half, 360.0 / arms as f32)),
            },
        }
    }

    /// The shading `t` of the way across an arm, from -1 at its clockwise edge to 1 at its
    /// counterclockwise edge.
    #[inline]
    fn profile(&self, t: f32) -> f32 {
        unit(self.shade[0] + self.shade[1] * t + self.shade[2] * t.abs())
    }
}

/// How sharp the edge of an arm `width` cells wide is: one over the width of its soft band.
#[inline]
fn sharpness(width: f32) -> f32 {
    1.0 / (SOFT_SHARE * width).min(SOFT)
}

/// Arms drawn as solid wedges.
struct Wedges {
    /// How steeply an arm leans from the straight line out, for each cell from the center: the
    /// twist, in radians a cell.
    lean: f32,
    /// One over half an arm's width (as a share of its sector).
    per_half: f32,
    /// The gap between one arm and the next, as a share of a sector.
    gap: f32,
    /// The shading at the edge a neighboring arm turns toward a pixel: the arm clockwise of it
    /// (its counterclockwise edge), then the arm counterclockwise of it.
    facing: [f32; 2],
}

impl Wedges {
    fn new(half: f32, lean: f32, shade: [f32; 3]) -> Self {
        Self {
            lean,
            per_half: if half > 0.0 { 1.0 / half } else { 0.0 },
            gap: 1.0 - 2.0 * half,
            facing: [
                unit(shade[0] + shade[1] + shade[2]),
                unit(shade[0] - shade[1] + shade[2]),
            ],
        }
    }

    /// How much of the wheel shows at a pixel `r` cells from the center and `f` sectors
    /// counterclockwise of the nearest arm's center line, and whether more of that is the arm
    /// on the pixel's other side.
    #[inline]
    fn shown(&self, wheel: &Pinwheel, f: f32, r: f32) -> (f32, bool) {
        let (off, side) = if f < 0.0 { (-f, 0) } else { (f, 1) };
        // A sector's width here in cells, measured square across an arm (a twisted arm leans,
        // so it is narrower across than along the circle).
        let across = if self.lean == 0.0 {
            r * wheel.sector
        } else {
            let lean = r * self.lean;
            r * wheel.sector / (1.0 + lean * lean).sqrt()
        };
        // Most pixels are well clear of every arm: more than the widest soft band outside the
        // nearest arm and the line it is kept at, with the next arm further off still. (The
        // sums below would say so too; this saves them.)
        if (off - wheel.half) * across >= SOFT / 2.0
            && (off * across).min(r) >= HALF_LINE + SOFT / 2.0
            && self.gap * across >= SOFT
        {
            return (0.0, false);
        }
        let width = 2.0 * wheel.half * across;
        let sharp = sharpness(width.max(2.0 * HALF_LINE));

        // How much of the pixel's cell the nearest arm covers: the arm itself, by how far the
        // pixel is inside its edge, or the line it is kept at. The arm on the pixel's other side
        // takes what the nearest arm and the gap between the two leave of the cell.
        let wedge = unit(0.5 - (off - wheel.half) * across * sharp);
        let near = wedge.max(line(off, across, r, sharp));
        let far = unit((1.0 - wedge) - self.gap * across * sharp).max(line(1.0 - off, across, r, sharp));
        if near + far <= 0.0 {
            return (0.0, false);
        }

        // Shading takes hold as the arm widens from one cell to two.
        let shaded = unit(width - 1.0);
        let own = near * (1.0 - shaded * (1.0 - wheel.profile(f * self.per_half)));
        let other = far * (1.0 - shaded * (1.0 - self.facing[side]));
        let end = unit((wheel.length - r) * sharp + 0.5);
        ((own + other) * end, other > own)
    }
}

/// How much of a pixel's cell (0–1) is covered by the line a thin arm is kept at, for a pixel
/// `off` sectors from the arm's center line and `r` cells from the center, where a sector is
/// `across` cells wide. The line ends at the center, like the arm.
#[inline]
fn line(off: f32, across: f32, r: f32, sharp: f32) -> f32 {
    unit(0.5 - ((off * across).min(r) - HALF_LINE) * sharp)
}

/// Arms drawn as rays: light only along the whole-degree directions out from the center, each a
/// cell wide. Out to `RAYS_TOUCH` cells neighboring rays overlap and an arm is a solid wedge,
/// half a cell wider each side than its angle; beyond, they stand apart.
///
/// The rays stay where they are while the wheel turns, so far out an arm's edge can only step
/// from one to the next. Close to the center, where a degree is a small part of a cell, the
/// edge glides instead: the nearest lit direction is anywhere within the arm there, and is
/// held to whole degrees more and more firmly out to where the rays part.
struct Rays {
    /// One sector, in degrees.
    sector: f32,
    /// Half an arm's width in degrees, and one over it.
    half: f32,
    per_half: f32,
    /// Half the stretch of directions within which an arm lights rays, in degrees. The stretch
    /// is the arm's width in whole degrees and one more, so an arm lights that many rays, and
    /// the thinnest arm lights the one ray nearest its center line.
    lit_half: f32,
    /// How far shading has taken hold: an arm `w` degrees wide is about `w + 1` rays, and its
    /// shading runs over them from one part in `w + 1` up to full, so a single ray is at full
    /// strength and no ray of a wide arm is quite dark.
    shaded: f32,
}

impl Rays {
    fn new(half: f32, sector: f32) -> Self {
        let width = 2.0 * half * sector;
        Self {
            sector,
            half: width / 2.0,
            per_half: if width > 0.0 { 2.0 / width } else { 0.0 },
            lit_half: (width.floor() + 1.0) / 2.0,
            shaded: width / (width + 1.0),
        }
    }

    /// How much of the wheel shows at a pixel `r` cells from the center, in `direction`, and `f`
    /// sectors counterclockwise of the nearest arm's center line, and whether more of that is
    /// the arm on the pixel's other side.
    #[inline]
    fn shown(&self, wheel: &Pinwheel, direction: u32, f: f32, r: f32) -> (f32, bool) {
        // The nearest arm's center line, in degrees counterclockwise of the pixel. Most pixels
        // are well clear of every direction it lights (and of the center, where every arm
        // starts): more than a ray's reach and the widest soft band away. (The sums below would
        // say so too; this saves them.)
        let center = -f * self.sector;
        let clear = (center.abs() - self.lit_half - RAY_TIE) * RADIANS_PER_DEGREE * r;
        if clear.min(r) >= HALF_LINE + SOFT / 2.0 {
            return (0.0, false);
        }
        // How far counterclockwise of its nearest ray the pixel is (-0.5 to 0.5 degrees): the
        // fraction of its direction counted in degrees.
        let beside = ((u64::from(direction) * 360) as u32 as i32) as f32 * STEP;
        // How firmly lit directions are held to the rays here: not at all out to half way to
        // where the rays part, fully from there on.
        let held = unit(r * (2.0 / RAYS_TOUCH) - 1.0);
        // The arm's width here in cells, and how sharp its edge is: as a solid wedge (a ray's
        // reach wider each side), or as rays.
        let width = 2.0 * wheel.half * wheel.sector * r;
        let solid = sharpness(width + 2.0 * HALF_LINE);
        let sharp = solid + held * (RAY_SHARP - solid);

        // The neighboring arm's center line, likewise.
        let next = if f < 0.0 {
            center - self.sector
        } else {
            center + self.sector
        };
        let (near, near_at) = self.arm(center, beside, held, sharp, r);
        let (far, far_at) = self.arm(next, beside, held, sharp, r);
        if near + far <= 0.0 {
            return (0.0, false);
        }

        // An arm no wider than a cell is at full strength, as a single ray is.
        let shaded = self.shaded.min(unit(width - 1.0));
        let own = near * (1.0 - shaded * (1.0 - wheel.profile(near_at)));
        let other = far * (1.0 - shaded * (1.0 - wheel.profile(far_at)));
        // Where the wheel is solid the two arms are two shapes, and what each covers of the
        // cell adds up; where the rays stand apart, two arms that both reach the pixel do it
        // with the same ray.
        let both = own.max(other) + (1.0 - held) * own.min(other);
        let end = unit((wheel.length - r) * sharp + 0.5);
        (both * end, other > own)
    }

    /// One arm: how much of the pixel's cell its nearest lit direction covers, and where across
    /// the arm that direction is (-1 to 1). `center` is the arm's center line in degrees
    /// counterclockwise of the pixel, and the pixel is `beside` degrees counterclockwise of its
    /// own nearest ray.
    #[inline]
    fn arm(&self, center: f32, beside: f32, held: f32, sharp: f32, r: f32) -> (f32, f32) {
        // Degrees round from the pixel to the nearest direction the arm lights: any direction
        // within the arm...
        let within = (center - self.half).max(0.0) + (center + self.half).min(0.0);
        // ...or the nearest of the rays it lights, counted in whole degrees from the pixel's
        // own nearest ray.
        let middle = beside + center + RAY_TIE;
        let ray = whole_from(middle - self.lit_half) - whole_from(-(middle + self.lit_half)) - beside;
        let turn = within + held * (ray - within);
        let away = (turn.abs() * RADIANS_PER_DEGREE * r).min(r);
        (
            unit(0.5 - (away - HALF_LINE) * sharp),
            (turn - center) * self.per_half,
        )
    }
}

/// The least whole number that is at least `x` and at least 0.
#[inline]
fn whole_from(x: f32) -> f32 {
    let x = x.max(0.0);
    let whole = x as i32 as f32;
    if whole < x { whole + 1.0 } else { whole }
}

/// An angle in turns as steps round the circle.
fn steps(turns: f64) -> u32 {
    (turns.rem_euclid(1.0) * TURN) as u64 as u32
}

/// `steps` round the circle one way or the other. Going the other way is the same number of
/// steps exactly, so a wheel turning clockwise is the exact reflection of one turning
/// counterclockwise.
fn toward(steps: u32, counterclockwise: bool) -> u32 {
    if counterclockwise {
        steps
    } else {
        steps.wrapping_neg()
    }
}

/// The angle whose tangent is `t` (0 to 1), in steps: none to an eighth of a turn. A polynomial
/// in place of the library's arctangent (Abramowitz and Stegun, 4.4.49: within 2e-8 radians),
/// so it costs a few multiplications and gives the same bits everywhere.
#[inline]
fn arctangent(t: f32) -> u32 {
    const fn scaled(c: f64) -> f32 {
        (c * STEPS_PER_RADIAN) as f32
    }
    const C: [f32; 9] = [
        scaled(1.0),
        scaled(-0.333_331_452_8),
        scaled(0.199_935_508_5),
        scaled(-0.142_088_994_4),
        scaled(0.106_562_639_3),
        scaled(-0.075_289_640_0),
        scaled(0.042_909_613_8),
        scaled(-0.016_165_736_7),
        scaled(0.002_866_225_7),
    ];
    let z = t * t;
    let z2 = z * z;
    let z4 = z2 * z2;
    let low = (C[0] + C[1] * z) + (C[2] + C[3] * z) * z2;
    let high = (C[4] + C[5] * z) + (C[6] + C[7] * z) * z2;
    (t * (low + (high + C[8] * z4) * z4)) as u32
}

/// A pixel's place on one axis, with anything that isn't a number counted as 0.
#[inline]
fn place(at: f32) -> f32 {
    if at.is_finite() { at } else { 0.0 }
}

impl Shade for Pinwheel {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        // Where the pixel is from the center, in cells.
        let x = (place(px.u) - self.center[0]) * self.cells[0];
        let y = (place(px.v) - self.center[1]) * self.cells[1];
        let squared = x * x + y * y;
        if squared >= self.reach_squared {
            return Rgba::CLEAR;
        }
        let r = squared.sqrt();

        // Its direction, counterclockwise from pointing right: the angle within an eighth of
        // a turn, unfolded by which side of the diagonal and of each axis the pixel is on.
        let (wide, tall) = (x.abs(), y.abs());
        let steep = tall > wide;
        let (short, long) = if steep { (wide, tall) } else { (tall, wide) };
        let mut direction = if short < long {
            arctangent(short / long)
        } else if long > 0.0 {
            EIGHTH_TURN
        } else {
            // The center itself has no direction: straight up serves, which a wheel and its
            // reflection agree on.
            QUARTER_TURN
        };
        if steep {
            direction = QUARTER_TURN - direction;
        }
        if x < 0.0 {
            direction = HALF_TURN - direction;
        }
        if y < 0.0 {
            direction = direction.wrapping_neg();
        }

        // Its place on the wheel standing still with straight arms, in sectors (32 bits of
        // whole sectors above 32 bits of fraction): the nearest arm, and how far
        // counterclockwise of that arm's center line it sits (-0.5 to 0.5 of a sector).
        let curled = (r * self.curl) as i64 as u32;
        let on_wheel = direction.wrapping_sub(self.first).wrapping_sub(curled);
        let sectors = u64::from(on_wheel) * self.arms;
        let nearest = ((sectors + u64::from(HALF_TURN)) >> 32) as usize;
        let f = (sectors as u32 as i32) as f32 * STEP;

        // How much of the wheel shows at the pixel, and whether more of that is the arm on the
        // pixel's other side than the nearest one: the arm showing more gives the color.
        let (shown, neighbor) = match &self.drawn {
            Drawn::Wedges(wedges) => wedges.shown(self, f, r),
            Drawn::Rays(rays) => rays.shown(self, direction, f, r),
        };
        let alpha = unit(shown);
        if alpha <= 0.0 {
            return Rgba::CLEAR;
        }
        let place = match (neighbor, f < 0.0) {
            (false, _) => nearest + 1,
            (true, true) => nearest,
            (true, false) => nearest + 2,
        };
        Rgba::with_alpha(self.colors[place % PLACES], alpha)
    }
}
