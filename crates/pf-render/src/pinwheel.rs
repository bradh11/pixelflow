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
//!   from the center, and an arm lights the rays within its width. Each ray is a string of
//!   marks half a cell apart, and a mark lights the cell it falls in. Close to the center the
//!   rays share cells and the arm is solid; far from it they stand apart with dark between,
//!   and stay where they are while the arm's light passes from one to the next. A twisted arm
//!   crosses from ray to ray as it goes out, and every half cell of it lights a cell, so it is
//!   an unbroken run of them. A pixel looks only at the marks that could fall in its own cell
//!   (three steps of distance on the two rays either side of it), however many rays the arm
//!   has.
//!
//! **Directions are whole numbers.** A full turn is 2^32 steps, so going round the circle wraps
//! by itself, turning and twisting are additions, and multiplying by the number of arms (or by
//! 360, for the rays) folds the circle onto one sector (or one degree) exactly. Nothing drifts
//! however long the effect runs, and a solid wheel is exactly as symmetric as the pixels it is
//! asked for: a smooth wheel with straight arms is the exact reflection of the one turning the
//! other way, and one whose arm count is a multiple of four repeats exactly every quarter turn.
//! The arctangent is a short polynomial, so the same pixel gets the same bits on every machine.
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

/// Half the width of the line a thin arm is kept at, in cells.
const HALF_LINE: f32 = 0.5;

/// How far apart, in cells, the marks along a ray are: each lights the cell it falls in. Half a
/// cell is how closely xLights' picture marks its rays: a twisted arm, which crosses many rays in
/// a short distance, then lights as many cells as it does there.
const MARK: f32 = 0.5;

/// The furthest, in cells, a mark can be from the center of a cell it falls in is half the
/// cell's diagonal, 0.71; marks no nearer than this are certainly outside it.
const MARK_REACH: f32 = 0.75;

/// A mark is in a cell when it is between these of the cell's center, across and up: half a
/// cell either way, moved down a hair so that a mark on the line between two cells (a ray along
/// a row of an even-sized buffer puts all its marks there) lights the upper one and not both,
/// and stretched a smaller hair so that no mark falls between two cells however the sums round.
const CELL_FROM: f32 = -0.5 - 1.0 / 1024.0;
const CELL_TO: f32 = 0.5 - 1.0 / 1024.0 + 1.0 / 4096.0;

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
    /// The pixel position the center settings name, across and up the target (0–1).
    center: [f32; 2],
    /// How many cells beyond that position the center is. The settings place the center by the
    /// target's whole extent, from its left (bottom) edge at -100 to its right (top) edge at
    /// 100, and a target of `n` cells is `n` cells from edge to edge; pixel positions run from
    /// the middle of its first cell to the middle of its last, `n - 1` cells. The two agree in
    /// the middle of the target and part evenly either way, to half a cell at its edges.
    beyond: [f32; 2],
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

        // How far the center is from the middle, as a share of the target's extent.
        let moved = [p.center_x / 200.0, p.center_y / 200.0];

        Self {
            center: [0.5 + moved[0], 0.5 + moved[1]],
            beyond: moved,
            cells: [(columns - 1) as f32, (rows - 1) as f32],
            length: reach,
            reach_squared: if reach > 0.0 {
                (reach + MARK_REACH.max(SOFT / 2.0)).powi(2)
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
                PinwheelStyle::Spokes => Drawn::Rays(Rays::new(
                    half,
                    360.0 / arms as f32,
                    (twist * 360.0) as f32,
                    length as f32,
                )),
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
        if self.clear(wheel, f, r) {
            return (0.0, false);
        }
        self.covered(wheel, f, r)
    }

    /// A sector's width in cells, `r` cells from the center, measured square across an arm (a
    /// twisted arm leans, so it is narrower across than along the circle).
    #[inline]
    fn across(&self, wheel: &Pinwheel, r: f32) -> f32 {
        if self.lean == 0.0 {
            r * wheel.sector
        } else {
            let lean = r * self.lean;
            r * wheel.sector / (1.0 + lean * lean).sqrt()
        }
    }

    /// Whether the pixel is well clear of every arm, as most are: more than the widest soft
    /// band outside the nearest arm and the line it is kept at, with the next arm further off
    /// still. [`Self::covered`] would find nothing there too; this saves its sums.
    #[inline]
    fn clear(&self, wheel: &Pinwheel, f: f32, r: f32) -> bool {
        let (off, across) = (f.abs(), self.across(wheel, r));
        (off - wheel.half) * across >= SOFT / 2.0
            && (off * across).min(r) >= HALF_LINE + SOFT / 2.0
            && self.gap * across >= SOFT
    }

    /// [`Self::shown`], worked out in full.
    #[inline]
    fn covered(&self, wheel: &Pinwheel, f: f32, r: f32) -> (f32, bool) {
        let (off, side) = if f < 0.0 { (-f, 0) } else { (f, 1) };
        let across = self.across(wheel, r);
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

/// Arms drawn as rays: light only along the whole-degree directions out from the center. An arm
/// lights the rays within its width, as many as it is whole degrees wide and one more, and each
/// ray is a string of marks half a cell apart, each lighting the cell it falls in. Out to
/// `RAYS_TOUCH` cells neighboring rays share cells and an arm is solid; beyond, they stand apart
/// with dark between, and stay where they are while the wheel turns.
///
/// With twist an arm crosses from ray to ray as it goes out, often in less than a cell, and a
/// mark belongs to the ray the arm is on at the mark's own distance: so every half cell of the
/// arm's length lights a cell, and a twisted arm is an unbroken run of them however fast it
/// crosses the rays.
///
/// Close to the center, where a degree is a small part of a cell, an arm is drawn as the solid
/// wedge its rays make there, half a cell wider all round than its angle, with a soft edge that
/// glides as the wheel turns instead of stepping from ray to ray. From half way out to where
/// the rays part the wedge fades and the marks take over; the brighter of the two shows.
struct Rays {
    /// One sector, in degrees.
    sector: f32,
    /// Half an arm's width, in degrees.
    half: f32,
    /// How many rays an arm lights: its width in whole degrees, and one more.
    count: f32,
    /// How far the arms curl round for each cell out from the center, in degrees.
    twist: f32,
    /// The same in radians: how steeply an arm leans from the straight line out, for each cell
    /// from the center.
    lean: f32,
    /// Where the arms end, in cells from the center: no ray is marked beyond it.
    length: f32,
}

impl Rays {
    fn new(half: f32, sector: f32, twist: f32, length: f32) -> Self {
        let width = 2.0 * half * sector;
        Self {
            sector,
            half: width / 2.0,
            count: width.floor() + 1.0,
            twist,
            lean: twist * RADIANS_PER_DEGREE,
            length,
        }
    }

    /// How much of the wheel shows at a pixel, and whether more of that is the arm on the
    /// pixel's other side.
    #[inline]
    fn shown(&self, wheel: &Pinwheel, spot: &Spot) -> (f32, bool) {
        if self.clear(spot.f, spot.r) {
            return (0.0, false);
        }
        self.covered(wheel, spot)
    }

    /// Whether the pixel is well clear of everything the nearest arm lights (and of the center,
    /// where every arm starts), as most are. [`Self::covered`] would find nothing there too;
    /// this saves its sums.
    #[inline]
    fn clear(&self, f: f32, r: f32) -> bool {
        self.out_of_reach(-f * self.sector, r)
    }

    /// Whether the arm whose center line is `center` degrees counterclockwise of a pixel `r`
    /// cells out can light nothing there: neither a mark in the pixel's cell, at any distance a
    /// mark in it could be at, nor the solid wedge with its soft edge.
    #[inline]
    fn out_of_reach(&self, center: f32, r: f32) -> bool {
        // Degrees from the pixel to the nearest ray the arm lights at any of those distances.
        let outside = center.abs() - self.count / 2.0 - RAY_TIE - self.twist.abs() * MARK_REACH;
        let cells = outside * RADIANS_PER_DEGREE * (r - MARK_REACH).max(0.0);
        if r >= RAYS_TOUCH {
            cells >= MARK_REACH
        } else {
            (cells * self.square(r)).min(r) >= HALF_LINE + SOFT / 2.0
        }
    }

    /// How much shorter a step round the circle is when measured square across an arm, `r`
    /// cells out: a twisted arm leans, so it is narrower across than along the circle.
    #[inline]
    fn square(&self, r: f32) -> f32 {
        if self.lean == 0.0 {
            1.0
        } else {
            let lean = r * self.lean;
            1.0 / (1.0 + lean * lean).sqrt()
        }
    }

    /// [`Self::shown`], worked out in full.
    #[inline]
    fn covered(&self, wheel: &Pinwheel, spot: &Spot) -> (f32, bool) {
        let r = spot.r;
        // The nearest arm's center line and its neighbor's, in degrees counterclockwise of the
        // pixel.
        let center = -spot.f * self.sector;
        let next = if spot.f < 0.0 {
            center - self.sector
        } else {
            center + self.sector
        };
        // The arm's width here in cells along the circle: shading takes hold as it widens from
        // one cell to two, and an arm of a single ray is at full strength anyway.
        let width = 2.0 * self.half * RADIANS_PER_DEGREE * r;
        let shaded = if self.count > 1.0 { unit(width - 1.0) } else { 0.0 };
        // How far the marks have taken over from the solid wedge here: not at all out to half
        // way to where the rays part, fully from there on.
        let marked = unit(r * (2.0 / RAYS_TOUCH) - 1.0);

        let mut shown = (0.0, false);
        if marked < 0.6 {
            let per_degree = RADIANS_PER_DEGREE * r * self.square(r);
            let sharp = sharpness(2.0 * self.half * per_degree + 2.0 * HALF_LINE);
            let own = self.wedge(wheel, center, r, per_degree, sharp, shaded);
            let other = self.wedge(wheel, next, r, per_degree, sharp, shaded);
            let end = unit((wheel.length - r) * sharp + 0.5);
            // The two arms are two shapes, and what each covers of the cell adds up.
            shown = (
                (own + other).min(1.0) * end * unit(6.0 - 10.0 * marked),
                other > own,
            );
        }
        if marked > 0.4 {
            // How far counterclockwise of its nearest ray the pixel is (-0.5 to 0.5 degrees):
            // the fraction of its direction counted in degrees.
            let beside = ((u64::from(spot.direction) * 360) as u32 as i32) as f32 * STEP;
            // Far out, most pixels are further from their nearest ray than a mark can be from
            // the center of the cell it lights: no arm marks those.
            if r * beside.abs() * RADIANS_PER_DEGREE < MARK_REACH {
                let own = self.marks(wheel, center, beside, spot, shaded);
                let other = if self.out_of_reach(next, r) {
                    0.0
                } else {
                    self.marks(wheel, next, beside, spot, shaded)
                };
                // Two arms that both mark the cell do it with the same ray, or near enough.
                let marks = own.max(other) * unit(10.0 * marked - 4.0);
                if marks > shown.0 {
                    shown = (marks, other > own);
                }
            }
        }
        shown
    }

    /// One arm as a solid wedge: how much it shows at a pixel `r` cells out, for an arm whose
    /// center line is `center` degrees counterclockwise of the pixel. A degree round the circle
    /// is `per_degree` cells measured square across the arm.
    #[inline]
    fn wedge(&self, wheel: &Pinwheel, center: f32, r: f32, per_degree: f32, sharp: f32, shaded: f32) -> f32 {
        // Degrees round from the pixel to the nearest direction within the arm, and how far
        // that is in cells: within half a cell the pixel is covered, as by a ray there.
        let within = (center - self.half).max(0.0) + (center + self.half).min(0.0);
        let away = (within.abs() * per_degree).min(r);
        let cover = unit(0.5 - (away - HALF_LINE) * sharp);
        // Where across the arm that direction is, counted as its rays are: the first ray, on
        // the clockwise edge, is at 0, and the last falls one ray short of 1.
        let across = ((within - center) * 2.0 - 1.0) / self.count;
        cover * (1.0 - shaded * (1.0 - wheel.profile(across)))
    }

    /// One arm as rays: how much it shows at a pixel, for an arm whose center line is `center`
    /// degrees counterclockwise of the pixel at the pixel's own distance. The pixel is `beside`
    /// degrees counterclockwise of its own nearest ray.
    #[inline]
    fn marks(&self, wheel: &Pinwheel, center: f32, beside: f32, spot: &Spot, shaded: f32) -> f32 {
        let r = spot.r;
        // The pixel's direction as a step of one cell: out from the center, and (turned a
        // quarter turn) round it.
        let (out_x, out_y) = (spot.x / r, spot.y / r);
        // A mark that falls in the pixel's cell is at one of the three steps of distance
        // nearest the pixel's own, on one of the two rays either side of the pixel.
        let nearest = (r * (1.0 / MARK) + 0.5) as i32;
        let side = if beside < 0.0 { -1 } else { 1 };
        let mut best = 0.0f32;
        for step in nearest - 1..=nearest + 1 {
            let out = step as f32 * MARK;
            if out > self.length {
                break;
            }
            let along = out - r;
            // The rays the arm lights at that distance, counted in whole degrees from the
            // pixel's own nearest ray: the arm has curled on from where it is at the pixel's.
            let middle = beside + center + self.twist * along + RAY_TIE;
            let first = -floor(self.count / 2.0 - middle);
            let last = floor(middle + self.count / 2.0);
            for ray in [0, side] {
                let ray = ray.clamp(first, last.max(first));
                // The mark's place from the pixel's center: round the circle and out (the
                // sine and cosine of the small angle between them, to their third and second
                // powers), then across and up. A mark too far round is outside the cell
                // wherever it is along its ray.
                let turned = (ray as f32 - beside) * RADIANS_PER_DEGREE;
                if (out * turned).abs() >= MARK_REACH {
                    continue;
                }
                let squared = turned * turned;
                let round = out * turned * (1.0 - squared * (1.0 / 6.0));
                let further = along - out * squared * 0.5;
                let (x, y) = (further * out_x - round * out_y, further * out_y + round * out_x);
                if (CELL_FROM..=CELL_TO).contains(&x) && (CELL_FROM..=CELL_TO).contains(&y) {
                    // The ray's place across the arm: the first is at 0, and the last falls
                    // one ray short of 1.
                    let across = (2 * (ray - first)) as f32 / self.count - 1.0;
                    best = best.max(1.0 - shaded * (1.0 - wheel.profile(across)));
                }
            }
        }
        best
    }
}

/// The greatest whole number that is at most `x`.
#[inline]
fn floor(x: f32) -> i32 {
    let whole = x as i32;
    if whole as f32 > x { whole - 1 } else { whole }
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

/// Where a pixel is on the wheel.
struct Spot {
    /// Cells across and up from the center, and its distance from it.
    x: f32,
    y: f32,
    r: f32,
    /// Its direction from the center, in steps counterclockwise from pointing right.
    direction: u32,
    /// The nearest arm's place round the wheel, counting counterclockwise from the first arm.
    nearest: usize,
    /// How far counterclockwise of that arm's center line it sits (-0.5 to 0.5 of a sector).
    f: f32,
}

impl Pinwheel {
    /// Where a pixel is on the wheel; `None` beyond where anything is lit.
    #[inline]
    fn spot(&self, px: &Pixel) -> Option<Spot> {
        // Where the pixel is from the center, in cells.
        let x = (place(px.u) - self.center[0]) * self.cells[0] - self.beyond[0];
        let y = (place(px.v) - self.center[1]) * self.cells[1] - self.beyond[1];
        let squared = x * x + y * y;
        if squared >= self.reach_squared {
            return None;
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
        // counterclockwise of that arm's center line it sits.
        let curled = (r * self.curl) as i64 as u32;
        let on_wheel = direction.wrapping_sub(self.first).wrapping_sub(curled);
        let sectors = u64::from(on_wheel) * self.arms;
        Some(Spot {
            x,
            y,
            r,
            direction,
            nearest: ((sectors + u64::from(HALF_TURN)) >> 32) as usize,
            f: (sectors as u32 as i32) as f32 * STEP,
        })
    }
}

impl Shade for Pinwheel {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let Some(spot) = self.spot(px) else {
            return Rgba::CLEAR;
        };
        // How much of the wheel shows at the pixel, and whether more of that is the arm on the
        // pixel's other side than the nearest one: the arm showing more gives the color.
        let (shown, neighbor) = match &self.drawn {
            Drawn::Wedges(wedges) => wedges.shown(self, spot.f, spot.r),
            Drawn::Rays(rays) => rays.shown(self, &spot),
        };
        let alpha = unit(shown);
        if alpha <= 0.0 {
            return Rgba::CLEAR;
        }
        let place = match (neighbor, spot.f < 0.0) {
            (false, _) => spot.nearest + 1,
            (true, true) => spot.nearest,
            (true, false) => spot.nearest + 2,
        };
        Rgba::with_alpha(self.colors[place % PLACES], alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_sequence::Rgb;

    /// Whether the shortcut answers for the pixel at this spot, and what the sums give there.
    fn both_ways(wheel: &Pinwheel, spot: &Spot) -> (bool, (f32, bool)) {
        match &wheel.drawn {
            Drawn::Wedges(w) => (w.clear(wheel, spot.f, spot.r), w.covered(wheel, spot.f, spot.r)),
            Drawn::Rays(rays) => (rays.clear(spot.f, spot.r), rays.covered(wheel, spot)),
        }
    }

    fn pixel(u: f64, v: f64) -> Pixel {
        Pixel {
            u: u as f32,
            v: v as f32,
            index: 0,
            count: 1,
        }
    }

    /// What was tried, to show the sweep reached every case.
    #[derive(Default)]
    struct Tried {
        /// Pixels the shortcut answered for, and pixels the sums found nothing at without it.
        skipped: usize,
        unlit_in_full: usize,
        lit: usize,
        /// Places found where the shortcut starts or stops answering, each tried on both sides.
        cut_offs: usize,
    }

    /// One pixel: wherever the shortcut answers, the sums must find nothing, exactly; and what
    /// the wheel shows must be what the sums give.
    fn try_pixel(wheel: &Pinwheel, px: &Pixel, tried: &mut Tried, what: &dyn Fn() -> String) -> Option<bool> {
        let spot = wheel.spot(px)?;
        let (clear, full) = both_ways(wheel, &spot);
        if clear {
            assert_eq!(
                (full.0.to_bits(), full.1),
                (0.0f32.to_bits(), false),
                "{}: the shortcut answers at ({}, {}) where the sums give {full:?}",
                what(),
                px.u,
                px.v
            );
            tried.skipped += 1;
        } else if full.0 <= 0.0 {
            tried.unlit_in_full += 1;
        } else {
            tried.lit += 1;
        }
        let shown = match &wheel.drawn {
            Drawn::Wedges(w) => w.shown(wheel, spot.f, spot.r),
            Drawn::Rays(rays) => rays.shown(wheel, &spot),
        };
        assert_eq!(
            (shown.0.to_bits(), shown.1),
            (full.0.to_bits(), full.1),
            "{}",
            what()
        );
        Some(clear)
    }

    /// Pixels along a path (`at` gives the place for 0 to 1 along it): a spread of them, and
    /// wherever the shortcut starts or stops answering between two of those, the two nearest
    /// places either side of that cut-off that can be told apart, and a few a little further off.
    fn try_path(
        wheel: &Pinwheel,
        at: &dyn Fn(f64) -> (f64, f64),
        tried: &mut Tried,
        what: &dyn Fn() -> String,
    ) {
        const STEPS: usize = 96;
        let try_at = |t: f64, tried: &mut Tried| {
            let (u, v) = at(t);
            try_pixel(wheel, &pixel(u, v), tried, what)
        };
        let mut last: Option<(f64, bool)> = None;
        for step in 0..=STEPS {
            let t = step as f64 / STEPS as f64;
            let Some(clear) = try_at(t, tried) else {
                last = None;
                continue;
            };
            if let Some((before, was)) = last
                && was != clear
            {
                // Close in on the cut-off between the two.
                let (mut a, mut b) = (before, t);
                for _ in 0..60 {
                    let middle = (a + b) / 2.0;
                    match try_at(middle, tried) {
                        Some(here) if here == was => a = middle,
                        _ => b = middle,
                    }
                }
                tried.cut_offs += 1;
                for nudge in [0.0, 1e-9, 1e-7, 1e-5, 1e-3] {
                    try_at(a - nudge, tried);
                    try_at(b + nudge, tried);
                }
            }
            last = Some((t, clear));
        }
    }

    /// The shortcut for pixels clear of every arm changes nothing: over wheels of every kind, on
    /// buffers from one pixel to hundreds of cells, at pixels on the grid, between its cells, and
    /// right at the shortcut's cut-off on both sides, the wheel shows bit for bit what the sums
    /// alone give.
    #[test]
    fn the_shortcut_for_pixels_clear_of_every_arm_changes_nothing() {
        let buffers = [(1, 1), (3, 3), (41, 1), (1, 41), (9, 5), (101, 101), (400, 178)];
        let placed = [
            (100.0, 0.0, 0.0, 0.0, 0u64),
            (37.0, 25.0, -60.0, 77.7, 1350),
            (400.0, -100.0, 100.0, 359.5, 86_400_000),
        ];
        let mut tried = [Tried::default(), Tried::default()];
        for (which, style) in [PinwheelStyle::Smooth, PinwheelStyle::Spokes]
            .into_iter()
            .enumerate()
        {
            for arms in [1, 2, 3, 4, 7, 12, 20] {
                for thickness in [0.0, 0.5, 1.0, 5.0, 25.0, 50.0, 90.0, 99.9, 100.0] {
                    for twist in [0.0, 30.0, -360.0, 360.0] {
                        for shading in [PinwheelShading::Flat, PinwheelShading::Sweep] {
                            for (columns, rows) in buffers {
                                for (arm_size, center_x, center_y, offset, elapsed_ms) in placed {
                                    let p = PinwheelParams {
                                        arms,
                                        arm_size,
                                        twist,
                                        thickness,
                                        speed: 7.0,
                                        counterclockwise: arms % 2 == 0,
                                        shading,
                                        offset,
                                        center_x,
                                        center_y,
                                        style,
                                    };
                                    let what = || format!("{p:?} on {columns} by {rows} at {elapsed_ms} ms");
                                    let time = EffectTime::within(0, 90_000_000, elapsed_ms);
                                    let canvas = Canvas { columns, rows };
                                    let wheel = Pinwheel::new(&p, &time, Colors::new(&[Rgb::WHITE]), canvas);
                                    let tried = &mut tried[which];
                                    // Pixels on the grid (every one, or a spread of a large one).
                                    let stride = (columns * rows / 300).max(1) as usize;
                                    for k in (0..columns * rows).step_by(stride) {
                                        let (x, y) = (k % columns, k / columns);
                                        let along = |i: u32, n: u32| {
                                            if n > 1 {
                                                f64::from(i) / f64::from(n - 1)
                                            } else {
                                                0.5
                                            }
                                        };
                                        try_pixel(
                                            &wheel,
                                            &pixel(along(x, columns), along(y, rows)),
                                            tried,
                                            &what,
                                        );
                                    }
                                    // Straight lines over the buffer and a little past its edges,
                                    // at places that fall between cells.
                                    let (cu, cv) = (f64::from(wheel.center[0]), f64::from(wheel.center[1]));
                                    for k in 0..4 {
                                        let lean = f64::from(k) * 0.37 - 0.5;
                                        try_path(
                                            &wheel,
                                            &|t| (t * 1.2 - 0.1, cv + lean * (t - 0.4)),
                                            tried,
                                            &what,
                                        );
                                        try_path(
                                            &wheel,
                                            &|t| (cu + lean * (t - 0.6), t * 1.2 - 0.1),
                                            tried,
                                            &what,
                                        );
                                    }
                                    // Rings about the center, near it and out to where the arms end.
                                    if columns > 1 && rows > 1 {
                                        let (across, up) = (f64::from(columns - 1), f64::from(rows - 1));
                                        for r in [
                                            0.7,
                                            1.9,
                                            6.3,
                                            0.31 * f64::from(wheel.length),
                                            0.97 * f64::from(wheel.length),
                                        ] {
                                            let ring = |t: f64| {
                                                let (sin, cos) = (t * std::f64::consts::TAU).sin_cos();
                                                (cu + r * cos / across, cv + r * sin / up)
                                            };
                                            try_path(&wheel, &ring, tried, &what);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        for (style, tried) in ["smooth", "spokes"].into_iter().zip(&tried) {
            assert!(
                tried.skipped > 100_000
                    && tried.unlit_in_full > 10_000
                    && tried.lit > 100_000
                    && tried.cut_offs > 10_000,
                "{style}: {} skipped, {} unlit in full, {} lit, {} cut-offs",
                tried.skipped,
                tried.unlit_in_full,
                tried.lit,
                tried.cut_offs
            );
        }
    }
}
