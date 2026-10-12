//! The look score: how alike two renders of the same pixels look to someone watching the show.
//!
//! Fidelity here means "does the imported sequence look like it did in xLights": the right part
//! of the prop lit, as brightly, in the same colors, in the same pattern, moving the same way at
//! the same speed, to the same rhythm. It doesn't mean the same pixels, and it says nothing of
//! how either program builds an effect. Each of those six is a component from 0 (nothing alike)
//! to 1 (alike), and [`Look::score`] puts them together.
//!
//! Everything is worked out from the two renders, frame by frame, on the target's own grid
//! ([`Grid`]): a matrix's columns and rows, a tree's strings and the pixels up them, a line's
//! pixels in a row, a group's layout.
//!
//! - **Coverage**: how much of the two curves of "share of pixels lit" over time is the same.
//! - **Brightness**: the same for mean brightness, so half as bright and a fade the other
//!   doesn't have both show.
//! - **Color**: second by second, how much of the light is in the same colors (hue bins 15°
//!   wide and one for white, a hue off by under 60° counting in part).
//! - **Pattern**: where the light is in broad strokes (a grid of at most 8 by 8, the better of
//!   the pictures as they are and one moved a cell), times how alike its texture is (how alike
//!   cells are 1, 2, … 16 cells apart, which is the spacing of stripes and the size of dots).
//! - **Motion**: which way the picture moves and how fast (the move that best lays each row and
//!   column on itself a moment later), and how much of the light changes from frame to frame;
//!   the lesser of the two.
//! - **Rhythm**: whether the whole target pulses in step (the brightness with its slow curve
//!   taken off), and how often dark pixels light up; the lesser of the two.
//!
//! Each is passed through [`felt`], because people notice ratios: a fifth off passes for the
//! same, half off is half alike.
//!
//! **A render isn't asked to be more like the other than it is like itself.** Twinkles and fire
//! light different pixels on every render, so each measure asks how alike the two renders are
//! *next to how alike a render is to itself*: the pixels are split in two at random, and what
//! the two halves of one render disagree on is chance, not a difference between the renders. A
//! layout that doesn't hold still in one render (blobs coming and going) isn't asked to be where
//! the other has it, and a speed is only as sure as its readings agree.
//!
//! **What chance decides is held against PixelFlow's own renders from other seeds.** Splitting
//! the pixels allows for chance that is each pixel's own. It can't allow for chance that moves
//! many pixels together: both halves of a render see a burst go off, and no other render has
//! that burst at that moment or in that place. For an effect that draws on chance ([`Chance`]),
//! and for the layered look of one that shares its pixels with such an effect, PixelFlow renders
//! the sequence again from other seeds, and each part of the look is how alike PixelFlow's
//! renders and xLights' are *next to how alike two of PixelFlow's own renders are*
//! ([`next_to_seeds`]). What PixelFlow's renders disagree on among themselves is chance,
//! whatever it is: where a ball started, which color a burst took, how many bursts crossed a
//! prop. Where they agree, nothing is allowed.
//!
//! **Where the moments are chance, the parts it moves are compared over the whole effect.** Held
//! moment against moment, two renders of bursts at chance moments have nothing in common, and
//! that says nothing of whether the colors are right. So for such an effect
//! ([`Chance::Moments`]), a part that PixelFlow's own renders have less than half of in common
//! moment by moment is read with the order of the moments set aside:
//!
//! - **Coverage** and **brightness**: the same two curves, each sorted from its highest moment
//!   to its lowest, so they say how much of the time how much is lit and never when.
//! - **Color**: all the light of the effect sorted into the same bins, once.
//! - **Pattern**: where the light fell over the whole effect, on the same coarse grid, in place
//!   of where it is at each look; light a single burst left where it happened to go off counts
//!   for no layout of the render's own. The texture is as before.
//! - **Motion**: each render's movement averaged over its windows, and how fast it moves, in
//!   place of whether the two move the same way in each window.
//! - **Rhythm**: how often dark pixels light up. Whether the two pulse in step is not asked.
//!
//! An effect that keeps its moments, whatever else it draws by chance ([`Chance::Drawn`]), is
//! still read moment by moment, so that coming in late shows, and held against the other seeds
//! as it is.
//!
//! **And there has to be as much light.** What chance leaves in common between two renders can
//! be little, and half of little passes for little. So next to other seeds, coverage and
//! brightness are also held to how much is lit and how brightly over the whole effect: as much
//! as in PixelFlow's own seeds, or outside what those do among themselves ([`as_much`]).

pub use crate::chance::Chance;
use crate::metrics::{Limits, PIXELFLOW, Pair, XLIGHTS};
pub use pf_xlights::sequence::Verdict;
use serde::Serialize;
use std::collections::VecDeque;
use std::rc::Rc;

/// Most cells along a side of the grid motion and texture are read on.
const MAX_FINE: usize = 32;
/// Most cells along a side of the grid the layout of the picture is compared on.
const MAX_COARSE: usize = 8;
/// Hue bins, 15° each.
const HUES: usize = 24;
/// Hue bins and one for white.
const BINS: usize = HUES + 1;
const WHITE: usize = HUES;
/// How far apart (in cells) two places are compared for texture.
const LAGS: usize = 16;
/// Most cells a pattern may move between two looks at it.
const REACH: usize = 4;
/// Milliseconds between looks at the picture (layout, texture, movement).
const LOOK_MS: u32 = 80;
/// Milliseconds an envelope is smoothed over.
const ENVELOPE_MS: u32 = 500;
/// Milliseconds back a render's layout is held against itself, to see how steady it is.
const STEADY_MS: u32 = 500;
/// A picture this dim (root mean square, of 255) is dark: it has no pattern to compare.
const DARK: f64 = 2.0;
/// Structure this small next to the picture's light is taken as none (see [`structure`]).
const FLAT: f64 = 0.02;
/// A share of pixels lit this small is nothing.
const LIT_FLOOR: f64 = 0.01;
/// A brightness this low (of 1) is nothing.
const LEVEL_FLOOR: f64 = 0.004;
/// Movement this slow (target lengths a second) is none.
const STILL: f64 = 0.05;
/// A share of the light changing per frame this small is none.
const CHANGE_FLOOR: f64 = 0.01;
/// Flashes this rare (per pixel per second) are none.
const FLASH_FLOOR: f64 = 0.25;
/// A beat this faint next to the brightness (root mean square) is none.
const BEAT_FLOOR: f64 = 0.1;
/// A correlation between two looks under this says nothing of how the picture moved.
const SURE: f64 = 0.3;
/// A move has to lay the pictures on each other this much better than no move does, or the
/// picture is taken as not having moved.
const CLEARLY: f64 = 0.05;
/// How far off (in cells) one reading of a picture's move is at best.
const READING: f64 = 0.25;

/// Components within this of the weakest are as weak, when naming what is off.
const AS_WEAK: f64 = 0.05;
/// A picture moving at least this fast (target lengths a second) is taken to light its pixels
/// by moving over them.
const SWEEPING: f64 = 0.2;

/// Two of PixelFlow's own renders less alike than this in some part of their look are moved
/// more than a factor of two by chance alone: the look has too little to go on there to say
/// much.
pub const LITTLE: f64 = 0.5;
/// How many times what chance is seen to do a difference has to be before it counts.
const BEYOND_CHANCE: f64 = 1.5;

/// The share of the composite that is the weakest component: one thing badly wrong reads as a
/// different look however well the rest agrees.
const WEAKEST: f64 = 0.3;
/// At or above this score a viewer wouldn't tell the two apart at show distance.
pub const MATCHES: f64 = 85.0;
/// At or above this score (and under [`MATCHES`]) it's the same idea with noticeable differences.
pub const CLOSE: f64 = 55.0;

/// One part of how a render looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Component {
    Coverage,
    Brightness,
    Color,
    Pattern,
    Motion,
    Rhythm,
}

impl Component {
    pub const ALL: [Component; 6] = [
        Component::Coverage,
        Component::Brightness,
        Component::Color,
        Component::Pattern,
        Component::Motion,
        Component::Rhythm,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Component::Coverage => "coverage",
            Component::Brightness => "brightness",
            Component::Color => "color",
            Component::Pattern => "pattern",
            Component::Motion => "motion",
            Component::Rhythm => "rhythm",
        }
    }

    /// What the component counts for in the composite (they add up to 1). Color and pattern are
    /// what a viewer names first ("the red spiral"), so they count most; how much is lit and how
    /// brightly come next; movement matters as much as those; rhythm least, because a rhythm
    /// that is off nearly always shows in brightness or motion as well.
    pub fn weight(self) -> f64 {
        match self {
            Component::Coverage => 0.15,
            Component::Brightness => 0.15,
            Component::Color => 0.25,
            Component::Pattern => 0.20,
            Component::Motion => 0.15,
            Component::Rhythm => 0.10,
        }
    }
}

/// The six components, each 0–1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Components {
    /// How much of the target is lit, over time.
    pub coverage: f64,
    /// How bright it is, and how that rises and falls.
    pub brightness: f64,
    /// Which colors, in what shares.
    pub color: f64,
    /// Where the light is, in broad strokes, and how fine its detail is.
    pub pattern: f64,
    /// Which way the picture moves, how fast, and how much it changes.
    pub motion: f64,
    /// How often pixels flash, and whether the whole target pulses in step.
    pub rhythm: f64,
}

impl Components {
    pub fn get(&self, component: Component) -> f64 {
        match component {
            Component::Coverage => self.coverage,
            Component::Brightness => self.brightness,
            Component::Color => self.color,
            Component::Pattern => self.pattern,
            Component::Motion => self.motion,
            Component::Rhythm => self.rhythm,
        }
    }

    /// The component that agrees least. Of several about as weak (within [`AS_WEAK`]), the
    /// first: a pattern moving at half the speed also flashes each pixel half as often, and it
    /// is the motion that is off.
    pub fn weakest(&self) -> Component {
        let least = Component::ALL
            .iter()
            .map(|&c| self.get(c))
            .fold(f64::INFINITY, f64::min);
        Component::ALL
            .into_iter()
            .find(|&c| self.get(c) <= least + AS_WEAK)
            .unwrap_or(Component::Coverage)
    }

    /// The composite, 0–100: the weighted mean of the components, pulled toward the weakest.
    pub fn score(&self) -> f64 {
        let mean: f64 = Component::ALL.iter().map(|&c| c.weight() * self.get(c)).sum();
        let weakest = self.get(self.weakest());
        100.0 * ((1.0 - WEAKEST) * mean + WEAKEST * weakest)
    }
}

/// The verdict a composite score earns.
pub fn verdict(score: f64) -> Verdict {
    if score >= MATCHES {
        Verdict::Matches
    } else if score >= CLOSE {
        Verdict::Close
    } else {
        Verdict::Different
    }
}

/// What each side showed: the numbers behind the components, for saying what differs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Seen {
    /// Share of pixels lit, 0–1.
    pub lit: Pair,
    /// The share of the time nothing is lit, 0–1.
    pub dark: Pair,
    /// Mean brightness, 0–1.
    pub level: Pair,
    /// How the two brightness envelopes rise and fall together, −1 to 1; `None` when either
    /// stays level.
    pub envelope_correlation: Option<f64>,
    /// How alike the broad layout of the light is, 0–1.
    pub layout: f64,
    /// How alike the fineness of the detail is, 0–1.
    pub texture: f64,
    /// Stripes, arms or dots across the target and up it; `None` where nothing repeats.
    pub repeats_across: [Option<f64>; 2],
    pub repeats_up: [Option<f64>; 2],
    /// Movement across the target and up it, in target lengths a second.
    pub across: Pair,
    pub up: Pair,
    /// How the two movements agree: 1 the same way at the same speed, 0 unrelated or still, −1
    /// opposite.
    pub heading: f64,
    /// The share of the light that changes from one frame to the next, 0–1.
    pub change: Pair,
    /// Times a dark pixel lights up, per pixel per second (one that lights up fully counts 1,
    /// one that lights up dimly less).
    pub flashes: Pair,
    /// How alike the two pulse, 0–1 (1 when neither does).
    pub beat: f64,
}

/// How alike the two renders look over one set of pixel-frames.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Look {
    /// The composite, 0–100.
    pub score: f64,
    pub verdict: Verdict,
    pub components: Components,
    pub weakest: Component,
    pub seen: Seen,
    /// How much was looked at: pixels times seconds, and seconds.
    pub pixel_seconds: f64,
    pub seconds: f64,
    /// What chance decides in the effect, and so how it was compared: [`Chance::Nothing`]
    /// moment by moment against xLights' render alone, anything else next to PixelFlow's own
    /// renders from other seeds (and over the whole effect for [`Chance::Moments`]).
    pub chance: Chance,
    /// For a look next to PixelFlow's other seeds: how alike two of PixelFlow's own renders
    /// are in the part of the look chance moves most, 0–1 (see [`LITTLE`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub itself: Option<f64>,
}

impl Look {
    /// Why a look next to PixelFlow's other seeds says less than its score seems to; `None`
    /// when it doesn't, and for a look moment by moment.
    pub fn caveat(&self) -> Option<&'static str> {
        self.itself
            .filter(|&itself| itself < LITTLE)
            .map(|_| "says little: PixelFlow's own renders differ as much from one seed to the next")
    }

    /// What a viewer would notice first, in a few words ("moves the opposite way"); `None` when
    /// the two match.
    pub fn differs(&self) -> Option<String> {
        (self.verdict != Verdict::Matches).then(|| self.describe(self.weakest))
    }

    /// What a viewer would notice of one part of the look, in a few words.
    pub fn describe(&self, component: Component) -> String {
        let seen = &self.seen;
        let times = |ours: f64, theirs: f64| ours / theirs.max(1e-9);
        if seen.level.xlights < LEVEL_FLOOR && seen.level.pixelflow >= LEVEL_FLOOR {
            return "lit where xLights is dark".to_string();
        }
        if seen.level.pixelflow < LEVEL_FLOOR && seen.level.xlights >= LEVEL_FLOOR {
            return "dark where xLights is lit".to_string();
        }
        // Over the whole effect two renders can be lit as much, and as brightly, and still
        // differ in how that is spread over the time.
        let in_all = self.chance == Chance::Moments;
        let much_the_same =
            |pair: Pair| times(pair.pixelflow.min(pair.xlights), pair.pixelflow.max(pair.xlights)) >= 0.8;
        match component {
            Component::Coverage if in_all && much_the_same(seen.lit) => {
                if (seen.dark.pixelflow - seen.dark.xlights).abs() >= 0.1 {
                    format!(
                        "dark {:.0}% of the time against {:.0}%",
                        seen.dark.pixelflow * 100.0,
                        seen.dark.xlights * 100.0
                    )
                } else {
                    "as much lit on the whole, more of it at once or less".to_string()
                }
            }
            Component::Coverage => format!(
                "{} of the target lit ({:.0}% against {:.0}%)",
                if seen.lit.pixelflow > seen.lit.xlights {
                    "more"
                } else {
                    "less"
                },
                seen.lit.pixelflow * 100.0,
                seen.lit.xlights * 100.0
            ),
            Component::Brightness => {
                let ratio = times(seen.level.pixelflow, seen.level.xlights);
                if ratio < 0.8 {
                    format!("dimmer ({ratio:.1}× as bright)")
                } else if ratio > 1.25 {
                    format!("brighter ({ratio:.1}× as bright)")
                } else if in_all {
                    "as bright on the whole, in brighter or dimmer moments".to_string()
                } else {
                    "brightens and dims at different moments".to_string()
                }
            }
            Component::Color => "different colors".to_string(),
            Component::Pattern => {
                let repeats = |ours: Option<f64>, theirs: Option<f64>| match (ours, theirs) {
                    (Some(a), Some(b)) if (a - b).abs() > 0.25 * a.max(b) => Some((a, b)),
                    _ => None,
                };
                let [ours, theirs] = seen.repeats_across;
                let [ours_up, theirs_up] = seen.repeats_up;
                match repeats(ours, theirs).or(repeats(ours_up, theirs_up)) {
                    Some((a, b)) => format!("{a:.0} repeats where xLights has {b:.0}"),
                    None if seen.layout <= seen.texture && self.chance != Chance::Nothing => {
                        "the light falls on other parts of the target".to_string()
                    }
                    None if seen.layout <= seen.texture => "the light is laid out differently".to_string(),
                    None => "finer or coarser detail".to_string(),
                }
            }
            Component::Motion => {
                let speed = |across: f64, up: f64| across.hypot(up);
                let ours = speed(seen.across.pixelflow, seen.up.pixelflow);
                let theirs = speed(seen.across.xlights, seen.up.xlights);
                if seen.heading < -0.25 {
                    "moves the opposite way".to_string()
                } else if ours < STILL && theirs >= 2.0 * STILL {
                    "still where xLights moves".to_string()
                } else if theirs < STILL && ours >= 2.0 * STILL {
                    "moves where xLights is still".to_string()
                } else if ours >= STILL && theirs >= STILL && times(ours, theirs) < 0.8 {
                    format!("slower ({:.1}× the speed)", times(ours, theirs))
                } else if ours >= STILL && theirs >= STILL && times(ours, theirs) > 1.25 {
                    format!("faster ({:.1}× the speed)", times(ours, theirs))
                } else if ours >= STILL && theirs >= STILL && seen.heading < 0.5 {
                    "moves in another direction".to_string()
                } else if seen.change.pixelflow < seen.change.xlights {
                    "changes less from frame to frame".to_string()
                } else {
                    "changes more from frame to frame".to_string()
                }
            }
            Component::Rhythm => {
                let (ours, theirs) = (seen.flashes.pixelflow, seen.flashes.xlights);
                if ours.max(theirs) >= FLASH_FLOOR && times(ours.min(theirs), ours.max(theirs)) < 0.7 {
                    format!(
                        "flashes {} often ({ours:.1} a second per pixel against {theirs:.1})",
                        if ours > theirs { "more" } else { "less" }
                    )
                } else if in_all {
                    format!("flashes {} often", if ours > theirs { "more" } else { "less" })
                } else {
                    "pulses out of step".to_string()
                }
            }
        }
    }
}

/// Where a target's pixels fall on its own grid: the cell each is in at the two sizes the
/// picture is read at, and which half of the pixels each belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    columns: usize,
    rows: usize,
    coarse_columns: usize,
    coarse_rows: usize,
    fine: Vec<u16>,
    coarse: Vec<u16>,
    half: Vec<u8>,
}

/// A well-mixed bit from two numbers: the same on every platform.
fn coin(a: u32, b: u32) -> u8 {
    let mut x = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B) ^ 0x2545_F491;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x & 1) as u8
}

impl Grid {
    /// The grid of a target `columns` by `rows` cells whose pixels are at `at`: left (0) to
    /// right (1) and bottom (0) to top (1).
    pub fn new(at: &[(f32, f32)], columns: u32, rows: u32) -> Self {
        let (wide, tall) = (columns.max(1) as usize, rows.max(1) as usize);
        let (fine_columns, fine_rows) = (wide.min(MAX_FINE), tall.min(MAX_FINE));
        let (coarse_columns, coarse_rows) = (fine_columns.min(MAX_COARSE), fine_rows.min(MAX_COARSE));
        let place = |v: f32, cells: usize, fine: usize| {
            let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
            ((v * (cells - 1) as f32).round() as usize).min(cells - 1) * fine / cells
        };
        let mut placed = vec![0u32; coarse_columns * coarse_rows];
        let (mut fine, mut coarse, mut half) = (Vec::new(), Vec::new(), Vec::new());
        for &(u, v) in at {
            let (x, y) = (place(u, wide, fine_columns), place(v, tall, fine_rows));
            let cell = y * coarse_rows / fine_rows * coarse_columns + x * coarse_columns / fine_columns;
            // A cell's pixels go to the two halves in pairs, a coin deciding which of each pair
            // goes where, so the halves are the same size and neither follows the wiring.
            let nth = placed[cell];
            placed[cell] += 1;
            let first = coin(cell as u32, nth / 2);
            half.push(if nth % 2 == 0 { first } else { 1 - first });
            fine.push((y * fine_columns + x) as u16);
            coarse.push(cell as u16);
        }
        Self {
            columns: fine_columns,
            rows: fine_rows,
            coarse_columns,
            coarse_rows,
            fine,
            coarse,
            half,
        }
    }

    /// Pixels on the grid.
    pub fn len(&self) -> usize {
        self.fine.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fine.is_empty()
    }
}

/// The picture being added, cell by cell: shared between the sets of sums that take turns.
#[derive(Debug, Clone)]
pub struct Scratch {
    fine: [Vec<[f32; 3]>; 2],
    count: Vec<u32>,
    /// By side, then half.
    halves: [[Vec<[f32; 3]>; 2]; 2],
    half_count: [Vec<u32>; 2],
}

impl Default for Scratch {
    fn default() -> Self {
        let fine = || vec![[0.0; 3]; MAX_FINE * MAX_FINE];
        let coarse = || vec![[0.0; 3]; MAX_COARSE * MAX_COARSE];
        Self {
            fine: [fine(), fine()],
            count: vec![0; MAX_FINE * MAX_FINE],
            halves: [[coarse(), coarse()], [coarse(), coarse()]],
            half_count: [vec![0; MAX_COARSE * MAX_COARSE], vec![0; MAX_COARSE * MAX_COARSE]],
        }
    }
}

/// The frame being added.
#[derive(Debug, Clone, Copy)]
struct Now {
    pixels: u32,
    half_pixels: [u32; 2],
    /// Brightness and pixels lit, by side, then half.
    level: [[f64; 2]; 2],
    lit: [[u32; 2]; 2],
    /// Light by color, by side, then half.
    palette: [[[f32; BINS]; 2]; 2],
    change: [f64; 2],
    light: [f64; 2],
    flashes: [f64; 2],
    watched: u32,
}

impl Default for Now {
    fn default() -> Self {
        Self {
            pixels: 0,
            half_pixels: [0; 2],
            level: [[0.0; 2]; 2],
            lit: [[0; 2]; 2],
            palette: [[[0.0; BINS]; 2]; 2],
            change: [0.0; 2],
            light: [0.0; 2],
            flashes: [0.0; 2],
            watched: 0,
        }
    }
}

/// One frame: each side's brightness and share of pixels lit, and the same for each half of
/// the pixels.
#[derive(Debug, Clone, Copy)]
struct Point {
    level: [f32; 2],
    lit: [f32; 2],
    half_level: [[f32; 2]; 2],
    half_lit: [[f32; 2]; 2],
}

/// The last picture looked at, for seeing how the next has moved.
#[derive(Debug, Clone)]
struct Kept {
    frame: u32,
    cells: [Vec<[f32; 3]>; 2],
    there: Vec<bool>,
}

/// Readings of how far a picture moved between looks: the cells moved times how sure each
/// reading is, the same squared, how sure, and how many looks gave a reading.
#[derive(Debug, Clone, Copy, Default)]
struct Drift {
    moved: f64,
    squared: f64,
    sure: f64,
    readings: u32,
}

/// A weighted mean in the making.
#[derive(Debug, Clone, Copy, Default)]
struct Mean {
    sum: f64,
    weight: f64,
}

impl Mean {
    fn add(&mut self, value: f64, weight: f64) {
        self.sum += value * weight;
        self.weight += weight;
    }

    fn get(&self) -> Option<f64> {
        (self.weight > 0.0).then(|| self.sum / self.weight)
    }
}

/// How much of each part of the look two renders have the same, 0–1, before [`felt`], and how
/// much light each has: what [`next_to_seeds`] holds against the same between two of
/// PixelFlow's own renders.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parts {
    /// Moment by moment, by [`Component`] in the order of [`Component::ALL`].
    pub same: [f64; 6],
    /// How alike the layouts are moment by moment, and how the movements agree (as
    /// [`Seen::layout`] and [`Seen::heading`]).
    pub layout: f64,
    pub heading: f64,
    /// The same three over the whole effect, when [`LookStats::over`] asked for them.
    pub whole: Option<([f64; 6], f64, f64)>,
    /// Each render's share of pixels lit and its brightness, over everything added.
    pub lit: [f64; 2],
    pub level: [f64; 2],
    /// Each render's flashes a pixel a second, the share of its light that changes from frame
    /// to frame, how fast it moves on average, and its movement across and up over all its
    /// windows (the last two in target lengths a second).
    pub flashes: [f64; 2],
    pub change: [f64; 2],
    pub speed: [f64; 2],
    pub net: [[f64; 2]; 2],
}

/// How many times the spread of `renders` values a difference from their average has to be
/// before it is outside what chance does. With many renders that is [`BEYOND_CHANCE`]. A spread
/// read from two or three is itself a matter of chance, and often too small, so it is allowed
/// more: as much as keeps the odds the same (Student's t, at the odds of [`BEYOND_CHANCE`]
/// spreads of a sure one: about 87 in 100 inside).
fn beyond(renders: usize) -> f64 {
    match renders {
        0 | 1 => BEYOND_CHANCE,
        2 => 4.7,
        3 => 2.45,
        4 => 2.03,
        5 => 1.87,
        6 => 1.78,
        more => BEYOND_CHANCE + 1.4 / (more - 1) as f64,
    }
}

/// How much of a difference of `apart` counts next to `spread`, what PixelFlow's own `renders`
/// values differ by among themselves: none of it within the band chance gives ([`beyond`], and
/// a little more for their average being only so sure), all of it from twice that far, and in
/// between a share. All of it when the renders agree.
fn counted(apart: f64, spread: f64, renders: usize) -> f64 {
    let chance = beyond(renders) * spread * (1.0 + 1.0 / renders.max(1) as f64).sqrt();
    if chance > 0.0 {
        (apart / chance - 1.0).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// Whether there is as much of something in xLights' render (`theirs`) as in PixelFlow's (one
/// value for each seed), 0–1. PixelFlow's renders differ among themselves by chance, and a
/// difference from their average within what they do isn't one ([`counted`]). What counts is
/// the smaller amount over the larger, a `floor` keeping next to nothing from being told from
/// next to nothing.
fn as_much(ours: &[f64], theirs: f64, floor: f64) -> f64 {
    if ours.is_empty() {
        return 1.0;
    }
    let n = ours.len() as f64;
    let mean = ours.iter().sum::<f64>() / n;
    let spread = if ours.len() > 1 {
        (ours.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt()
    } else {
        0.0
    };
    let counts = counted((theirs - mean).abs(), spread, ours.len());
    1.0 - counts * (1.0 - (mean.min(theirs) + floor) / (mean.max(theirs) + floor))
}

/// Whether xLights' render moves the way PixelFlow's do over the whole effect (each a movement
/// across and up), 0–1: as far from the average of PixelFlow's seeds as they are from one
/// another is chance ([`counted`]). Balls that set off every way by chance drift another way in
/// every render. What counts is how far apart the two movements are next to how fast the
/// picture moves (`fastest`): opposite ways is nothing alike, half the speed a half.
fn same_way(ours: &[[f64; 2]], theirs: [f64; 2], fastest: f64) -> f64 {
    if ours.is_empty() {
        return 1.0;
    }
    let n = ours.len() as f64;
    let mean = [0, 1].map(|axis| ours.iter().map(|v| v[axis]).sum::<f64>() / n);
    let from_mean = |v: [f64; 2]| (v[0] - mean[0]).hypot(v[1] - mean[1]);
    let spread = if ours.len() > 1 {
        (ours.iter().map(|&v| from_mean(v).powi(2)).sum::<f64>() / (n - 1.0)).sqrt()
    } else {
        0.0
    };
    let apart = from_mean(theirs);
    1.0 - counted(apart, spread, ours.len()) * (apart / fastest.max(1e-9)).min(1.0)
}

/// The look of PixelFlow's render next to its own renders of the same effect from other seeds.
///
/// `between` is PixelFlow's renders against xLights' (one [`Parts`] for each seed, the render
/// `by_moment` is of among them), and `within` PixelFlow's renders against one another (one
/// for each two seeds). Each part of the look is how alike PixelFlow's renders are to xLights'
/// on average, next to how alike the two of its own that are least alike: PixelFlow isn't asked
/// to be more like xLights than it is like itself from another seed.
///
/// A part is read moment by moment. Where `chance` is the effect's moments and PixelFlow's own
/// renders have less than half of that part in common moment by moment ([`LITTLE`]), chance
/// moves it too far for the moments to say anything, and it is read over the whole effect.
///
/// The ratio allows for what chance does to *when* and *where*. It would also pass a render
/// with half the light, where chance leaves little in common to halve. So coverage and
/// brightness are, besides, no better than [`as_much`] finds of each on the whole: as large a
/// share lit and as bright as in PixelFlow's seeds, or outside what they do among themselves.
///
/// Over the whole effect, movement and rhythm are amounts and nothing else: which way and how
/// fast the picture moves, how much of it changes from frame to frame, how often dark pixels
/// light up. Each is held against PixelFlow's seeds the same way ([`same_way`], [`as_much`]),
/// which reads chance that is drawn once for the whole effect (which way three balls set off)
/// better than how alike any two renders happen to be.
///
/// What the look shows of each side ([`Look::seen`]) is from the render `by_moment` is of.
pub fn next_to_seeds(by_moment: &Look, chance: Chance, between: &[Parts], within: &[Parts]) -> Look {
    let mean = |of: &dyn Fn(&Parts) -> f64| {
        if between.is_empty() {
            1.0
        } else {
            between.iter().map(of).sum::<f64>() / between.len() as f64
        }
    };
    let least = |of: &dyn Fn(&Parts) -> f64| within.iter().map(of).fold(1.0, f64::min);
    let over_whole = |part: &Parts, at: usize| part.whole.map_or(part.same[at], |(same, _, _)| same[at]);
    let every = between.iter().chain(within).all(|part| part.whole.is_some());
    let mut same = [0.0; 6];
    let mut itself: f64 = 1.0;
    // Which parts are read over the whole effect.
    let mut whole = [false; 6];
    for component in Component::ALL {
        let at = component as usize;
        let mut own = least(&|p| p.same[at]);
        let mut alike = mean(&|p| p.same[at]);
        if chance == Chance::Moments && every && own < LITTLE {
            whole[at] = true;
            own = least(&|p| over_whole(p, at));
            alike = mean(&|p| over_whole(p, at));
        }
        itself = itself.min(own);
        let mut part = (alike / own.max(1e-6)).min(1.0);
        let amounts = |of: &dyn Fn(&Parts) -> [f64; 2], floor: f64| {
            let ours: Vec<f64> = between.iter().map(|p| of(p)[PIXELFLOW]).collect();
            as_much(&ours, mean(&|p| of(p)[XLIGHTS]), floor)
        };
        match component {
            Component::Coverage => part = part.min(amounts(&|p| p.lit, LIT_FLOOR)),
            Component::Brightness => part = part.min(amounts(&|p| p.level, LEVEL_FLOOR)),
            Component::Motion if whole[at] => {
                let ours: Vec<[f64; 2]> = between.iter().map(|p| p.net[PIXELFLOW]).collect();
                let theirs = [0, 1].map(|axis| mean(&|p| p.net[XLIGHTS][axis]));
                let fastest = mean(&|p| p.speed[PIXELFLOW]).max(mean(&|p| p.speed[XLIGHTS])) + STILL;
                part = same_way(&ours, theirs, fastest)
                    .min(amounts(&|p| p.speed, STILL))
                    .min(amounts(&|p| p.change, CHANGE_FLOOR));
            }
            Component::Rhythm if whole[at] => part = amounts(&|p| p.flashes, FLASH_FLOOR),
            _ => {}
        }
        same[at] = round(felt(part));
    }
    let components = Components {
        coverage: same[Component::Coverage as usize],
        brightness: same[Component::Brightness as usize],
        color: same[Component::Color as usize],
        pattern: same[Component::Pattern as usize],
        motion: same[Component::Motion as usize],
        rhythm: same[Component::Rhythm as usize],
    };
    let score = round(components.score());
    let layout = if whole[Component::Pattern as usize] {
        mean(&|p| p.whole.map_or(p.layout, |(_, layout, _)| layout))
    } else {
        mean(&|p| p.layout)
    };
    let heading = if whole[Component::Motion as usize] {
        mean(&|p| p.whole.map_or(p.heading, |(_, _, heading)| heading))
    } else {
        mean(&|p| p.heading)
    };
    Look {
        score,
        verdict: verdict(score),
        components,
        weakest: components.weakest(),
        seen: Seen {
            layout: round(layout),
            heading: round(heading),
            ..by_moment.seen
        },
        chance,
        itself: Some(round(itself.clamp(0.0, 1.0))),
        ..*by_moment
    }
}

/// What a look over the whole effect adds up besides, for an effect whose moments are chance.
#[derive(Debug, Clone)]
struct Whole {
    /// Light by color, by side.
    palette: [[f64; BINS]; 2],
    /// Where the light fell: each coarse cell's color added up over the frames it had pixels
    /// on, and how many frames that is. The windows are dealt in turn to two halves, and each
    /// half is added up apart: by side, then half; and by half.
    fallen: [[[[f64; 3]; MAX_COARSE * MAX_COARSE]; 2]; 2],
    fallen_frames: [[u32; MAX_COARSE * MAX_COARSE]; 2],
    /// Frames added so far.
    frames: u32,
    /// Each window's movement; by side, then axis.
    velocities: Vec<[[f64; 2]; 2]>,
}

impl Whole {
    /// One side's light as it fell over the whole effect, on the coarse grid, as a [`Coarse`]
    /// picture. Its own structure is what the two halves of its windows agree on: a burst is
    /// in one window and so in one half, and the light it left where it happened to go off
    /// counts for none, while light that keeps to one part of the prop is in both. `None` when
    /// the side was dark on the whole: as dim, all its light spread over all its time, as a
    /// picture too dim to have a pattern ([`DARK`]). A few sparks crossing a prop are that.
    fn fell(&self, side: usize, cells: usize, there: &[bool; MAX_COARSE * MAX_COARSE]) -> Option<Coarse> {
        let (sums, frames) = (&self.fallen[side], &self.fallen_frames);
        let n = there.iter().filter(|t| **t).count() as f64;
        let mut whole = [[0.0f32; 3]; MAX_COARSE * MAX_COARSE];
        let mut squares = 0.0;
        for c in (0..cells).filter(|&c| there[c]) {
            let count = f64::from(frames[0][c] + frames[1][c]);
            for k in 0..3 {
                whole[c][k] = ((sums[0][c][k] + sums[1][c][k]) / count) as f32;
            }
            squares += dot(whole[c], whole[c]);
        }
        let size = (squares / n).sqrt();
        if size < DARK {
            return None;
        }
        let scale = (1.0 / size) as f32;
        let mut mean = [0.0f64; 3];
        for c in (0..cells).filter(|&c| there[c]) {
            whole[c] = whole[c].map(|v| v * scale);
            for (sum, v) in mean.iter_mut().zip(whole[c]) {
                *sum += f64::from(v);
            }
        }
        let mean = mean.map(|v| (v / n) as f32);
        let (mut spread, mut agreed, mut halved) = (0.0, 0.0, 0.0);
        for c in (0..cells).filter(|&c| there[c]) {
            if frames[0][c] > 0 && frames[1][c] > 0 {
                let half = |h: usize| {
                    let color = sums[h][c].map(|v| (v / f64::from(frames[h][c])) as f32 * scale);
                    less(color, mean)
                };
                agreed += dot(half(0), half(1));
                halved += 1.0;
            }
            whole[c] = less(whole[c], mean);
            spread += dot(whole[c], whole[c]);
        }
        Some(Coarse {
            cells: whole,
            // An effect of a single window has only its structure as it is.
            own: if halved > 0.0 {
                (agreed / halved).max(0.0)
            } else {
                spread / n
            },
        })
    }

    /// How alike the two renders are in where their light fell over the whole effect, 0–1 (see
    /// [`agreement`]): 1 when neither put its light anywhere in particular.
    fn layout(&self, grid: &Grid) -> f64 {
        let cells = grid.coarse_columns * grid.coarse_rows;
        let mut there = [false; MAX_COARSE * MAX_COARSE];
        for (c, there) in there.iter_mut().enumerate().take(cells) {
            *there = self.fallen_frames[0][c] + self.fallen_frames[1][c] > 0;
        }
        if !there.contains(&true) {
            return 1.0;
        }
        let sides = [PIXELFLOW, XLIGHTS].map(|side| self.fell(side, cells, &there));
        agreement(grid, (&sides[PIXELFLOW], &there), (&sides[XLIGHTS], &there)).unwrap_or(1.0)
    }

    /// How alike the two renders move over the whole effect, 0–1, and how their movements
    /// agree (as [`Seen::heading`]). Each render's movement is its windows' averaged, and how
    /// fast it moves is the average of their speeds: opposite ways scores 0 and half the speed
    /// a half, as moment by moment. Each is only as sure as its windows agree with one another
    /// (something wandering every way averages out to no movement, and to another nothing in
    /// every render), and as the readings in them (`reading`, squared, for all the windows
    /// together).
    fn movement(&self, reading: f64) -> (f64, f64) {
        let n = self.velocities.len() as f64;
        if n == 0.0 {
            return (1.0, 0.0);
        }
        let mean = |of: &dyn Fn(&[[f64; 2]; 2]) -> f64| self.velocities.iter().map(of).sum::<f64>() / n;
        // How far off a mean over the windows may be, squared.
        let unsure = |of: &dyn Fn(&[[f64; 2]; 2]) -> f64, mean: f64| {
            if n < 2.0 {
                return 0.0;
            }
            self.velocities
                .iter()
                .map(|v| (of(v) - mean).powi(2))
                .sum::<f64>()
                / (n - 1.0)
                / n
        };
        let (mut net, mut speed) = ([[0.0; 2]; 2], [0.0; 2]);
        let (mut net_unsure, mut speed_unsure) = (reading, reading);
        for side in [PIXELFLOW, XLIGHTS] {
            for axis in 0..2 {
                net[side][axis] = mean(&|v| v[side][axis]);
                net_unsure += unsure(&|v| v[side][axis], net[side][axis]);
            }
            let fast = |v: &[[f64; 2]; 2]| v[side][0].hypot(v[side][1]);
            speed[side] = mean(&fast);
            speed_unsure += unsure(&fast, speed[side]);
        }
        let [ours, theirs] = net;
        let fastest = speed[PIXELFLOW].max(speed[XLIGHTS]) + STILL;
        let beyond = |apart: f64, unsure: f64| (apart - BEYOND_CHANCE * unsure.sqrt()).max(0.0);
        let way = beyond((ours[0] - theirs[0]).hypot(ours[1] - theirs[1]), net_unsure);
        let pace = beyond((speed[PIXELFLOW] - speed[XLIGHTS]).abs(), speed_unsure);
        let alike = 1.0 - (way.max(pace) / fastest).min(1.0);
        let most = (ours[0].powi(2) + ours[1].powi(2)).max(theirs[0].powi(2) + theirs[1].powi(2));
        let heading = if most > 0.0 {
            (ours[0] * theirs[0] + ours[1] * theirs[1]) / most
        } else {
            0.0
        };
        (alike, heading)
    }
}

/// Sums over one set of pixel-frames (an effect's, alone or layered), from which its [`Look`]
/// is worked out. Pixels are added frame by frame with [`LookStats::add`], each frame ended
/// with [`LookStats::end_frame`].
#[derive(Debug, Clone)]
pub struct LookStats {
    grid: Rc<Grid>,
    frame_ms: u32,
    lit: u8,
    /// Frames between looks at the picture.
    every: u32,
    /// Looks back a render's layout is held against itself.
    back: usize,
    /// Frames in a window.
    window: u32,
    /// Each pixel's brightness on each side the last time it was added, and the frame after.
    before: Vec<[u8; 2]>,
    next: Vec<u32>,
    now: Now,
    first: Option<u32>,
    frames: u32,
    samples: u64,
    series: Vec<Point>,
    /// The window being filled: light by color (side, then half), and frames.
    palette: [[[f64; BINS]; 2]; 2],
    palette_frames: u32,
    color: Mean,
    /// How alike the two layouts are, and how alike each side's is to its own a moment before.
    layout: Mean,
    steady: [Mean; 2],
    past: VecDeque<Broad>,
    /// By side, then axis: how alike cells a lag apart are (index 0: a cell and itself).
    texture: [[[f64; LAGS + 1]; 2]; 2],
    texture_looks: [f64; 2],
    kept: Option<Kept>,
    /// This window's movement, by side, then axis.
    drift: [[Drift; 2]; 2],
    drift_looks: u32,
    /// Over the windows: the two velocities' dot product (what agrees, and as it is), the
    /// faster one's speed squared, the speed squared that is within what the readings can tell,
    /// each side's velocity, and how many windows.
    agree: f64,
    heading: f64,
    fastest: f64,
    unsure: f64,
    velocity: [[f64; 2]; 2],
    speed: [f64; 2],
    windows: u32,
    change: [f64; 2],
    light: [f64; 2],
    flashes: [f64; 2],
    watched: u64,
    /// For reading the parts over the whole effect (see [`LookStats::over`]).
    whole: Option<Box<Whole>>,
}

fn hue_sixths(rgb: [u8; 3], max: u8, chroma: u8) -> f32 {
    let [r, g, b] = rgb.map(f32::from);
    let chroma = f32::from(chroma);
    if max == rgb[0] {
        ((g - b) / chroma).rem_euclid(6.0)
    } else if max == rgb[1] {
        (b - r) / chroma + 2.0
    } else {
        (r - g) / chroma + 4.0
    }
}

/// Adds a pixel's light to the colors it is: its brightness, shared between white (what it
/// lacks in saturation) and the two hue bins nearest its hue.
fn add_color(rgb: [u8; 3], palette: &mut [f32; BINS]) {
    let (max, min) = (rgb[0].max(rgb[1]).max(rgb[2]), rgb[0].min(rgb[1]).min(rgb[2]));
    if max == 0 {
        return;
    }
    let level = f32::from(max) / 255.0;
    let chroma = max - min;
    let saturation = f32::from(chroma) / f32::from(max);
    palette[WHITE] += level * (1.0 - saturation);
    if chroma > 0 {
        let at = hue_sixths(rgb, max, chroma) * (HUES as f32 / 6.0);
        let low = at.floor();
        let share = at - low;
        let low = low as usize % HUES;
        palette[low] += level * saturation * (1.0 - share);
        palette[(low + 1) % HUES] += level * saturation * share;
    }
}

/// How much of two palettes is the same light, 0–1: each as shares of its own light, matched bin
/// for bin, then with what's left in the bins 15°, 30°, and 45° of hue away counting three
/// quarters, a half, and a quarter. A hue 60° off or more doesn't match, and white matches only
/// white.
fn palette_overlap(a: &[f64; BINS], b: &[f64; BINS]) -> f64 {
    let (total_a, total_b): (f64, f64) = (a.iter().sum(), b.iter().sum());
    if total_a <= 0.0 || total_b <= 0.0 {
        return if total_a <= 0.0 && total_b <= 0.0 {
            1.0
        } else {
            0.0
        };
    }
    let mut left_a = a.map(|x| x / total_a);
    let mut left_b = b.map(|x| x / total_b);
    let mut matched = 0.0;
    let mut take = |i: usize, j: usize, worth: f64, left_a: &mut [f64; BINS], left_b: &mut [f64; BINS]| {
        let both = left_a[i].min(left_b[j]);
        matched += worth * both;
        left_a[i] -= both;
        left_b[j] -= both;
    };
    for i in 0..BINS {
        take(i, i, 1.0, &mut left_a, &mut left_b);
    }
    for away in 1..=3 {
        let worth = 1.0 - away as f64 / 4.0;
        for i in 0..HUES {
            take(i, (i + away) % HUES, worth, &mut left_a, &mut left_b);
            take(i, (i + HUES - away) % HUES, worth, &mut left_a, &mut left_b);
        }
    }
    matched.min(1.0)
}

/// How much of two series is the same, 0–1: what both have over what either has, a `floor`
/// keeping two series of next to nothing from counting as different.
fn overlap(pairs: impl Iterator<Item = (f64, f64)>, floor: f64) -> f64 {
    let (mut both, mut either) = (0.0, 0.0);
    for (a, b) in pairs {
        both += a.min(b) + floor;
        either += a.max(b) + floor;
    }
    if either > 0.0 { both / either } else { 1.0 }
}

/// How alike two things look when `same` (0–1) of them is the same. People notice ratios, and
/// small ones not at all: a fifth off passes for the same (0.97), half off is half alike, and
/// three quarters off has next to nothing in common (0.11). It is `1 / (1 + d³)`, `d` being how
/// many halvings apart the two are.
pub fn felt(same: f64) -> f64 {
    if same <= 0.0 {
        return 0.0;
    }
    let halvings = (1.0 / same.min(1.0)).log2();
    1.0 / (1.0 + halvings.powi(3))
}

/// `between` (how alike the two renders are) next to `within` (how alike each render's two
/// halves are): a render isn't asked to be more like the other than it is like itself.
fn next_to_itself(between: f64, within: [f64; 2]) -> f64 {
    let within = ((within[0] + within[1]) / 2.0).max(1e-6);
    (between / within).min(1.0)
}

/// Each value's mean with its neighbors, over `width` values centered on it.
fn smoothed(values: &[f64], width: usize) -> Vec<f64> {
    let mut sums = Vec::with_capacity(values.len() + 1);
    sums.push(0.0);
    for v in values {
        sums.push(sums[sums.len() - 1] + v);
    }
    let reach = width / 2;
    (0..values.len())
        .map(|i| {
            let (from, to) = (i.saturating_sub(reach), (i + reach + 1).min(values.len()));
            (sums[to] - sums[from]) / (to - from) as f64
        })
        .collect()
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f64 {
    f64::from(a[0] * b[0] + a[1] * b[1] + a[2] * b[2])
}

fn less(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Puts `cells` (where `there`) on a common footing: divided by their root mean square, so a
/// picture is compared by where its light is and not by how much there is, then with their mean
/// taken off. Returns what is left of the picture once the mean is gone (0 flat, toward 1 a few
/// bright cells); `None` when the picture is dark.
fn centered(cells: &mut [[f32; 3]], there: impl Fn(usize) -> bool) -> Option<f64> {
    let (mut squares, mut n) = (0.0, 0.0);
    for (c, cell) in cells.iter().enumerate() {
        if there(c) {
            squares += dot(*cell, *cell);
            n += 1.0;
        }
    }
    if n == 0.0 || (squares / n).sqrt() < DARK {
        cells.fill([0.0; 3]);
        return None;
    }
    let scale = (1.0 / (squares / n).sqrt()) as f32;
    let mut mean = [0.0f64; 3];
    for (c, cell) in cells.iter_mut().enumerate() {
        if there(c) {
            *cell = cell.map(|v| v * scale);
            for (sum, v) in mean.iter_mut().zip(*cell) {
                *sum += f64::from(v);
            }
        } else {
            *cell = [0.0; 3];
        }
    }
    let mean = mean.map(|v| (v / n) as f32);
    let mut left = 0.0;
    for (c, cell) in cells.iter_mut().enumerate() {
        if there(c) {
            *cell = less(*cell, mean);
            left += dot(*cell, *cell);
        }
    }
    Some(left / n)
}

impl LookStats {
    /// Empty sums for pixels on `grid`, from frames `frame_ms` long.
    pub fn new(grid: Rc<Grid>, frame_ms: u32, limits: &Limits) -> Self {
        let frame_ms = frame_ms.max(1);
        let pixels = grid.len();
        let every = (LOOK_MS as f32 / frame_ms as f32).round().max(1.0) as u32;
        Self {
            grid,
            frame_ms,
            lit: limits.lit,
            every,
            back: (STEADY_MS / (every * frame_ms)).max(1) as usize,
            window: (limits.window_ms / frame_ms).max(1),
            before: vec![[0; 2]; pixels],
            next: vec![0; pixels],
            now: Now::default(),
            first: None,
            frames: 0,
            samples: 0,
            series: Vec::new(),
            palette: [[[0.0; BINS]; 2]; 2],
            palette_frames: 0,
            color: Mean::default(),
            layout: Mean::default(),
            steady: [Mean::default(); 2],
            past: VecDeque::new(),
            texture: [[[0.0; LAGS + 1]; 2]; 2],
            texture_looks: [0.0; 2],
            kept: None,
            drift: [[Drift::default(); 2]; 2],
            drift_looks: 0,
            agree: 0.0,
            heading: 0.0,
            fastest: 0.0,
            unsure: 0.0,
            velocity: [[0.0; 2]; 2],
            speed: [0.0; 2],
            windows: 0,
            change: [0.0; 2],
            light: [0.0; 2],
            flashes: [0.0; 2],
            watched: 0,
            whole: None,
        }
    }

    /// Reads the parts ([`LookStats::looks`]) over the whole effect as well as moment by
    /// moment, when `chance` decides the effect's moments.
    pub fn over(mut self, chance: Chance) -> Self {
        self.whole = (chance == Chance::Moments).then(|| {
            Box::new(Whole {
                palette: [[0.0; BINS]; 2],
                fallen: [[[[0.0; 3]; MAX_COARSE * MAX_COARSE]; 2]; 2],
                fallen_frames: [[0; MAX_COARSE * MAX_COARSE]; 2],
                frames: 0,
                velocities: Vec::new(),
            })
        });
        self
    }

    /// Adds the grid's pixel `pixel` as each side drew it in frame `frame`.
    pub fn add(&mut self, scratch: &mut Scratch, frame: u32, pixel: usize, colors: [[u8; 3]; 2]) {
        let (cell, coarse) = (
            usize::from(self.grid.fine[pixel]),
            usize::from(self.grid.coarse[pixel]),
        );
        let half = usize::from(self.grid.half[pixel]);
        let followed = self.next[pixel] == frame && frame > 0;
        let now = &mut self.now;
        now.pixels += 1;
        now.half_pixels[half] += 1;
        scratch.count[cell] += 1;
        scratch.half_count[half][coarse] += 1;
        for side in [PIXELFLOW, XLIGHTS] {
            let rgb = colors[side];
            let color = rgb.map(f32::from);
            for ((fine, coarse), c) in scratch.fine[side][cell]
                .iter_mut()
                .zip(&mut scratch.halves[side][half][coarse])
                .zip(color)
            {
                *fine += c;
                *coarse += c;
            }
            let level = rgb[0].max(rgb[1]).max(rgb[2]);
            now.level[side][half] += f64::from(level);
            now.lit[side][half] += u32::from(level > self.lit);
            add_color(rgb, &mut now.palette[side][half]);
            if followed {
                let before = self.before[pixel][side];
                now.change[side] += f64::from(level.abs_diff(before));
                now.light[side] += f64::from(level) + f64::from(before);
                // A dark pixel lighting up is a flash, as big as the light that arrives: one
                // creeping over the edge of "lit" hardly counts.
                if before <= self.lit && level > self.lit {
                    now.flashes[side] += f64::from(level - before) / 255.0;
                }
            }
            self.before[pixel][side] = level;
        }
        now.watched += u32::from(followed);
        self.next[pixel] = frame + 1;
    }

    /// Ends frame `frame` (one without pixels doesn't count).
    pub fn end_frame(&mut self, scratch: &mut Scratch, frame: u32) {
        let now = std::mem::take(&mut self.now);
        if now.pixels == 0 {
            return;
        }
        self.frames += 1;
        self.samples += u64::from(now.pixels);
        let n = f64::from(now.pixels);
        let mut point = Point {
            level: [0.0; 2],
            lit: [0.0; 2],
            half_level: [[0.0; 2]; 2],
            half_lit: [[0.0; 2]; 2],
        };
        for side in [PIXELFLOW, XLIGHTS] {
            let level = (now.level[side][0] + now.level[side][1]) / 255.0 / n;
            let lit = f64::from(now.lit[side][0] + now.lit[side][1]) / n;
            point.level[side] = level as f32;
            point.lit[side] = lit as f32;
            for half in 0..2 {
                // A half with no pixels this frame stands at the whole's value.
                let pixels = f64::from(now.half_pixels[half]);
                let (half_level, half_lit) = if pixels > 0.0 {
                    (
                        now.level[side][half] / 255.0 / pixels,
                        f64::from(now.lit[side][half]) / pixels,
                    )
                } else {
                    (level, lit)
                };
                point.half_level[side][half] = half_level as f32;
                point.half_lit[side][half] = half_lit as f32;
                for (sum, light) in self.palette[side][half].iter_mut().zip(now.palette[side][half]) {
                    *sum += f64::from(light);
                }
            }
            self.change[side] += now.change[side];
            self.light[side] += now.light[side];
            self.flashes[side] += now.flashes[side];
        }
        self.watched += u64::from(now.watched);
        self.series.push(point);
        if let Some(whole) = &mut self.whole {
            for side in [PIXELFLOW, XLIGHTS] {
                for pixels in &now.palette[side] {
                    for (sum, light) in whole.palette[side].iter_mut().zip(pixels) {
                        *sum += f64::from(*light);
                    }
                }
            }
            let half = (whole.frames / self.window % 2) as usize;
            whole.frames += 1;
            let counts = &scratch.half_count;
            for c in 0..self.grid.coarse_columns * self.grid.coarse_rows {
                let pixels = counts[0][c] + counts[1][c];
                if pixels == 0 {
                    continue;
                }
                whole.fallen_frames[half][c] += 1;
                for side in [PIXELFLOW, XLIGHTS] {
                    let sums = &scratch.halves[side];
                    for (k, sum) in whole.fallen[side][half][c].iter_mut().enumerate() {
                        *sum += f64::from(sums[0][c][k] + sums[1][c][k]) / f64::from(pixels);
                    }
                }
            }
        }

        let first = *self.first.get_or_insert(frame);
        if (frame - first).is_multiple_of(self.every) {
            self.look_at(scratch, frame);
        }
        let (cells, coarse) = (
            self.grid.columns * self.grid.rows,
            self.grid.coarse_columns * self.grid.coarse_rows,
        );
        for side in 0..2 {
            scratch.fine[side][..cells].fill([0.0; 3]);
            for half in 0..2 {
                scratch.halves[side][half][..coarse].fill([0.0; 3]);
            }
        }
        scratch.count[..cells].fill(0);
        for count in &mut scratch.half_count {
            count[..coarse].fill(0);
        }

        self.palette_frames += 1;
        if self.palette_frames >= self.window {
            self.close_window();
        }
    }

    /// Ends the window being filled: its colors are compared, and its movement.
    fn close_window(&mut self) {
        if self.palette_frames == 0 {
            return;
        }
        let palette = std::mem::replace(&mut self.palette, [[[0.0; BINS]; 2]; 2]);
        self.palette_frames = 0;
        let whole = |side: usize| {
            let mut sum = [0.0; BINS];
            for half in &palette[side] {
                for (total, light) in sum.iter_mut().zip(half) {
                    *total += light;
                }
            }
            sum
        };
        let (ours, theirs) = (whole(PIXELFLOW), whole(XLIGHTS));
        let light: f64 = ours.iter().chain(&theirs).sum();
        if light > 0.0 {
            let within =
                [PIXELFLOW, XLIGHTS].map(|side| palette_overlap(&palette[side][0], &palette[side][1]));
            self.color
                .add(next_to_itself(palette_overlap(&ours, &theirs), within), light);
        }

        if self.drift_looks > 0 {
            let seconds = f64::from(self.every * self.frame_ms) / 1000.0;
            let lengths = [self.grid.columns as f64, self.grid.rows as f64];
            let mut velocity = [[0.0; 2]; 2];
            // What the speeds read may be off by, squared: the readings scatter (widely, over a
            // picture of sparkles), and the fewer there are the less their mean says. A
            // difference within that isn't one.
            let mut unsure = STILL * STILL;
            for side in [PIXELFLOW, XLIGHTS] {
                for axis in 0..2 {
                    let drift = self.drift[side][axis];
                    if drift.sure > 0.0 {
                        let cells = drift.moved / drift.sure;
                        // A picture that only now and then seems to move is mostly not moving.
                        let answered = f64::from(drift.readings) / f64::from(self.drift_looks);
                        let per_second = 1.0 / lengths[axis] / seconds;
                        velocity[side][axis] = cells * per_second * answered;
                        let scatter =
                            (drift.squared / drift.sure - cells * cells).max(0.0) + READING * READING;
                        unsure += scatter * per_second * per_second / f64::from(drift.readings);
                    }
                    self.velocity[side][axis] += velocity[side][axis];
                }
            }
            self.unsure += unsure;
            let [ours, theirs] = velocity;
            let along = ours[0] * theirs[0] + ours[1] * theirs[1];
            let speed = |v: [f64; 2]| v[0] * v[0] + v[1] * v[1];
            self.agree += along.max(0.0);
            self.heading += along;
            self.fastest += speed(ours).max(speed(theirs));
            self.speed[PIXELFLOW] += speed(ours).sqrt();
            self.speed[XLIGHTS] += speed(theirs).sqrt();
            self.windows += 1;
            if let Some(whole) = &mut self.whole {
                whole.velocities.push(velocity);
            }
        }
        self.drift = [[Drift::default(); 2]; 2];
        self.drift_looks = 0;
    }

    /// Looks at the picture in `scratch`: its layout, its texture, and how it has moved since
    /// the last look.
    fn look_at(&mut self, scratch: &mut Scratch, frame: u32) {
        let grid = Rc::clone(&self.grid);
        if let Some(now) = broad(&grid, scratch, frame) {
            let [ours, theirs] = &now.sides;
            if let Some(alike) = agreement(&grid, (ours, &now.there), (theirs, &now.there)) {
                self.layout.add(alike, 1.0);
            }
            // The same picture a moment before: a layout that doesn't hold still in one render
            // (bars sliding past, blobs coming and going) isn't asked to be where the other
            // render has it.
            let then = frame.checked_sub(self.back as u32 * self.every);
            if let Some(before) = self.past.iter().find(|b| Some(b.frame) == then) {
                for side in [PIXELFLOW, XLIGHTS] {
                    let pair = (&now.sides[side], &now.there);
                    if let Some(alike) = agreement(&grid, pair, (&before.sides[side], &before.there)) {
                        self.steady[side].add(alike, 1.0);
                    }
                }
            }
            self.past.push_back(now);
            if self.past.len() > self.back {
                self.past.pop_front();
            }
        }
        let cells = grid.columns * grid.rows;
        let there: Vec<bool> = scratch.count[..cells].iter().map(|&n| n > 0).collect();
        let mut pictures: [Vec<[f32; 3]>; 2] = [Vec::new(), Vec::new()];
        let mut lit = [false; 2];
        for side in [PIXELFLOW, XLIGHTS] {
            let mut picture: Vec<[f32; 3]> = scratch.fine[side][..cells]
                .iter()
                .zip(&scratch.count[..cells])
                .map(|(sum, &n)| {
                    if n > 0 {
                        sum.map(|v| v / n as f32)
                    } else {
                        [0.0; 3]
                    }
                })
                .collect();
            lit[side] = centered(&mut picture, |c| there[c]).is_some();
            if lit[side] {
                self.add_texture(side, &picture, &there);
            }
            pictures[side] = picture;
        }
        if let Some(kept) = &self.kept
            && kept.frame + self.every == frame
        {
            self.drift_looks += 1;
            for side in [PIXELFLOW, XLIGHTS] {
                for (axis, moved) in drift(&grid, kept, side, &pictures[side], &there)
                    .into_iter()
                    .enumerate()
                {
                    if let Some(read) = moved {
                        let drift = &mut self.drift[side][axis];
                        drift.moved += read.moved;
                        drift.squared += read.squared;
                        drift.sure += read.sure;
                        drift.readings += read.readings;
                    }
                }
            }
        }
        self.kept = Some(Kept {
            frame,
            cells: pictures,
            there,
        });
    }

    /// Adds a picture's texture: how alike cells are that are 0, 1, 2, … cells apart, across
    /// and up, as a share of the picture's light. Stripes come back alike at their spacing;
    /// scattered dots are alike only to themselves.
    fn add_texture(&mut self, side: usize, picture: &[[f32; 3]], there: &[bool]) {
        let (columns, rows) = (self.grid.columns, self.grid.rows);
        self.texture_looks[side] += 1.0;
        for (axis, (length, step)) in [(columns, 1), (rows, columns)].into_iter().enumerate() {
            for lag in 0..=LAGS.min(length - 1) {
                let (mut sum, mut pairs) = (0.0, 0u32);
                for y in 0..rows {
                    for x in 0..columns {
                        let along = if axis == 0 { x } else { y };
                        let (c, d) = (y * columns + x, y * columns + x + lag * step);
                        if along + lag < length && there[c] && there[d] {
                            sum += dot(picture[c], picture[d]);
                            pairs += 1;
                        }
                    }
                }
                if pairs > 0 {
                    self.texture[side][axis][lag] += sum / f64::from(pairs);
                }
            }
        }
    }

    /// The look of everything added, moment by moment; `None` when nothing was.
    pub fn finish(self) -> Option<Look> {
        self.looks().map(|(by_moment, _)| by_moment)
    }

    /// The look of everything added moment by moment, and how much of each part the two
    /// renders have the same, for [`next_to_seeds`]: moment by moment, and over the whole
    /// effect when [`LookStats::over`] asked. `None` when nothing was added.
    pub fn looks(mut self) -> Option<(Look, Parts)> {
        if self.frames == 0 {
            return None;
        }
        self.close_window();
        let seconds = f64::from(self.frame_ms) / 1000.0;
        let width = (ENVELOPE_MS / self.frame_ms).max(1) as usize;
        let series = |get: &dyn Fn(&Point) -> f32| -> Vec<f64> {
            self.series.iter().map(|p| f64::from(get(p))).collect()
        };
        let mean = |values: &[f64]| values.iter().sum::<f64>() / values.len() as f64;

        // Coverage and brightness: how much of the two smoothed curves is the same.
        let alike =
            |whole: &dyn Fn(&Point, usize) -> f32, half: &dyn Fn(&Point, usize, usize) -> f32, floor| {
                let envelope = |side: usize| smoothed(&series(&|p| whole(p, side)), width);
                let (ours, theirs) = (envelope(PIXELFLOW), envelope(XLIGHTS));
                let between = overlap(ours.iter().copied().zip(theirs.iter().copied()), floor);
                let within = [PIXELFLOW, XLIGHTS].map(|side| {
                    let halves = [0, 1].map(|h| smoothed(&series(&|p| half(p, side, h)), width));
                    overlap(halves[0].iter().copied().zip(halves[1].iter().copied()), floor)
                });
                (next_to_itself(between, within), ours, theirs)
            };
        let (coverage, _, _) = alike(&|p, s| p.lit[s], &|p, s, h| p.half_lit[s][h], LIT_FLOOR);
        let (brightness, ours, theirs) =
            alike(&|p, s| p.level[s], &|p, s, h| p.half_level[s][h], LEVEL_FLOOR);
        let envelope_correlation = correlation(&ours, &theirs);

        // Rhythm: what's left of the brightness once its envelope is taken off, and how often
        // pixels light up.
        let level = [PIXELFLOW, XLIGHTS].map(|side| series(&|p| p.level[side]));
        let lit = [PIXELFLOW, XLIGHTS].map(|side| mean(&series(&|p| p.lit[side])));
        let means = [mean(&level[PIXELFLOW]), mean(&level[XLIGHTS])];
        let pulse = |values: &[f64]| -> Vec<f64> {
            let envelope = smoothed(values, width);
            values.iter().zip(envelope).map(|(v, e)| v - e).collect()
        };
        let pulses = [pulse(&level[PIXELFLOW]), pulse(&level[XLIGHTS])];
        let own = |side: usize| -> f64 {
            let halves = [0, 1].map(|h| pulse(&series(&|p| p.half_level[side][h])));
            halves[0]
                .iter()
                .zip(&halves[1])
                .map(|(a, b)| a * b)
                .sum::<f64>()
                .max(0.0)
        };
        let frames = self.series.len();
        let together = (-1i64..=1)
            .map(|lag| {
                (0..frames)
                    .filter_map(|t| {
                        let other = usize::try_from(t as i64 + lag).ok().filter(|&o| o < frames)?;
                        Some(pulses[PIXELFLOW][t] * pulses[XLIGHTS][other])
                    })
                    .sum::<f64>()
            })
            .fold(0.0, f64::max);
        let faint = frames as f64 * (BEAT_FLOOR * means[0].max(means[1])).powi(2) + 1e-12;
        let beat = ((2.0 * together + faint) / (own(PIXELFLOW) + own(XLIGHTS) + faint)).min(1.0);
        let watched = self.watched as f64;
        let flashes = self.flashes.map(|n| {
            if watched > 0.0 {
                n / (watched * seconds)
            } else {
                0.0
            }
        });
        // A picture sweeping over its pixels lights each as often as it moves fast: that is
        // its motion, so two that sweep are compared at the slower one's speed.
        let speeds = self.speed.map(|sum| sum / f64::from(self.windows.max(1)));
        let slower = speeds[0].min(speeds[1]);
        let at_one_speed = |side: usize| {
            if slower >= SWEEPING {
                slower / speeds[side]
            } else {
                1.0
            }
        };
        // Counts this small differ by chance: a gap within what chance gives isn't one.
        let ours_n = self.flashes[PIXELFLOW] * at_one_speed(PIXELFLOW);
        let theirs_n = self.flashes[XLIGHTS] * at_one_speed(XLIGHTS);
        let chance = 1.5 * (ours_n + theirs_n).sqrt();
        let gap = ((ours_n - theirs_n).abs() - chance).max(0.0);
        let floor = FLASH_FLOOR * watched * seconds;
        let rate = 1.0 - gap / (ours_n.max(theirs_n) + floor).max(1e-9);
        let rhythm = beat.min(rate);

        // Motion.
        let change = [PIXELFLOW, XLIGHTS].map(|side| {
            if self.light[side] > 0.0 {
                self.change[side] / self.light[side]
            } else {
                0.0
            }
        });
        let changing = (change[0].min(change[1]) + CHANGE_FLOOR) / (change[0].max(change[1]) + CHANGE_FLOOR);
        let moving = if self.windows > 0 {
            (self.agree + self.unsure) / (self.fastest + self.unsure)
        } else {
            1.0
        };
        let windows = f64::from(self.windows.max(1));
        let velocity = self.velocity.map(|side| side.map(|v| v / windows));
        let heading = if self.fastest > 0.0 {
            self.heading / self.fastest
        } else {
            0.0
        };

        // Pattern.
        let steady = self.steady.map(|side| side.get().unwrap_or(1.0));
        let layout = next_to_itself(self.layout.get().unwrap_or(1.0), steady);
        let shown = |side: usize, axis: usize, lag: usize| {
            if self.texture_looks[side] > 0.0 {
                self.texture[side][axis][lag] / self.texture_looks[side]
            } else {
                0.0
            }
        };
        let lengths = [self.grid.columns, self.grid.rows];
        let (mut apart, mut lags) = (0.0, 0u32);
        for (axis, length) in lengths.into_iter().enumerate() {
            for lag in 1..=LAGS.min(length - 1) {
                apart += (shown(PIXELFLOW, axis, lag) - shown(XLIGHTS, axis, lag)).abs();
                lags += 1;
            }
        }
        let texture = if lags > 0 {
            (1.0 - 2.0 * apart / f64::from(lags)).max(0.0)
        } else {
            1.0
        };
        let repeats = |side: usize, axis: usize| -> Option<f64> {
            let length = lengths[axis];
            let most = LAGS.min(length - 1);
            let own = shown(side, axis, 0);
            // The first spacing at which cells are alike again, more than at the spacings
            // either side of it.
            (2..most)
                .find(|&lag| {
                    let here = shown(side, axis, lag);
                    here > 0.3 * own
                        && own > FLAT
                        && here > shown(side, axis, lag - 1)
                        && here >= shown(side, axis, lag + 1)
                })
                .map(|lag| length as f64 / lag as f64)
        };

        let components = Components {
            coverage: round(felt(coverage)),
            brightness: round(felt(brightness)),
            color: round(felt(self.color.get().unwrap_or(1.0))),
            pattern: round(felt(layout * texture)),
            motion: round(felt(moving.min(changing))),
            rhythm: round(felt(rhythm)),
        };
        let score = round(components.score());
        let pair = |values: [f64; 2]| Pair {
            pixelflow: round(values[PIXELFLOW]),
            xlights: round(values[XLIGHTS]),
        };
        let dark = [PIXELFLOW, XLIGHTS]
            .map(|side| self.series.iter().filter(|p| p.lit[side] <= 0.0).count() as f64 / frames as f64);
        let by_moment = Look {
            score,
            verdict: verdict(score),
            components,
            weakest: components.weakest(),
            seen: Seen {
                lit: pair(lit),
                dark: pair(dark),
                level: pair(means),
                envelope_correlation: envelope_correlation.map(round),
                layout: round(layout),
                texture: round(texture),
                repeats_across: [PIXELFLOW, XLIGHTS].map(|side| repeats(side, 0).map(round)),
                repeats_up: [PIXELFLOW, XLIGHTS].map(|side| repeats(side, 1).map(round)),
                across: pair([velocity[PIXELFLOW][0], velocity[XLIGHTS][0]]),
                up: pair([velocity[PIXELFLOW][1], velocity[XLIGHTS][1]]),
                heading: round(heading),
                change: pair(change),
                flashes: pair(flashes),
                beat: round(beat.clamp(0.0, 1.0)),
            },
            pixel_seconds: round(self.samples as f64 * seconds),
            seconds: round(f64::from(self.frames) * seconds),
            chance: Chance::Nothing,
            itself: None,
        };
        let mut parts = Parts {
            same: [
                coverage,
                brightness,
                self.color.get().unwrap_or(1.0),
                layout * texture,
                moving.min(changing),
                rhythm,
            ],
            layout,
            heading,
            whole: None,
            lit,
            level: means,
            flashes,
            change,
            speed: speeds,
            net: velocity,
        };
        let Some(whole) = &self.whole else {
            return Some((by_moment, parts));
        };

        // Over the whole effect: the same curves, each sorted from its highest moment to its
        // lowest; all of each render's light, once; where it fell; how it moved over all its
        // windows; and how often dark pixels lit up.
        let sorted = |of: &dyn Fn(&Point, usize) -> f32, floor: f64| {
            let [ours, theirs] = [PIXELFLOW, XLIGHTS].map(|side| smoothed(&series(&|p| of(p, side)), width));
            sorted_overlap(ours, theirs, floor)
        };
        let fell = whole.layout(&self.grid);
        let windows = whole.velocities.len() as f64;
        let reading = if windows > 0.0 {
            (self.unsure - windows * STILL * STILL).max(0.0) / (windows * windows)
        } else {
            0.0
        };
        let (moved, headed) = whole.movement(reading);
        let same = [
            sorted(&|p, s| p.lit[s], LIT_FLOOR),
            sorted(&|p, s| p.level[s], LEVEL_FLOOR),
            palette_overlap(&whole.palette[PIXELFLOW], &whole.palette[XLIGHTS]),
            fell * texture,
            moved.min(changing),
            rate,
        ];
        parts.whole = Some((same, fell, headed));
        Some((by_moment, parts))
    }
}

/// How much of two curves is the same when neither has to be in step with the other, 0–1: each
/// sorted from its highest moment to its lowest, then [`overlap`]. That is how much of the time
/// each is how high, and nothing of when. A shorter curve is read at the same shares of the way
/// along; 1 when either is empty.
fn sorted_overlap(mut a: Vec<f64>, mut b: Vec<f64>, floor: f64) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 1.0;
    }
    a.sort_by(|x, y| y.total_cmp(x));
    b.sort_by(|x, y| y.total_cmp(x));
    let n = a.len().max(b.len());
    overlap((0..n).map(|i| (a[i * a.len() / n], b[i * b.len() / n])), floor)
}

/// Pearson's correlation of two series; `None` when either barely moves.
fn correlation(a: &[f64], b: &[f64]) -> Option<f64> {
    let n = a.len().min(b.len());
    if n < 3 {
        return None;
    }
    let n_f = n as f64;
    let (mean_a, mean_b) = (a[..n].iter().sum::<f64>() / n_f, b[..n].iter().sum::<f64>() / n_f);
    let (mut aa, mut bb, mut ab) = (0.0, 0.0, 0.0);
    for (x, y) in a[..n].iter().zip(&b[..n]) {
        aa += (x - mean_a).powi(2);
        bb += (y - mean_b).powi(2);
        ab += (x - mean_a) * (y - mean_b);
    }
    let least = n_f * LEVEL_FLOOR * LEVEL_FLOOR;
    (aa > least && bb > least).then(|| (ab / (aa * bb).sqrt()).clamp(-1.0, 1.0))
}

/// One side's picture on the coarse grid: each cell's color relative to the picture's root
/// mean square, less the mean, and how much structure the picture has of its own (what its two
/// halves agree on: chance lights each half differently, so scattered dots have next to none).
#[derive(Debug, Clone, Copy)]
struct Coarse {
    cells: [[f32; 3]; MAX_COARSE * MAX_COARSE],
    own: f64,
}

/// Both sides' coarse pictures at one frame; a side that is dark has none.
#[derive(Debug, Clone)]
struct Broad {
    frame: u32,
    there: [bool; MAX_COARSE * MAX_COARSE],
    sides: [Option<Coarse>; 2],
}

/// The coarse pictures in `scratch`; `None` when both sides are dark.
fn broad(grid: &Grid, scratch: &Scratch, frame: u32) -> Option<Broad> {
    let cells = grid.coarse_columns * grid.coarse_rows;
    let counts = &scratch.half_count;
    let mut there = [false; MAX_COARSE * MAX_COARSE];
    for (c, there) in there.iter_mut().enumerate().take(cells) {
        *there = counts[0][c] + counts[1][c] > 0;
    }
    let n = there.iter().filter(|t| **t).count() as f64;
    if n == 0.0 {
        return None;
    }
    let mut sides = [None, None];
    for side in [PIXELFLOW, XLIGHTS] {
        let sums = &scratch.halves[side];
        let mut whole = [[0.0f32; 3]; MAX_COARSE * MAX_COARSE];
        let mut squares = 0.0;
        for c in (0..cells).filter(|&c| there[c]) {
            let pixels = (counts[0][c] + counts[1][c]) as f32;
            for k in 0..3 {
                whole[c][k] = (sums[0][c][k] + sums[1][c][k]) / pixels;
            }
            squares += dot(whole[c], whole[c]);
        }
        let size = (squares / n).sqrt();
        if size < DARK {
            continue;
        }
        let scale = (1.0 / size) as f32;
        let mut mean = [0.0f64; 3];
        for c in (0..cells).filter(|&c| there[c]) {
            whole[c] = whole[c].map(|v| v * scale);
            for (sum, v) in mean.iter_mut().zip(whole[c]) {
                *sum += f64::from(v);
            }
        }
        let mean = mean.map(|v| (v / n) as f32);
        let (mut spread, mut agreed, mut halved) = (0.0, 0.0, 0.0);
        for c in (0..cells).filter(|&c| there[c]) {
            if counts[0][c] > 0 && counts[1][c] > 0 {
                let half = |h: usize| less(sums[h][c].map(|v| v / counts[h][c] as f32 * scale), mean);
                agreed += dot(half(0), half(1));
                halved += 1.0;
            }
            whole[c] = less(whole[c], mean);
            spread += dot(whole[c], whole[c]);
        }
        sides[side] = Some(Coarse {
            cells: whole,
            // A target too small to halve has only its structure as it is.
            own: if halved > 0.0 {
                (agreed / halved).max(0.0)
            } else {
                spread / n
            },
        });
    }
    sides
        .iter()
        .any(Option::is_some)
        .then_some(Broad { frame, there, sides })
}

/// How alike the broad layout of two coarse pictures is, 0–1: where the light is, each picture
/// taken relative to its own brightness, the better of the pictures as they are and one moved a
/// cell any way (so a pattern a pixel off still agrees).
///
/// It is the two pictures' agreement over their own structure, `(2·ab + f) / (aa + bb + f)`,
/// where `f` ([`FLAT`]) lets two pictures with next to no structure agree. `None` when both
/// are dark.
fn agreement(
    grid: &Grid,
    a: (&Option<Coarse>, &[bool; MAX_COARSE * MAX_COARSE]),
    b: (&Option<Coarse>, &[bool; MAX_COARSE * MAX_COARSE]),
) -> Option<f64> {
    let (a, b) = match (a.0, b.0) {
        (None, None) => return None,
        (Some(one), None) | (None, Some(one)) => return Some(FLAT / (one.own + FLAT)),
        (Some(first), Some(second)) => ((first, a.1), (second, b.1)),
    };
    let (columns, rows) = (grid.coarse_columns as i64, grid.coarse_rows as i64);
    let n = a.1.iter().filter(|t| **t).count().max(1) as f64;
    let reach = |length: i64| if length >= 3 { 1i64 } else { 0 };
    let mut best = 0.0f64;
    for dy in -reach(rows)..=reach(rows) {
        for dx in -reach(columns)..=reach(columns) {
            let mut sum = 0.0;
            for y in 0..rows {
                for x in 0..columns {
                    let (ox, oy) = (x + dx, y + dy);
                    if ox < 0 || oy < 0 || ox >= columns || oy >= rows {
                        continue;
                    }
                    let (c, d) = ((y * columns + x) as usize, (oy * columns + ox) as usize);
                    if a.1[c] && b.1[d] {
                        sum += dot(a.0.cells[c], b.0.cells[d]);
                    }
                }
            }
            best = best.max(sum / n);
        }
    }
    Some(((2.0 * best + FLAT) / (a.0.own + b.0.own + FLAT)).min(1.0))
}

/// How far `side`'s picture has moved since the one kept, across and up. Each line of the grid
/// along the way of the move (a row for across, a column for up) gives a reading of its own:
/// the move, up to [`REACH`] cells either way, that best lays the line as it was on the line as
/// it is. The readings are added up by how much each line holds and how sure it is, so a bright
/// thing standing still in one corner says "still" for its own lines only, and whatever moves
/// across the rest is still read. `None` where no line says anything: a flat picture, or one
/// unlike itself wherever it's laid.
fn drift(grid: &Grid, kept: &Kept, side: usize, picture: &[[f32; 3]], there: &[bool]) -> [Option<Drift>; 2] {
    let (columns, rows) = (grid.columns, grid.rows);
    let before = &kept.cells[side];
    let mut moved = [None, None];
    for (axis, (length, lines)) in [(columns, rows), (rows, columns)].into_iter().enumerate() {
        let reach = REACH.min((length - 1) / 2) as i64;
        if reach == 0 {
            continue;
        }
        // The cell `along` a line.
        let cell = |line: usize, along: usize| {
            if axis == 0 {
                line * columns + along
            } else {
                along * columns + line
            }
        };
        let mut drift = Drift::default();
        for line in 0..lines {
            let held = |cells: &[[f32; 3]], there: &[bool]| -> f64 {
                (0..length)
                    .map(|i| cell(line, i))
                    .filter(|&c| there[c])
                    .map(|c| dot(cells[c], cells[c]))
                    .sum()
            };
            let scale = (held(before, &kept.there) * held(picture, there)).sqrt();
            // A line with next to nothing on it (next to the picture's own light) says nothing.
            if scale < FLAT * length as f64 {
                continue;
            }
            let at = |lag: i64| -> f64 {
                let mut sum = 0.0;
                for i in 0..length as i64 {
                    let j = i + lag;
                    if j < 0 || j >= length as i64 {
                        continue;
                    }
                    let (c, d) = (cell(line, i as usize), cell(line, j as usize));
                    if kept.there[c] && there[d] {
                        sum += dot(before[c], picture[d]);
                    }
                }
                sum / scale
            };
            let alike: Vec<f64> = (-reach..=reach).map(at).collect();
            let at = |lag: i64| alike[(lag + reach) as usize];
            let mut best = 0i64;
            for lag in -reach..=reach {
                // The smaller move on a tie.
                if at(lag) - 1e-3 * lag.abs() as f64 > at(best) - 1e-3 * best.abs() as f64 {
                    best = lag;
                }
            }
            // A broad, smooth line lies on itself nearly as well wherever it's put: only a move
            // that is clearly better than staying counts as one.
            if at(best) < at(0) + CLEARLY {
                best = 0;
            }
            if at(best) < SURE {
                continue;
            }
            let mut cells = best as f64;
            if best.abs() < reach {
                let curve = at(best - 1) - 2.0 * at(best) + at(best + 1);
                if curve < -1e-9 {
                    cells += (0.5 * (at(best - 1) - at(best + 1)) / curve).clamp(-0.5, 0.5);
                }
            }
            let sure = at(best) * scale;
            drift.moved += cells * sure;
            drift.squared += cells * cells * sure;
            drift.sure += sure;
        }
        if drift.sure > 0.0 {
            drift.readings = 1;
            moved[axis] = Some(drift);
        }
    }
    moved
}

fn round(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLACK: [u8; 3] = [0; 3];
    const WHITE: [u8; 3] = [255; 3];
    const RED: [u8; 3] = [255, 0, 0];
    const GREEN: [u8; 3] = [0, 255, 0];

    /// A matrix `columns` wide and `rows` tall, its pixels row by row from the bottom left.
    fn matrix(columns: u32, rows: u32) -> Rc<Grid> {
        let along = |i: u32, n: u32| if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 };
        let at: Vec<(f32, f32)> = (0..rows)
            .flat_map(|y| (0..columns).map(move |x| (along(x, columns), along(y, rows))))
            .collect();
        Rc::new(Grid::new(&at, columns, rows))
    }

    /// The look of two renders of `frames` frames (25 ms each) on a matrix: each side's color
    /// for a pixel is `draw(side, frame, x, y)`.
    fn look(columns: u32, rows: u32, frames: u32, draw: impl Fn(usize, u32, u32, u32) -> [u8; 3]) -> Look {
        let mut stats = LookStats::new(matrix(columns, rows), 25, &Limits::default());
        let mut scratch = Scratch::default();
        for frame in 0..frames {
            for y in 0..rows {
                for x in 0..columns {
                    let colors = [PIXELFLOW, XLIGHTS].map(|side| draw(side, frame, x, y));
                    stats.add(&mut scratch, frame, (y * columns + x) as usize, colors);
                }
            }
            stats.end_frame(&mut scratch, frame);
        }
        stats.finish().expect("frames were added")
    }

    /// Bars `width` pixels wide with as much dark between them, moving `per_frame` pixels a
    /// frame to the right.
    fn bars(width: u32, per_frame: f32, color: [u8; 3]) -> impl Fn(u32, u32) -> [u8; 3] {
        move |frame, x| {
            let at = (x as f32 - frame as f32 * per_frame).rem_euclid(2.0 * width as f32);
            if at < width as f32 { color } else { BLACK }
        }
    }

    /// A number in 0..1 from a seed and a place: the same on every platform.
    fn chance(seed: u32, a: u32, b: u32) -> f32 {
        let mut x =
            seed.wrapping_mul(0x9E37_79B9) ^ a.wrapping_mul(0x85EB_CA6B) ^ b.wrapping_mul(0xC2B2_AE35);
        x ^= x >> 16;
        x = x.wrapping_mul(0x7FEB_352D);
        x ^= x >> 15;
        x = x.wrapping_mul(0x846C_A68B);
        x ^= x >> 16;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Twinkles: each pixel is lit for one stretch of `hold` frames in every `every`, starting
    /// at its own random moment.
    fn twinkle(seed: u32, every: u32, hold: u32, color: [u8; 3]) -> impl Fn(u32, u32, u32) -> [u8; 3] {
        move |frame, x, y| {
            let pixel = y * 1000 + x;
            let cycle = (frame + (chance(seed, pixel, 0) * every as f32) as u32) / every;
            let start = (chance(seed, pixel, cycle + 1) * (every - hold) as f32) as u32;
            let at = (frame + (chance(seed, pixel, 0) * every as f32) as u32) % every;
            if at >= start && at < start + hold {
                color
            } else {
                BLACK
            }
        }
    }

    #[test]
    fn two_renders_that_are_the_same_match_on_every_component() {
        let draw = bars(4, 0.5, RED);
        let look = look(32, 8, 200, |_, frame, x, _| draw(frame, x));
        for component in Component::ALL {
            assert!(look.components.get(component) > 0.99, "{component:?}: {look:?}");
        }
        assert!(look.score > 99.0);
        assert_eq!((look.verdict, look.differs()), (Verdict::Matches, None));
    }

    #[test]
    fn a_pattern_one_pixel_over_is_the_same_pattern() {
        let draw = bars(4, 0.0, WHITE);
        let look = look(32, 8, 80, |side, frame, x, _| draw(frame, x + side as u32));
        assert!(look.components.pattern > 0.9, "{look:?}");
        assert_eq!(look.verdict, Verdict::Matches);
    }

    #[test]
    fn a_pattern_moving_the_other_way_fails_on_motion_alone() {
        let draw = bars(4, 0.5, WHITE);
        let look = look(32, 8, 200, |side, frame, x, _| {
            if side == PIXELFLOW {
                draw(frame, x)
            } else {
                draw(frame, 31 - x)
            }
        });
        assert!(look.components.motion < 0.1, "{look:?}");
        assert!(look.seen.heading < -0.9, "{look:?}");
        assert_eq!(look.weakest, Component::Motion);
        for component in [Component::Coverage, Component::Brightness, Component::Color] {
            assert!(look.components.get(component) > 0.95, "{component:?}: {look:?}");
        }
        assert_ne!(look.verdict, Verdict::Matches);
        assert_eq!(look.differs().as_deref(), Some("moves the opposite way"));
    }

    #[test]
    fn half_the_speed_is_half_the_motion() {
        let (ours, theirs) = (bars(4, 0.25, WHITE), bars(4, 0.5, WHITE));
        let look = look(32, 8, 400, |side, frame, x, _| {
            if side == PIXELFLOW {
                ours(frame, x)
            } else {
                theirs(frame, x)
            }
        });
        assert!((0.35..0.65).contains(&look.components.motion), "{look:?}");
        let (slow, fast) = (look.seen.across.pixelflow, look.seen.across.xlights);
        assert!((0.4..0.6).contains(&(slow / fast)), "{slow} against {fast}");
        assert_eq!(look.verdict, Verdict::Close);
        assert!(
            look.differs().is_some_and(|d| d.starts_with("slower")),
            "{:?}",
            look.differs()
        );
    }

    #[test]
    fn still_against_moving_is_caught() {
        let (ours, theirs) = (bars(4, 0.0, WHITE), bars(4, 0.5, WHITE));
        let look = look(32, 8, 200, |side, frame, x, _| {
            if side == PIXELFLOW {
                ours(frame, x)
            } else {
                theirs(frame, x)
            }
        });
        assert!(look.components.motion < 0.1, "{look:?}");
        assert_eq!(look.differs().as_deref(), Some("still where xLights moves"));
        assert_ne!(look.verdict, Verdict::Matches);
    }

    #[test]
    fn a_shifted_hue_costs_color_by_how_far_it_is() {
        // Red against orange (30° away), then against green (120° away).
        let orange = [255, 128, 0];
        let near = look(
            16,
            4,
            80,
            |side, _, _, _| if side == PIXELFLOW { orange } else { RED },
        );
        assert!((0.4..0.6).contains(&near.components.color), "{near:?}");
        assert_eq!(near.weakest, Component::Color);
        assert_eq!(near.verdict, Verdict::Close);
        let far = look(
            16,
            4,
            80,
            |side, _, _, _| if side == PIXELFLOW { GREEN } else { RED },
        );
        assert!(far.components.color < 0.01, "{far:?}");
        assert_eq!(far.verdict, Verdict::Different);
        assert_eq!(far.differs().as_deref(), Some("different colors"));
        // Everything but the color is the same.
        for component in [Component::Coverage, Component::Brightness, Component::Pattern] {
            assert!(far.components.get(component) > 0.99, "{component:?}: {far:?}");
        }
    }

    #[test]
    fn half_the_brightness_is_caught_and_is_still_the_same_colors() {
        let look = look(
            16,
            4,
            80,
            |side, _, _, _| {
                if side == PIXELFLOW { [127, 0, 0] } else { RED }
            },
        );
        assert!((0.45..0.55).contains(&look.components.brightness), "{look:?}");
        assert!(
            look.components.color > 0.99 && look.components.coverage > 0.99,
            "{look:?}"
        );
        assert_eq!(
            (look.weakest, look.verdict),
            (Component::Brightness, Verdict::Close)
        );
        assert!(
            look.differs().is_some_and(|d| d.starts_with("dimmer")),
            "{:?}",
            look.differs()
        );
    }

    #[test]
    fn a_different_number_of_bands_costs_pattern() {
        let (ours, theirs) = (bars(2, 0.0, WHITE), bars(4, 0.0, WHITE));
        let look = look(32, 8, 80, |side, frame, x, _| {
            if side == PIXELFLOW {
                ours(frame, x)
            } else {
                theirs(frame, x)
            }
        });
        assert!(look.components.pattern < 0.5, "{look:?}");
        assert_eq!(look.weakest, Component::Pattern);
        assert_eq!(look.seen.repeats_across, [Some(8.0), Some(4.0)]);
        assert_eq!(look.differs().as_deref(), Some("8 repeats where xLights has 4"));
        // As much of it is lit, as brightly.
        assert!(
            look.components.coverage > 0.95 && look.components.brightness > 0.95,
            "{look:?}"
        );
    }

    #[test]
    fn the_same_random_twinkle_from_two_seeds_matches() {
        let (ours, theirs) = (twinkle(1, 40, 8, WHITE), twinkle(2, 40, 8, WHITE));
        let look = look(24, 12, 400, |side, frame, x, y| {
            if side == PIXELFLOW {
                ours(frame, x, y)
            } else {
                theirs(frame, x, y)
            }
        });
        assert_eq!(look.verdict, Verdict::Matches, "{look:?}");
        for component in Component::ALL {
            assert!(look.components.get(component) > 0.8, "{component:?}: {look:?}");
        }
    }

    #[test]
    fn a_denser_or_faster_twinkle_is_not_the_same_twinkle() {
        let theirs = twinkle(2, 40, 8, WHITE);
        // Lit three times as long in each cycle: three times as much of the prop is lit.
        let dense = twinkle(1, 40, 24, WHITE);
        let denser = look(24, 12, 400, |side, frame, x, y| {
            if side == PIXELFLOW {
                dense(frame, x, y)
            } else {
                theirs(frame, x, y)
            }
        });
        assert!(denser.components.coverage < 0.5, "{denser:?}");
        assert_ne!(denser.verdict, Verdict::Matches);
        // Four times as often, for a quarter as long: as much is lit, but it flickers faster.
        let quick = twinkle(1, 10, 2, WHITE);
        let quicker = look(24, 12, 400, |side, frame, x, y| {
            if side == PIXELFLOW {
                quick(frame, x, y)
            } else {
                theirs(frame, x, y)
            }
        });
        assert!(quicker.components.coverage > 0.8, "{quicker:?}");
        assert!(quicker.components.rhythm < 0.5, "{quicker:?}");
        assert_ne!(quicker.verdict, Verdict::Matches);
    }

    #[test]
    fn a_strobe_out_of_step_or_at_another_rate_costs_rhythm() {
        let strobe = |every: u32, from: u32| {
            move |frame: u32| {
                if (frame + from) % every < every / 2 {
                    WHITE
                } else {
                    BLACK
                }
            }
        };
        let (a, b) = (strobe(8, 0), strobe(8, 0));
        let same = look(
            16,
            4,
            240,
            |side, frame, _, _| if side == PIXELFLOW { a(frame) } else { b(frame) },
        );
        assert!(same.components.rhythm > 0.99, "{same:?}");
        let late = strobe(8, 4);
        let off = look(
            16,
            4,
            240,
            |side, frame, _, _| if side == PIXELFLOW { late(frame) } else { b(frame) },
        );
        assert!(off.components.rhythm < 0.2, "{off:?}");
        let fast = strobe(4, 0);
        let faster = look(
            16,
            4,
            240,
            |side, frame, _, _| if side == PIXELFLOW { fast(frame) } else { b(frame) },
        );
        assert!(faster.components.rhythm < 0.6, "{faster:?}");
        let steady = look(
            16,
            4,
            240,
            |side, frame, _, _| if side == PIXELFLOW { [128; 3] } else { b(frame) },
        );
        assert!(steady.components.rhythm < 0.2, "{steady:?}");
        assert_ne!(steady.verdict, Verdict::Matches);
    }

    #[test]
    fn light_in_another_part_of_the_prop_costs_pattern() {
        // The left half lit against the top half.
        let look = look(16, 16, 40, |side, _, x, y| {
            let lit = if side == PIXELFLOW { x < 8 } else { y >= 8 };
            if lit { WHITE } else { BLACK }
        });
        assert!(look.components.pattern < 0.2, "{look:?}");
        assert!(
            look.components.coverage > 0.99 && look.components.brightness > 0.99,
            "{look:?}"
        );
        assert_eq!(look.weakest, Component::Pattern);
    }

    #[test]
    fn a_fade_against_a_steady_light_costs_brightness() {
        let look = look(16, 4, 200, |side, frame, _, _| {
            if side == PIXELFLOW {
                [(frame * 255 / 199) as u8; 3]
            } else {
                WHITE
            }
        });
        assert!((0.3..0.7).contains(&look.components.brightness), "{look:?}");
        assert!(look.components.color > 0.99, "{look:?}");
    }

    #[test]
    fn nothing_where_xlights_shows_something_is_different() {
        let draw = bars(4, 0.5, RED);
        let look = look(
            32,
            8,
            80,
            |side, frame, x, _| {
                if side == PIXELFLOW { BLACK } else { draw(frame, x) }
            },
        );
        assert!(
            look.components.coverage < 0.05 && look.components.brightness < 0.05,
            "{look:?}"
        );
        assert_eq!(look.verdict, Verdict::Different);
    }

    #[test]
    fn the_composite_is_the_weighted_mean_pulled_toward_the_weakest() {
        let all = |value: f64| Components {
            coverage: value,
            brightness: value,
            color: value,
            pattern: value,
            motion: value,
            rhythm: value,
        };
        assert!((Component::ALL.iter().map(|c| c.weight()).sum::<f64>() - 1.0).abs() < 1e-9);
        assert!((all(1.0).score() - 100.0).abs() < 1e-9);
        assert!(all(0.0).score().abs() < 1e-9);
        // One component at nothing: 70% of what the rest add up to.
        let no_motion = Components {
            motion: 0.0,
            ..all(1.0)
        };
        assert!((no_motion.score() - 70.0 * 0.85).abs() < 1e-9);
        assert_eq!(no_motion.weakest(), Component::Motion);
        let no_color = Components {
            color: 0.0,
            ..all(1.0)
        };
        assert!((no_color.score() - 70.0 * 0.75).abs() < 1e-9);
        // The same shortfall costs more in color than in rhythm.
        let off = |component: Component| {
            let mut c = all(1.0);
            match component {
                Component::Color => c.color = 0.5,
                _ => c.rhythm = 0.5,
            }
            c.score()
        };
        assert!(off(Component::Color) < off(Component::Rhythm));
    }

    #[test]
    fn a_small_difference_isnt_felt_and_a_large_one_is_felt_in_full() {
        assert_eq!(
            (felt(1.0), felt(0.0), felt(-1.0), felt(2.0)),
            (1.0, 0.0, 0.0, 1.0)
        );
        assert!((felt(0.5) - 0.5).abs() < 1e-9);
        // A fifth off passes; three quarters off has next to nothing in common.
        assert!(felt(0.8) > 0.95 && felt(0.9) > 0.99);
        assert!(felt(0.25) < 0.15);
        // More of it the same never looks less alike.
        let steps: Vec<f64> = (0..=20).map(|i| felt(f64::from(i) / 20.0)).collect();
        assert!(steps.windows(2).all(|pair| pair[0] <= pair[1]), "{steps:?}");
    }

    #[test]
    fn verdicts_follow_the_score() {
        assert_eq!(verdict(100.0), Verdict::Matches);
        assert_eq!(verdict(MATCHES + 1.0), Verdict::Matches);
        assert_eq!(verdict(MATCHES - 1.0), Verdict::Close);
        assert_eq!(verdict(CLOSE + 1.0), Verdict::Close);
        assert_eq!(verdict(CLOSE - 1.0), Verdict::Different);
        assert_eq!(verdict(0.0), Verdict::Different);
    }

    #[test]
    fn a_grid_follows_the_target_and_splits_its_pixels_evenly() {
        // A 100 by 3 matrix is read 32 wide and 3 tall, and laid out 8 wide.
        let grid = matrix(100, 3);
        assert_eq!(
            (grid.columns, grid.rows, grid.coarse_columns, grid.coarse_rows),
            (32, 3, 8, 3)
        );
        assert_eq!(grid.len(), 300);
        // Each of the 24 coarse cells splits its pixels as evenly as their number allows.
        let ones = grid.half.iter().filter(|&&h| h == 1).count();
        assert!(ones.abs_diff(150) <= 12, "{ones}");
        // A single line has one row and nothing to move up.
        let line = matrix(10, 1);
        assert_eq!((line.columns, line.rows), (10, 1));
    }

    #[test]
    fn palettes_overlap_by_how_near_their_hues_are() {
        let of = |colors: &[[u8; 3]]| {
            let mut palette = [0.0f32; BINS];
            for &c in colors {
                add_color(c, &mut palette);
            }
            palette.map(f64::from)
        };
        let same = palette_overlap(&of(&[RED, GREEN]), &of(&[GREEN, RED]));
        assert!((same - 1.0).abs() < 1e-6);
        // Half the palette in common.
        let half = palette_overlap(&of(&[RED, GREEN]), &of(&[RED, [0, 0, 255]]));
        assert!((half - 0.5).abs() < 1e-6, "{half}");
        // Red and yellow stripes aren't orange.
        let mixed = palette_overlap(&of(&[RED, [255, 255, 0]]), &of(&[[255, 128, 0]]));
        assert!((0.4..0.6).contains(&mixed), "{mixed}");
        // White matches no color, and a pastel half matches both.
        assert!(palette_overlap(&of(&[WHITE]), &of(&[RED])) < 1e-6);
        let pastel = palette_overlap(&of(&[[255, 128, 128]]), &of(&[RED]));
        assert!((0.4..0.6).contains(&pastel), "{pastel}");
        // Dimmer is the same color.
        assert!((palette_overlap(&of(&[[100, 0, 0]]), &of(&[RED])) - 1.0).abs() < 1e-6);
    }

    /// Renders in a made-up test: PixelFlow's (0 the one scored, then its other seeds) and, last,
    /// xLights'.
    const OTHER_SEEDS: usize = 2;
    const THEIRS: usize = OTHER_SEEDS + 1;

    /// The look of `frames` frames (25 ms each) on a matrix, moment by moment and next to
    /// PixelFlow's other seeds as an effect in which chance decides `chance`: `draw(render,
    /// frame, x, y)` is the pixel in PixelFlow's render from seed `render`, or in xLights' for
    /// [`THEIRS`].
    fn held(
        columns: u32,
        rows: u32,
        frames: u32,
        chance: Chance,
        draw: impl Fn(usize, u32, u32, u32) -> [u8; 3],
    ) -> (Look, Look) {
        let between = |a: usize, b: usize| {
            let mut stats = LookStats::new(matrix(columns, rows), 25, &Limits::default()).over(chance);
            let mut scratch = Scratch::default();
            for frame in 0..frames {
                for y in 0..rows {
                    for x in 0..columns {
                        let colors = [draw(a, frame, x, y), draw(b, frame, x, y)];
                        stats.add(&mut scratch, frame, (y * columns + x) as usize, colors);
                    }
                }
                stats.end_frame(&mut scratch, frame);
            }
            stats.looks().expect("frames were added")
        };
        let (by_moment, first) = between(0, THEIRS);
        let mut with_theirs = vec![first];
        let mut own = Vec::new();
        for seed in 1..=OTHER_SEEDS {
            with_theirs.push(between(seed, THEIRS).1);
            for other in 0..seed {
                own.push(between(other, seed).1);
            }
        }
        (by_moment, next_to_seeds(&by_moment, chance, &with_theirs, &own))
    }

    /// Bursts at chance moments in chance places: time is cut into stretches of `every` frames,
    /// and in each a disc `radius` pixels across lights up at a moment and a place of its own,
    /// in one of `colors`, and fades over `glow` frames.
    #[derive(Clone, Copy)]
    struct Bursts {
        every: u32,
        glow: u32,
        radius: f32,
        colors: &'static [[u8; 3]],
        /// Every burst here instead of at a chance place.
        at: Option<(f32, f32)>,
        level: f32,
    }

    impl Bursts {
        const USUAL: Bursts = Bursts {
            every: 20,
            glow: 12,
            radius: 7.0,
            colors: &[RED, GREEN, WHITE],
            at: None,
            level: 1.0,
        };

        fn draw(self, seed: u32, size: (u32, u32), frame: u32, x: u32, y: u32) -> [u8; 3] {
            let mut most = BLACK;
            // The stretch the frame is in, and the one before (its burst may still glow).
            for stretch in (frame / self.every).saturating_sub(1)..=frame / self.every {
                let start = stretch * self.every
                    + (chance(seed, stretch, 1) * (self.every - self.glow.min(self.every - 1)) as f32) as u32;
                let Some(age) = frame.checked_sub(start).filter(|&age| age < self.glow) else {
                    continue;
                };
                let (cx, cy) = self.at.unwrap_or((
                    chance(seed, stretch, 2) * size.0 as f32,
                    chance(seed, stretch, 3) * size.1 as f32,
                ));
                if (x as f32 - cx).hypot(y as f32 - cy) > self.radius {
                    continue;
                }
                let color = self.colors[(chance(seed, stretch, 4) * self.colors.len() as f32) as usize];
                let fade = self.level * (1.0 - age as f32 / self.glow as f32);
                let lit = color.map(|c| (f32::from(c) * fade) as u8);
                if lit.iter().max() > most.iter().max() {
                    most = lit;
                }
            }
            most
        }
    }

    const SIZE: (u32, u32) = (32, 16);
    const LONG: u32 = 2400;

    /// PixelFlow shows `ours` and xLights `theirs`, every render from a seed of its own.
    fn bursts(ours: Bursts, theirs: Bursts, frames: u32) -> (Look, Look) {
        held(SIZE.0, SIZE.1, frames, Chance::Moments, |render, frame, x, y| {
            let of = if render == THEIRS { theirs } else { ours };
            of.draw(100 + render as u32, SIZE, frame, x, y)
        })
    }

    #[test]
    fn bursts_at_other_moments_fail_moment_by_moment_and_match_next_to_other_seeds() {
        let (by_moment, held) = bursts(Bursts::USUAL, Bursts::USUAL, LONG);
        // Second by second the two never pulse together, and that alone makes them look apart.
        assert!(by_moment.components.rhythm < 0.5, "{by_moment:?}");
        assert_ne!(by_moment.verdict, Verdict::Matches);
        assert_eq!((by_moment.chance, by_moment.itself), (Chance::Nothing, None));
        // As many bursts, as big, as bright, in the same colors: the same look.
        for component in Component::ALL {
            assert!(held.components.get(component) > 0.9, "{component:?}: {held:?}");
        }
        assert_eq!(
            (held.verdict, held.chance, held.differs()),
            (Verdict::Matches, Chance::Moments, None)
        );
        // A hundred and twenty bursts: PixelFlow's own renders are much alike, and the look
        // can be taken at its word.
        assert!(held.itself.is_some_and(|itself| itself > LITTLE), "{held:?}");
        assert_eq!(held.caveat(), None);
        // What each side showed is the same in both looks.
        assert_eq!(held.seen.lit, by_moment.seen.lit);
        assert_eq!(
            (held.pixel_seconds, held.seconds),
            (by_moment.pixel_seconds, by_moment.seconds)
        );
    }

    #[test]
    fn twice_or_half_as_many_bursts_is_not_the_same_look() {
        for (every, more) in [(10, true), (40, false)] {
            let ours = Bursts {
                every,
                ..Bursts::USUAL
            };
            let (_, held) = bursts(ours, Bursts::USUAL, LONG);
            assert!(held.components.coverage < 0.8, "{every}: {held:?}");
            assert!(held.components.brightness < 0.8, "{every}: {held:?}");
            assert!(held.components.color > 0.9, "{every}: {held:?}");
            assert_ne!(held.verdict, Verdict::Matches, "{every}: {held:?}");
            let lit = held.seen.lit;
            assert_eq!(lit.pixelflow > lit.xlights, more, "{held:?}");
        }
    }

    #[test]
    fn bursts_in_another_color_or_half_as_bright_are_caught_over_the_whole_effect() {
        let green = Bursts {
            colors: &[GREEN],
            ..Bursts::USUAL
        };
        let red = Bursts {
            colors: &[RED],
            ..Bursts::USUAL
        };
        let (_, recolored) = bursts(green, red, LONG);
        assert!(recolored.components.color < 0.01, "{recolored:?}");
        assert_eq!(
            (recolored.verdict, recolored.differs().as_deref()),
            (Verdict::Different, Some("different colors"))
        );
        assert!(recolored.components.coverage > 0.9, "{recolored:?}");
        let dim = Bursts {
            level: 0.5,
            ..Bursts::USUAL
        };
        let (_, dimmer) = bursts(dim, Bursts::USUAL, LONG);
        assert!((0.3..0.8).contains(&dimmer.components.brightness), "{dimmer:?}");
        assert_eq!(dimmer.weakest, Component::Brightness);
        assert_ne!(dimmer.verdict, Verdict::Matches, "{dimmer:?}");
        assert!(
            dimmer.differs().is_some_and(|d| d.starts_with("dimmer")),
            "{:?}",
            dimmer.differs()
        );
    }

    #[test]
    fn steady_light_where_there_should_be_bursts_with_dark_between_is_caught() {
        // A burst every two seconds, dark the rest of the time, against every pixel glowing
        // dimly all the time.
        let sparse = Bursts {
            every: 80,
            ..Bursts::USUAL
        };
        let (_, held) = held(SIZE.0, SIZE.1, LONG, Chance::Moments, |render, frame, x, y| {
            if render == THEIRS {
                sparse.draw(9, SIZE, frame, x, y)
            } else {
                [40; 3]
            }
        });
        assert!(held.components.coverage < 0.2, "{held:?}");
        assert_eq!(held.verdict, Verdict::Different, "{held:?}");
        // xLights is dark most of the time, and PixelFlow never.
        assert!(
            held.seen.dark.xlights > 0.5 && held.seen.dark.pixelflow == 0.0,
            "{held:?}"
        );
    }

    #[test]
    fn every_burst_in_one_place_is_not_bursts_all_over() {
        let middle = Bursts {
            at: Some((16.0, 8.0)),
            ..Bursts::USUAL
        };
        let (_, held) = bursts(middle, Bursts::USUAL, LONG);
        assert!(held.components.pattern < 0.6, "{held:?}");
        assert_eq!(held.weakest, Component::Pattern);
        assert_ne!(held.verdict, Verdict::Matches);
        assert_eq!(
            held.differs().as_deref(),
            Some("the light falls on other parts of the target")
        );
        // As much is lit, as brightly, in the same colors.
        assert!(
            held.components.coverage > 0.8 && held.components.color > 0.9,
            "{held:?}"
        );
        // And both in the same one place is the same look.
        let (_, same) = bursts(middle, middle, LONG);
        assert_eq!(same.verdict, Verdict::Matches, "{same:?}");
    }

    #[test]
    fn a_few_bursts_say_little_and_the_look_says_so() {
        // Two seconds and one burst in each render, where and in which color chance has it:
        // PixelFlow's own renders have as little in common as any of them has with xLights'.
        let once = Bursts {
            every: 80,
            radius: 3.0,
            ..Bursts::USUAL
        };
        let (_, held) = bursts(once, once, 80);
        assert!(held.itself.is_some_and(|itself| itself < LITTLE), "{held:?}");
        assert!(held.caveat().is_some_and(|c| c.starts_with("says little")));
        // Nothing is held against it that chance alone would do.
        assert_eq!(held.verdict, Verdict::Matches, "{held:?}");
    }

    /// Four columns flashing for 8 frames in every 40, starting `late` frames into each: on the
    /// clock, in columns drawn by chance.
    fn flashes(seed: u32, late: u32, frame: u32, x: u32) -> [u8; 3] {
        let Some(at) = frame.checked_sub(late) else {
            return BLACK;
        };
        let column = (chance(seed, at / 40, 5) * 28.0) as u32;
        if at % 40 < 8 && (column..column + 4).contains(&x) {
            WHITE
        } else {
            BLACK
        }
    }

    #[test]
    fn an_effect_that_keeps_time_in_chance_places_is_still_held_to_its_time() {
        let on_time = held(32, 8, 800, Chance::Drawn, |render, frame, x, _| {
            flashes(render as u32, 0, frame, x)
        });
        // The flashes are in other columns in every render, PixelFlow's other seeds too: the
        // same flashes.
        assert_eq!(on_time.1.verdict, Verdict::Matches, "{:?}", on_time.1);
        assert!(on_time.1.components.pattern > 0.9, "{:?}", on_time.1);
        // Half a second late, the flashes come while xLights is dark. PixelFlow's own renders
        // all flash together, so this isn't chance.
        let late = held(32, 8, 800, Chance::Drawn, |render, frame, x, _| {
            flashes(render as u32, if render == THEIRS { 0 } else { 20 }, frame, x)
        });
        assert!(late.1.components.rhythm < 0.3, "{:?}", late.1);
        assert_ne!(late.1.verdict, Verdict::Matches, "{:?}", late.1);
        // Even were its moments taken as chance, it would be held to them: PixelFlow's own
        // renders have their flashes in common moment by moment, so chance doesn't move them,
        // and they are read moment by moment.
        let mistaken = held(32, 8, 800, Chance::Moments, |render, frame, x, _| {
            flashes(render as u32, if render == THEIRS { 0 } else { 20 }, frame, x)
        });
        assert!(mistaken.1.components.rhythm < 0.3, "{:?}", mistaken.1);
        assert_ne!(mistaken.1.verdict, Verdict::Matches, "{:?}", mistaken.1);
    }

    #[test]
    fn half_the_light_is_caught_however_little_chance_leaves_in_common() {
        // A ball wandering the matrix, somewhere else in every render: moment by moment two
        // renders have little in common, and one with a ball of half the size has half of
        // that. Next to how little PixelFlow's own renders share, that would pass.
        let ball = |seed: u32, radius: f32, frame: u32, x: u32, y: u32| {
            let at = |k: u32, size: f32| {
                let (from, to) = (chance(seed, frame / 40, k), chance(seed, frame / 40 + 1, k));
                (from + (to - from) * (frame % 40) as f32 / 40.0) * size
            };
            if (x as f32 - at(1, 32.0)).hypot(y as f32 - at(2, 16.0)) <= radius {
                WHITE
            } else {
                BLACK
            }
        };
        let (_, small) = held(32, 16, 800, Chance::Drawn, |render, frame, x, y| {
            let radius = if render == THEIRS { 5.0 } else { 3.5 };
            ball(render as u32, radius, frame, x, y)
        });
        // Half the area lit, and the look says so.
        assert!(small.components.coverage < 0.75, "{small:?}");
        assert!(small.components.brightness < 0.75, "{small:?}");
        assert_ne!(small.verdict, Verdict::Matches, "{small:?}");
        assert!(
            small
                .differs()
                .is_some_and(|d| d.starts_with("less of the target lit")),
            "{:?}",
            small.differs()
        );
        // The same ball in every render is the same look.
        let (_, same) = held(32, 16, 800, Chance::Drawn, |render, frame, x, y| {
            ball(render as u32, 5.0, frame, x, y)
        });
        assert_eq!(same.verdict, Verdict::Matches, "{same:?}");
    }

    #[test]
    fn as_much_of_something_is_judged_by_what_pixelflows_own_seeds_do() {
        // The seeds agree to within a few percent: half as much is half as much.
        assert!((as_much(&[0.20, 0.21, 0.19], 0.40, 0.0) - 0.5).abs() < 1e-9);
        assert!((as_much(&[0.20, 0.21, 0.19], 0.10, 0.0) - 0.5).abs() < 1e-9);
        // Inside what they do among themselves, nothing counts.
        assert_eq!(as_much(&[0.10, 0.30, 0.20], 0.33, 0.0), 1.0);
        assert_eq!(as_much(&[0.20, 0.21, 0.19], 0.205, 0.0), 1.0);
        // Just outside, a share of it does; well outside, all of it.
        let near = as_much(&[0.10, 0.30, 0.20], 0.55, 0.0);
        assert!(near < 1.0 && near > 0.2 / 0.55, "{near}");
        assert!((as_much(&[0.10, 0.30, 0.20], 0.80, 0.0) - 0.25).abs() < 1e-9);
        // A spread read from two renders is trusted less than one read from three, and one
        // from many is taken as it is.
        assert!(beyond(2) > beyond(3) && beyond(3) > beyond(6) && beyond(6) > beyond(40));
        assert!((beyond(1000) - BEYOND_CHANCE).abs() < 0.01);
        assert_eq!(as_much(&[0.10, 0.30], 0.9, 0.0), 1.0);
        // One seed, or seeds that agree to the last digit, have no spread: any difference counts.
        assert!((as_much(&[0.2], 0.4, 0.0) - 0.5).abs() < 1e-9);
        assert!((as_much(&[0.2, 0.2], 0.25, 0.0) - 0.8).abs() < 1e-9);
        // Next to nothing against next to nothing is the same.
        assert!(as_much(&[0.001, 0.001], 0.002, 0.01) > 0.9);
        assert_eq!(as_much(&[], 0.5, 0.0), 1.0);
    }

    #[test]
    fn where_pixelflows_seeds_agree_an_effect_is_judged_as_it_is_moment_by_moment() {
        // Nothing here is chance: bars at half the speed, the same from every seed.
        let (ours, theirs) = (bars(4, 0.25, WHITE), bars(4, 0.5, RED));
        let (by_moment, held) = held(32, 8, 400, Chance::Drawn, |render, frame, x, _| {
            if render == THEIRS {
                theirs(frame, x)
            } else {
                ours(frame, x)
            }
        });
        assert_eq!(held.components, by_moment.components);
        assert_eq!((held.score, held.verdict), (by_moment.score, by_moment.verdict));
        assert_eq!(held.itself, Some(1.0));
    }

    #[test]
    fn over_the_whole_effect_movement_is_which_way_and_how_fast() {
        let moving = |per_frame: f32, backwards: bool| {
            let draw = bars(4, per_frame, WHITE);
            move |frame: u32, x: u32| draw(frame, if backwards { 31 - x } else { x })
        };
        let motion = |ours: &dyn Fn(u32, u32) -> [u8; 3], theirs: &dyn Fn(u32, u32) -> [u8; 3]| {
            held(32, 8, 400, Chance::Moments, |render, frame, x, _| {
                if render == THEIRS {
                    theirs(frame, x)
                } else {
                    ours(frame, x)
                }
            })
            .1
        };
        let same = motion(&moving(0.5, false), &moving(0.5, false));
        assert!(same.components.motion > 0.95, "{same:?}");
        let opposite = motion(&moving(0.5, true), &moving(0.5, false));
        assert!(opposite.components.motion < 0.1, "{opposite:?}");
        assert!(opposite.seen.heading < -0.9, "{opposite:?}");
        assert_eq!(opposite.differs().as_deref(), Some("moves the opposite way"));
        let slow = motion(&moving(0.25, false), &moving(0.5, false));
        assert!((0.3..0.7).contains(&slow.components.motion), "{slow:?}");
        let still = motion(&moving(0.0, false), &moving(0.5, false));
        assert!(still.components.motion < 0.1, "{still:?}");
    }

    #[test]
    fn which_way_a_render_moves_is_judged_by_which_ways_pixelflows_seeds_move() {
        // Every seed moves right at the same speed: the other way is nothing alike, half the
        // speed a half, and the same is the same.
        let right = [[1.0, 0.0], [1.0, 0.0], [1.0, 0.0]];
        assert_eq!(same_way(&right, [1.0, 0.0], 1.0), 1.0);
        assert_eq!(same_way(&right, [-1.0, 0.0], 1.0), 0.0);
        assert!((same_way(&right, [0.5, 0.0], 1.0) - 0.5).abs() < 1e-9);
        assert!((same_way(&right, [0.0, 1.0], 1.0)).abs() < 1e-9);
        // Every seed drifts another way: so may xLights' render, whichever way that is.
        let every_way = [[1.0, 0.0], [-0.5, 0.8], [-0.4, -0.9]];
        for theirs in [[0.0, 1.0], [-1.0, 0.0], [0.7, -0.7], [0.0, 0.0]] {
            assert_eq!(same_way(&every_way, theirs, 1.0), 1.0, "{theirs:?}");
        }
        // But not three times as fast as any of them.
        assert!(same_way(&every_way, [9.0, 0.0], 9.0) < 0.5);
        assert_eq!(same_way(&[], [1.0, 0.0], 1.0), 1.0);
    }

    #[test]
    fn sorted_curves_say_how_much_of_the_time_and_never_when() {
        let rising: Vec<f64> = (0..100).map(|i| f64::from(i) / 100.0).collect();
        let falling: Vec<f64> = rising.iter().rev().copied().collect();
        // The same heights at other moments.
        assert!((sorted_overlap(rising.clone(), falling.clone(), 0.0) - 1.0).abs() < 1e-9);
        // Moment by moment the two have a third in common.
        let in_step = overlap(rising.iter().copied().zip(falling.iter().copied()), 0.0);
        assert!((0.3..0.4).contains(&in_step), "{in_step}");
        // Half as high throughout is half the same, however it's ordered.
        let half: Vec<f64> = falling.iter().map(|v| v / 2.0).collect();
        assert!((sorted_overlap(rising.clone(), half, 0.0) - 0.5).abs() < 1e-9);
        // Lit a tenth of the time against lit all the time at a tenth of the height: as much
        // light, and next to nothing the same.
        let bursts: Vec<f64> = (0..100).map(|i| if i % 10 == 0 { 1.0 } else { 0.0 }).collect();
        let steady = vec![0.1; 100];
        assert!(sorted_overlap(bursts.clone(), steady, 0.0) < 0.06);
        // A curve of another length is read at the same shares of the way along.
        let twice: Vec<f64> = bursts.iter().flat_map(|&v| [v, v]).collect();
        assert!((sorted_overlap(bursts, twice, 0.0) - 1.0).abs() < 1e-9);
        assert_eq!(sorted_overlap(Vec::new(), rising, 0.0), 1.0);
    }

    #[test]
    fn a_look_next_to_other_seeds_is_the_one_over_the_other_part_by_part() {
        let draw = bars(4, 0.5, RED);
        let by_moment = look(32, 8, 80, |_, frame, x, _| draw(frame, x));
        let parts = |same: f64| Parts {
            same: [same; 6],
            layout: same,
            heading: same,
            whole: None,
            lit: [0.5; 2],
            level: [0.5; 2],
            flashes: [1.0; 2],
            change: [0.1; 2],
            speed: [0.0; 2],
            net: [[0.0; 2]; 2],
        };
        // Half as alike to xLights as PixelFlow's own renders are to one another.
        let half = next_to_seeds(&by_moment, Chance::Moments, &[parts(0.4)], &[parts(0.8)]);
        for component in Component::ALL {
            assert!((half.components.get(component) - 0.5).abs() < 1e-3, "{half:?}");
        }
        assert_eq!((half.itself, half.chance), (Some(0.8), Chance::Moments));
        // As alike as its own renders are, or more: the same.
        let same = next_to_seeds(
            &by_moment,
            Chance::Moments,
            &[parts(0.5), parts(0.7)],
            &[parts(0.6)],
        );
        assert_eq!((same.score, same.verdict), (100.0, Verdict::Matches));
        // Its renders to xLights' on average, next to the least alike two of its own.
        let least = next_to_seeds(
            &by_moment,
            Chance::Drawn,
            &[parts(0.2), parts(0.4)],
            &[parts(0.9), parts(0.6), parts(0.8)],
        );
        assert!((least.components.color - felt(0.5)).abs() < 1e-3, "{least:?}");
        assert_eq!(least.itself, Some(0.6));
        // With no other seed to hold it against, it is judged as it is.
        let alone = next_to_seeds(&by_moment, Chance::Drawn, &[parts(0.5)], &[]);
        assert!((alone.components.coverage - 0.5).abs() < 1e-3, "{alone:?}");
        assert_eq!(alone.seen.lit, by_moment.seen.lit);
    }
}
