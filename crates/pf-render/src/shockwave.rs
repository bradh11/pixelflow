//! The Shockwave effect: a ring that grows (or closes in) about one cell of the target's grid,
//! its width and its color changing as it goes.
//!
//! The picture is made of whole cells. A pixel shows what its nearest cell shows, the center is a
//! cell, and so the only thing a pixel has to know about itself is a whole number: how far its
//! cell is from the center cell, squared (so many columns across, squared, and so many rows up,
//! squared, added together). Everything else about a frame is the same for every pixel and is
//! worked out once, in [`Shockwave::new`].
//!
//! 1. **Where the ring is.** How far the ring is through its travel comes from the frame it is
//!    on ([`EffectTime::cycle_position`], which also shares the effect out among several rings)
//!    bent by the acceleration ([`accelerate`]). The radius, the width and the color are each
//!    that far from their start to their end. A band is never thinner than half a cell.
//! 2. **Which cells it covers.** A cell is on the band when its distance from the center cell
//!    is no further from the radius than half the width, the edges themselves included. Cells
//!    further out are further away, so the band's cells are exactly those whose squared
//!    distance lies between two whole numbers ([`Band`]). Those two are found once a frame, by
//!    asking that same question of the squared distances either side of where each edge falls;
//!    a pixel then answers with a subtraction and a comparison, and no square root. Most pixels
//!    of most frames are off the band and are done there.
//! 3. **How strongly.** With hard edges every cell of the band is at full strength. With soft
//!    edges a cell is at full strength where its distance is the radius and falls in a straight
//!    line to nothing at either edge, in whole 255ths rounded down; only the band's own pixels
//!    pay for the square root that needs.
//!
//! Sizes are numbers of cells: the settings' own numbers, or with "sizes in percent" a share of
//! the grid's longer side (a radius of 100 is half of it, a width of 100 all of it, and a width
//! is brought down to a whole number of cells). The settings are read as whole numbers.
//!
//! A frame depends on nothing but the settings, the palette, the target's size, and which frame
//! of the effect it is. Nothing is remembered and nothing is random.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Shade, accelerate};
use crate::geometry::Pixel;
use crate::raster::{cell_of, grid_size};
use pf_sequence::ShockwaveParams;

/// The thinnest a band gets, in cells: a quarter of a cell either side of its radius.
const THINNEST: f64 = 0.5;

/// Alpha comes in this many equal steps from nothing to full strength.
const STEPS: f64 = 255.0;

/// How many squared distances either side of where an edge is reckoned to fall are asked about.
/// Squared distances are whole numbers and the reckoning is out by less than one, so the edge is
/// always found within two; one more costs nothing.
const TRIES: usize = 3;

pub struct Shockwave {
    /// The grid the picture is on.
    columns: i32,
    rows: i32,
    /// The center cell: its column and its row, counted from the left and from the bottom. It
    /// can be one cell beyond the grid's last.
    center: [i64; 2],
    /// The cells the band covers on this frame.
    band: Band,
    /// The radius, and half the width, in cells.
    radius: f64,
    half: f64,
    /// Whether the band fades toward its edges.
    soft: bool,
    /// The ring's color on this frame, at full strength.
    lit: Rgba,
}

impl Shockwave {
    /// Everything that is the same for every pixel of one frame.
    pub fn new(p: &ShockwaveParams, time: &EffectTime, colors: Colors, canvas: Canvas) -> Self {
        let (columns, rows) = grid_size(canvas);
        // Every number is read as a whole number: its fraction is dropped.
        let whole = |setting: f32| f64::from(setting.trunc());

        // How far through its travel the ring is: its radius, its width and its color alike.
        let rings = f64::from(p.cycles.max(1));
        let progress = accelerate(time.cycle_position(rings), whole(p.acceleration));
        let between = |start: f64, end: f64| start + (end - start) * progress;

        // Sizes in cells. In percent, a radius is a share of half the grid's longer side and
        // keeps its fraction; a width is a share of all of it, in whole cells.
        let one_percent = f64::from(columns.max(rows)) / 100.0;
        let radius = |setting: f32| match p.scale {
            true => one_percent * whole(setting) / 2.0,
            false => whole(setting),
        };
        let width = |setting: f32| match p.scale {
            true => (one_percent * whole(setting)).floor(),
            false => whole(setting),
        };
        let at = between(radius(p.start_radius), radius(p.end_radius));
        let half = between(width(p.start_width), width(p.end_width)).max(THINNEST) / 2.0;

        // The center cell: the setting's percent of the number of columns (or rows), rounded
        // down. At 100 that is one cell past the last.
        let place = |setting: f32, cells: i32| (i64::from(cells) * setting.trunc() as i64).div_euclid(100);
        let center = [place(p.center_x, columns), place(p.center_y, rows)];

        // No cell of the grid is further from the center than its furthest corner.
        let furthest =
            |center: i64, cells: i32| center.abs().max((i64::from(cells) - 1 - center).abs()) as u64;
        let corner = furthest(center[0], columns).pow(2) + furthest(center[1], rows).pow(2);

        Self {
            columns,
            rows,
            center,
            band: Band::covering(at, half, corner),
            radius: at,
            half,
            soft: p.blend_edges,
            lit: Rgba::opaque(colors.ramp(progress as f32)),
        }
    }
}

/// How far outside a ring of `radius` cells a cell is, whose squared distance from the center
/// cell is `apart`; negative inside it.
#[inline]
fn outside(apart: u64, radius: f64) -> f64 {
    (apart as f64).sqrt() - radius
}

/// The cells of a band, by their squared distance from the center cell: `first` is the least of
/// them and `span` how much more the greatest is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Band {
    first: u64,
    span: u64,
}

impl Band {
    /// A band no cell is on.
    const NONE: Band = Band {
        first: u64::MAX,
        span: 0,
    };

    /// The band of the cells no further from `radius` than `half`, on a grid none of whose
    /// cells has a squared distance above `corner`.
    ///
    /// A cell is on it exactly when `outside(apart, radius).abs() <= half`. A cell further from
    /// the center is never less far outside, so the cells not short of the inner edge are all
    /// those from some squared distance up, and the cells not beyond the outer edge all those
    /// up to some squared distance. Each of the two is reckoned from the edge's own distance
    /// and then settled by asking the question itself of its neighbors.
    fn covering(radius: f64, half: f64, corner: u64) -> Self {
        let reaches = |apart: u64| outside(apart, radius) >= -half;
        let stays = |apart: u64| outside(apart, radius) <= half;
        // One past the corner stands for every squared distance beyond it.
        let beyond = corner.saturating_add(1);
        let squared = |edge: f64| edge.max(0.0) * edge.max(0.0);

        let mut first = (squared(radius - half).ceil() as u64).min(beyond);
        for _ in 0..TRIES {
            if first > 0 && reaches(first - 1) {
                first -= 1;
            }
        }
        for _ in 0..TRIES {
            if !reaches(first) {
                first += 1;
            }
        }

        let mut last = (squared(radius + half).floor() as u64).min(beyond);
        for _ in 0..TRIES {
            if stays(last + 1) {
                last += 1;
            }
        }
        for _ in 0..TRIES {
            if last > 0 && !stays(last) {
                last -= 1;
            }
        }

        if reaches(first) && stays(last) && first <= last {
            Self {
                first,
                span: last - first,
            }
        } else {
            Self::NONE
        }
    }

    /// Whether a cell with this squared distance is on the band.
    #[inline]
    fn covers(&self, apart: u64) -> bool {
        apart.wrapping_sub(self.first) <= self.span
    }
}

impl Shade for Shockwave {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let (column, row) = cell_of(px, self.columns, self.rows);
        let across = i64::from(column) - self.center[0];
        let up = i64::from(row) - self.center[1];
        let apart = (across * across + up * up) as u64;
        if !self.band.covers(apart) {
            return Rgba::CLEAR;
        }
        if !self.soft {
            return self.lit;
        }
        // Full strength at the radius, nothing at either edge, in whole steps rounded down.
        let strength = 1.0 - outside(apart, self.radius).abs() / self.half;
        Rgba {
            a: (strength * STEPS) as u8 as f32 / STEPS as f32,
            ..self.lit
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The question a band's two whole numbers stand in for, asked of one cell.
    fn on_the_band(apart: u64, radius: f64, half: f64) -> bool {
        outside(apart, radius).abs() <= half
    }

    /// The band must cover exactly the squared distances the question itself picks, up to the
    /// grid's corner. `tried` counts the bands and how many of them cover nothing.
    fn agree(
        radius: f64,
        half: f64,
        corner: u64,
        squares: impl Iterator<Item = u64>,
        tried: &mut [usize; 2],
    ) {
        let band = Band::covering(radius, half, corner);
        tried[0] += 1;
        tried[1] += usize::from(band == Band::NONE);
        for apart in squares.filter(|apart| *apart <= corner) {
            assert_eq!(
                band.covers(apart),
                on_the_band(apart, radius, half),
                "radius {radius}, half {half}, corner {corner}, squared distance {apart}: {band:?}"
            );
        }
    }

    #[test]
    fn a_bands_two_whole_numbers_pick_the_cells_the_comparison_picks() {
        let mut tried = [0; 2];
        // Half widths from the thinnest band up, and radii a 64th of a cell apart, so that edges
        // fall on whole cells, between them, and on distances that are no cell's.
        let halves = [0.25, 0.3, 0.5, 0.75, 1.0, 1.25, 2.5, 3.0, 7.3, 20.0, 127.5];
        for half in halves {
            for step in 0..=64 * 30 {
                let radius = f64::from(step) / 64.0;
                agree(radius, half, 2_500, 0..=2_500, &mut tried);
            }
        }
        // Radii that are not round numbers in binary: thirds, sevenths and a size in percent.
        for half in halves {
            for step in 0..160 {
                for unit in [1.0 / 3.0, 1.0 / 7.0, 0.505, 0.58 * 0.5] {
                    agree(f64::from(step) * unit, half, 2_500, 0..=2_500, &mut tried);
                }
            }
        }
        // A grid whose corner is nearer than the band, on it, or just beyond it.
        for corner in [0, 1, 2, 80, 99, 100, 101, 143, 144, 145, 200] {
            for half in [0.25, 1.0, 2.0] {
                for radius in [0.0, 0.5, 1.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 750.0] {
                    agree(radius, half, corner, 0..=corner, &mut tried);
                }
            }
        }
        // The largest sizes there are: a radius of 750 with a width of 255, in cells, and the
        // same in percent of the longest grid a buffer can have (some 67 million cells).
        let about = |edge: f64| {
            let squared = (edge * edge) as u64;
            squared.saturating_sub(4)..=squared + 4
        };
        let longest = f64::from(1u32 << 26);
        for (radius, half) in [
            (750.0, 127.5),
            (749.3, 0.25),
            (longest * 3.75, longest * 1.275),
            (longest * 0.9, 0.25),
            (longest * 0.5 + 0.3, 31.5),
            (longest - 0.25, 0.25),
        ] {
            let corner = 1 << 53;
            let squares = about(radius - half)
                .chain(about(radius + half))
                .chain(about(radius));
            agree(radius, half, corner, squares.chain(0..=4), &mut tried);
        }
        assert!(
            tried[0] > 25_000 && tried[1] > 100 && tried[1] < tried[0] / 2,
            "{tried:?}"
        );
    }

    #[test]
    fn a_band_that_is_nowhere_covers_nothing() {
        for (radius, half) in [
            (f64::NAN, 1.0),
            (1.0, f64::NAN),
            (f64::INFINITY, 1.0),
            (f64::NEG_INFINITY, 1.0),
            (-5.0, 1.0),
            (1e300, 1.0),
        ] {
            let band = Band::covering(radius, half, 10_000);
            assert!((0..=10_000).all(|apart| !band.covers(apart)), "{radius}, {half}");
        }
        // No distance is as far as its own width from every radius, so an endless band covers
        // every cell.
        let everywhere = Band::covering(3.0, f64::INFINITY, 10_000);
        assert!((0..=10_000).all(|apart| everywhere.covers(apart)));
    }
}
