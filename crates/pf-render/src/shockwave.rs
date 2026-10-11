//! xLights' Shockwave (`ShockwaveEffect::Render`): one ring growing (or shrinking) from a center
//! point over each cycle, its width changing as it goes, its color running through the palette.
//!
//! Everything is counted in cells of the target's grid, as xLights counts it: the center is a
//! whole cell, and a cell is lit when its distance from the center falls within the ring.

use crate::color::{Colors, Rgba};
use crate::effects::{Canvas, EffectTime, Shade, accelerate};
use crate::geometry::Pixel;
use crate::raster::{cell_of, grid_size};
use pf_sequence::ShockwaveParams;

pub struct Shockwave {
    columns: i32,
    rows: i32,
    center: (i32, i32),
    color: [f32; 3],
    /// The ring's middle, and the cells either side of it that it covers.
    middle: f64,
    half_width: f64,
    blend_edges: bool,
}

impl Shockwave {
    pub fn new(p: &ShockwaveParams, time: &EffectTime, colors: Colors, canvas: Canvas) -> Self {
        let (columns, rows) = grid_size(canvas);
        let at = time.cycle_position(f64::from(p.cycles.max(1)));
        let at = accelerate(at, f64::from(p.acceleration.trunc()));
        // xLights reads every size as a whole number.
        let whole = |v: f32| f64::from(v.trunc());
        let (mut from, mut to) = (whole(p.start_radius), whole(p.end_radius));
        let (mut wide_from, mut wide_to) = (whole(p.start_width), whole(p.end_width));
        if p.scale {
            // Percent of the grid's longer side: the radii of half of it, the widths (kept
            // whole) of all of it.
            let longer = f64::from(columns.max(rows));
            from *= longer / 200.0;
            to *= longer / 200.0;
            wide_from = (wide_from * (longer / 100.0)).trunc();
            wide_to = (wide_to * (longer / 100.0)).trunc();
        }
        Self {
            columns,
            rows,
            center: (
                p.center_x.trunc() as i32 * columns / 100,
                p.center_y.trunc() as i32 * rows / 100,
            ),
            color: colors.ramp(at as f32),
            middle: from + (to - from) * at,
            half_width: ((wide_from + (wide_to - wide_from) * at) / 2.0).max(0.25),
            blend_edges: p.blend_edges,
        }
    }
}

impl Shade for Shockwave {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let (x, y) = cell_of(px, self.columns, self.rows);
        let r = f64::from(x - self.center.0).hypot(f64::from(y - self.center.1));
        let off = (r - self.middle).abs();
        if off > self.half_width {
            return Rgba::CLEAR;
        }
        if self.blend_edges {
            // Fading to nothing at both edges, in 8-bit steps.
            let cover = (255.0 * (1.0 - off / self.half_width)).trunc() / 255.0;
            Rgba::with_alpha(self.color, cover as f32)
        } else {
            Rgba::opaque(self.color)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_sequence::Rgb;

    const GRID: Canvas = Canvas {
        columns: 41,
        rows: 21,
    };

    /// Frame `frame` of a 21-frame effect (so frame 10 is half way, and frame 20 the end).
    fn at(frame: u64) -> EffectTime {
        EffectTime::within(0, 21 * 50, frame * 50).with_frame_ms(50)
    }

    fn cell(x: i32, y: i32) -> Pixel {
        Pixel {
            u: x as f32 / 40.0,
            v: y as f32 / 20.0,
            index: 0,
            count: 1,
        }
    }

    /// A ring from radius 0 to 20 cells, 4 cells wide throughout, with hard edges.
    fn ring() -> ShockwaveParams {
        ShockwaveParams {
            start_radius: 0.0,
            end_radius: 20.0,
            start_width: 4.0,
            end_width: 4.0,
            blend_edges: false,
            scale: false,
            ..ShockwaveParams::default()
        }
    }

    fn wave(p: &ShockwaveParams, frame: u64, palette: &[Rgb]) -> Shockwave {
        Shockwave::new(p, &at(frame), Colors::new(palette), GRID)
    }

    /// The cells lit along the row through the center, as distances right of it.
    fn lit_right(w: &Shockwave) -> Vec<i32> {
        (0..=20).filter(|&d| w.shade(&cell(20 + d, 10)).a > 0.0).collect()
    }

    #[test]
    fn the_ring_grows_from_the_start_radius_to_the_end_radius() {
        let p = ring();
        // The center is cell (50% of 41, 50% of 21) in whole cells.
        assert_eq!(wave(&p, 0, &[]).center, (20, 10));
        // Middle 0, 10, 20; two cells either side, and nothing inside the center.
        assert_eq!(lit_right(&wave(&p, 0, &[])), [0, 1, 2]);
        assert_eq!(lit_right(&wave(&p, 10, &[])), [8, 9, 10, 11, 12]);
        assert_eq!(lit_right(&wave(&p, 20, &[])), [18, 19, 20]);
        // A ring: the same distance up as across.
        let half = wave(&p, 10, &[]);
        assert!(half.shade(&cell(20, 0)).a > 0.0 && half.shade(&cell(20, 5)).a == 0.0);
        assert!(half.shade(&cell(26, 2)).a > 0.0, "6 across and 8 down is 10 away");
    }

    #[test]
    fn the_width_changes_over_the_effect_and_is_never_thinner_than_half_a_cell() {
        let p = ShockwaveParams {
            start_width: 0.0,
            end_width: 8.0,
            ..ring()
        };
        // Half a cell wide at least: only the cell exactly on the ring.
        assert_eq!(lit_right(&wave(&p, 0, &[])), [0]);
        assert_eq!(
            lit_right(&wave(&p, 10, &[])),
            [8, 9, 10, 11, 12],
            "4 wide half way"
        );
        assert_eq!(lit_right(&wave(&p, 20, &[])), [16, 17, 18, 19, 20]);
    }

    #[test]
    fn blended_edges_fade_from_the_middle_of_the_ring() {
        let p = ShockwaveParams {
            blend_edges: true,
            ..ring()
        };
        let w = wave(&p, 10, &[Rgb::RED]);
        let cover = |d: i32| w.shade(&cell(20 + d, 10)).a;
        assert_eq!(cover(10), 1.0);
        assert_eq!(cover(9), 127.0 / 255.0, "half way to the edge, in 8-bit steps");
        assert_eq!(cover(11), 127.0 / 255.0);
        assert_eq!(cover(8), 0.0, "nothing left at the edge itself");
        let c = w.shade(&cell(29, 10));
        assert_eq!([c.r, c.g, c.b], [1.0, 0.0, 0.0], "the color isn't dimmed");
    }

    #[test]
    fn the_color_runs_through_the_palette_over_each_cycle() {
        let palette = [Rgb::RED, Rgb::GREEN, Rgb::BLUE];
        let color = |p: &ShockwaveParams, frame| wave(p, frame, &palette).color;
        let p = ring();
        assert_eq!(color(&p, 0), [1.0, 0.0, 0.0]);
        assert_eq!(color(&p, 5), [0.5, 0.5, 0.0]);
        assert_eq!(color(&p, 10), [0.0, 1.0, 0.0]);
        assert_eq!(color(&p, 20), [0.0, 0.0, 1.0]);
        // Three cycles of 7 frames: each starts over, ring and color.
        let cycles = ShockwaveParams { cycles: 3, ..ring() };
        assert_eq!(color(&cycles, 7), [1.0, 0.0, 0.0]);
        assert_eq!(color(&cycles, 6), [0.0, 0.0, 1.0]);
        assert_eq!(lit_right(&wave(&cycles, 14, &palette)), [0, 1, 2]);
        assert_eq!(lit_right(&wave(&cycles, 13, &palette)), [18, 19, 20]);
    }

    #[test]
    fn acceleration_bends_the_rings_progress() {
        let fast = ShockwaveParams {
            acceleration: 10.0,
            ..ring()
        };
        let slow = ShockwaveParams {
            acceleration: -10.0,
            ..ring()
        };
        // Half way through, a ring speeding up has covered less ground and one slowing down more.
        let middle = |p: &ShockwaveParams| wave(p, 10, &[]).middle;
        assert!(middle(&fast) < 5.0 && middle(&slow) > 15.0);
        assert_eq!(middle(&ring()), 10.0);
        // Both arrive at the end.
        assert_eq!(wave(&fast, 20, &[]).middle, 20.0);
        assert_eq!(wave(&slow, 20, &[]).middle, 20.0);
    }

    #[test]
    fn scaled_sizes_are_percent_of_the_grids_longer_side() {
        let p = ShockwaveParams {
            start_radius: 0.0,
            end_radius: 100.0,
            start_width: 10.0,
            end_width: 10.0,
            scale: true,
            ..ring()
        };
        // 41 cells: a radius of 100% is 20.5 cells, and a width of 10% is 4 whole cells.
        let end = wave(&p, 20, &[]);
        assert_eq!(end.middle, 20.5);
        assert_eq!(end.half_width, 2.0);
    }

    #[test]
    fn the_center_moves_in_whole_cells_and_sizes_are_read_as_whole_numbers() {
        let p = ShockwaveParams {
            center_x: 25.9,
            center_y: 80.0,
            start_radius: 3.9,
            end_radius: 3.9,
            ..ring()
        };
        let w = wave(&p, 0, &[]);
        // 25% of 41 and 80% of 21, rounded down.
        assert_eq!(w.center, (10, 16));
        assert_eq!(w.middle, 3.0);
    }
}
