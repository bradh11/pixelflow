//! The dancers' paint: filled shapes with soft edges, drawn on a [`Raster`] one over another.
//!
//! Shapes are given in cells from the dancer's own origin: across from its center line (a
//! column's middle) and up from the ground (the bottom edge of a row). Each shape is its distance
//! to its edge, and a cell is covered by how far inside its middle is: fully half a cell in,
//! not at all half a cell out. So a shape whose edges fall between cells is drawn exactly, and
//! any other is softened by a cell (less on a small figure, where a slanted line should stay a
//! line), at any size.

use crate::color::{Rgba, unit};
use crate::raster::Raster;

/// A point: cells across from the center line, and up from the ground.
pub(crate) type P = [f32; 2];
pub(crate) type Color = [f32; 3];

pub(crate) const BLACK: Color = [0.0; 3];

/// The longest line of whole cells drawn, in cells.
const MAX_LINE: i32 = 1 << 14;

pub(crate) struct Paint<'a> {
    pub raster: &'a mut Raster,
    /// The dancer's center line and ground, in the raster's cells.
    pub center: f32,
    pub ground: f32,
    /// The columns the dancer may draw on (the last one not included).
    pub columns: (i32, i32),
    /// How hard edges are: 1 softens them over a whole cell, more over less.
    pub sharp: f32,
}

/// What a shape does to the cells it covers.
#[derive(Clone, Copy)]
enum Ink {
    Color(Color),
    /// Takes away what's drawn there, leaving the cell clear.
    Clear,
}

impl Paint<'_> {
    /// Inks the cells of the box `lo`–`hi` (and one around it) by `distance`: how far each
    /// cell's middle is outside the shape (negative inside), in cells.
    fn shape(&mut self, lo: P, hi: P, ink: Ink, distance: impl Fn(f32, f32) -> f32) {
        let x0 = ((self.center + lo[0] - 1.0).floor() as i32).max(self.columns.0);
        let x1 = ((self.center + hi[0] + 1.0).ceil() as i32).min(self.columns.1 - 1);
        let y0 = ((self.ground + lo[1] - 1.0).floor() as i32).max(0);
        let y1 = ((self.ground + hi[1] + 1.0).ceil() as i32).min(self.raster.height - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (lx, ly) = (x as f32 + 0.5 - self.center, y as f32 + 0.5 - self.ground);
                let cover = unit(0.5 - distance(lx, ly) * self.sharp);
                if cover <= 0.0 {
                    continue;
                }
                match ink {
                    Ink::Color(color) => self.raster.cover(x, y, Rgba::with_alpha(color, cover)),
                    Ink::Clear => {
                        let was = self.raster.get(x, y);
                        self.raster.set(
                            x,
                            y,
                            Rgba {
                                a: was.a * (1.0 - cover),
                                ..was
                            },
                        );
                    }
                }
            }
        }
    }

    /// Covers cells by `distance` (see [`Paint::shape`]), for shapes the others don't make.
    pub fn fill(&mut self, lo: P, hi: P, color: Color, distance: impl Fn(f32, f32) -> f32) {
        self.shape(lo, hi, Ink::Color(color), distance);
    }

    pub fn disc(&mut self, c: P, r: f32, color: Color) {
        self.ellipse(c, [r, r], color);
    }

    pub fn ellipse(&mut self, c: P, r: P, color: Color) {
        let (rx, ry) = (r[0].max(0.01), r[1].max(0.01));
        self.fill([c[0] - rx, c[1] - ry], [c[0] + rx, c[1] + ry], color, |x, y| {
            let (dx, dy) = (x - c[0], y - c[1]);
            if rx == ry {
                return (dx * dx + dy * dy).sqrt() - rx;
            }
            // Close to the true distance near the edge, which is all the soft edge needs.
            let k0 = ((dx / rx).powi(2) + (dy / ry).powi(2)).sqrt();
            let k1 = ((dx / (rx * rx)).powi(2) + (dy / (ry * ry)).powi(2)).sqrt();
            if k1 > 0.0 {
                k0 * (k0 - 1.0) / k1
            } else {
                -rx.min(ry)
            }
        });
    }

    /// A line one cell wide between two cells, as a row of whole cells: no soft edges, so a
    /// small figure's limbs stay clean lines.
    fn cells(&mut self, a: P, b: P, color: Color, keep: impl Fn(f32) -> bool) {
        let cell = |p: P| {
            (
                (self.center + p[0]).floor() as i32,
                (self.ground + p[1]).floor() as i32,
            )
        };
        let ((x0, y0), (x1, y1)) = (cell(a), cell(b));
        let (dx, dy) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
        let steps = dx.saturating_abs().max(dy.saturating_abs());
        // No figure is this many cells across: the line has gone astray.
        if steps > MAX_LINE {
            return;
        }
        for i in 0..=steps {
            let t = if steps == 0 { 0.0 } else { i as f32 / steps as f32 };
            let x = x0 + (dx as f32 * t).round() as i32;
            let y = y0 + (dy as f32 * t).round() as i32;
            let inside = (self.columns.0..self.columns.1).contains(&x);
            if inside && keep(y as f32 + 0.5 - self.ground) {
                self.raster.set(x, y, Rgba::opaque(color));
            }
        }
    }

    fn stroke(&mut self, a: P, b: P, r: f32, ink: Ink, keep: impl Fn(f32) -> bool) {
        if let Ink::Color(color) = ink
            && self.sharp > 1.0
            && r <= 0.5
        {
            return self.cells(a, b, color, keep);
        }
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx * dx + dy * dy;
        self.shape(
            [a[0].min(b[0]) - r, a[1].min(b[1]) - r],
            [a[0].max(b[0]) + r, a[1].max(b[1]) + r],
            ink,
            |x, y| {
                if !keep(y) {
                    return 1.0;
                }
                let t = if len > 0.0 {
                    unit(((x - a[0]) * dx + (y - a[1]) * dy) / len)
                } else {
                    0.0
                };
                let (ex, ey) = (a[0] + t * dx - x, a[1] + t * dy - y);
                (ex * ex + ey * ey).sqrt() - r
            },
        );
    }

    /// A line from `a` to `b`, `r` thick on each side, with round ends.
    pub fn line(&mut self, a: P, b: P, r: f32, color: Color) {
        self.stroke(a, b, r, Ink::Color(color), |_| true);
    }

    /// A line drawn only on the rows `keep` says (by a cell's height), for stripes.
    pub fn line_where(&mut self, a: P, b: P, r: f32, color: Color, keep: impl Fn(f32) -> bool) {
        self.stroke(a, b, r, Ink::Color(color), keep);
    }

    /// Clears a line through what's drawn, so what's drawn next stands apart from it.
    pub fn cut(&mut self, a: P, b: P, r: f32) {
        self.stroke(a, b, r, Ink::Clear, |_| true);
    }

    /// The box from `lo` to `hi`.
    pub fn rect(&mut self, lo: P, hi: P, color: Color) {
        let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
        let half = [(hi[0] - lo[0]) / 2.0, (hi[1] - lo[1]) / 2.0];
        if half[0] <= 0.0 || half[1] <= 0.0 {
            return;
        }
        self.fill(lo, hi, color, |x, y| {
            let (qx, qy) = ((x - c[0]).abs() - half[0], (y - c[1]).abs() - half[1]);
            qx.max(qy)
        });
    }

    /// A shape with no dents (a triangle, a trapezoid), its corners in order either way round.
    pub fn convex(&mut self, corners: &[P], color: Color) {
        let n = corners.len();
        if n < 3 {
            return;
        }
        let mut area = 0.0;
        let (mut lo, mut hi) = (corners[0], corners[0]);
        for (i, p) in corners.iter().enumerate() {
            let q = corners[(i + 1) % n];
            area += p[0] * q[1] - q[0] * p[1];
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        }
        if area == 0.0 || !area.is_finite() {
            return;
        }
        // Outward of each edge is to its right going round one way, its left the other.
        let turn = if area > 0.0 { 1.0 } else { -1.0 };
        self.fill(lo, hi, color, |x, y| {
            let mut out = f32::MIN;
            for (i, p) in corners.iter().enumerate() {
                let q = corners[(i + 1) % n];
                let (ex, ey) = (q[0] - p[0], q[1] - p[1]);
                let len = (ex * ex + ey * ey).sqrt();
                if len > 0.0 {
                    out = out.max(turn * ((x - p[0]) * ey - (y - p[1]) * ex) / len);
                }
            }
            out
        });
    }

    /// A soft glow behind what's drawn, around `c`: `level` at its middle, fading to nothing
    /// `r` out.
    pub fn glow_behind(&mut self, c: P, r: P, color: Color, level: f32) {
        let (rx, ry) = (r[0].max(0.5), r[1].max(0.5));
        let x0 = ((self.center + c[0] - rx).floor() as i32).max(self.columns.0);
        let x1 = ((self.center + c[0] + rx).ceil() as i32).min(self.columns.1 - 1);
        let y0 = ((self.ground + c[1] - ry).floor() as i32).max(0);
        let y1 = ((self.ground + c[1] + ry).ceil() as i32).min(self.raster.height - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = (x as f32 + 0.5 - self.center - c[0]) / rx;
                let dy = (y as f32 + 0.5 - self.ground - c[1]) / ry;
                let fade = unit(1.0 - (dx * dx + dy * dy));
                let glow = level * fade * fade;
                let top = self.raster.get(x, y);
                let under = glow * (1.0 - top.a);
                let a = top.a + under;
                if under <= 0.0 || a <= 0.0 {
                    continue;
                }
                let mix = |over: f32, below: f32| (over * top.a + below * under) / a;
                self.raster.set(
                    x,
                    y,
                    Rgba::new(
                        mix(top.r, color[0]),
                        mix(top.g, color[1]),
                        mix(top.b, color[2]),
                        a,
                    ),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::Canvas;

    fn raster(columns: u32, rows: u32) -> Raster {
        Raster::new(Canvas { columns, rows })
    }

    /// Paint for a dancer in the middle of a nine-column raster, standing on its bottom edge.
    fn paint(raster: &mut Raster, sharp: f32) -> Paint<'_> {
        let columns = (0, raster.width);
        Paint {
            raster,
            center: 4.5,
            ground: 0.0,
            columns,
            sharp,
        }
    }

    fn row(raster: &Raster, y: i32) -> Vec<f32> {
        (0..raster.width).map(|x| raster.get(x, y).a).collect()
    }

    #[test]
    fn shapes_on_the_cells_are_drawn_exactly() {
        let mut r = raster(9, 9);
        let mut p = paint(&mut r, 1.0);
        // A one-cell line up the center column, and a box three cells wide.
        p.line([0.0, 0.5], [0.0, 3.5], 0.5, [1.0; 3]);
        p.rect([-1.5, 5.0], [1.5, 7.0], [1.0; 3]);
        assert_eq!(row(&r, 2), [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(row(&r, 5), [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]);
        assert_eq!(row(&r, 7), [0.0; 9], "the box ends where it says");
    }

    #[test]
    fn edges_between_cells_are_softened_and_later_paint_covers() {
        let mut r = raster(9, 9);
        paint(&mut r, 1.0).disc([0.0, 4.5], 2.5, [1.0; 3]);
        let across = row(&r, 4);
        assert_eq!(across[2..7], [1.0; 5], "five cells across its middle");
        assert_eq!((across[1], across[7]), (0.0, 0.0));
        let corner = r.get(2, 2).a;
        assert!(corner > 0.0 && corner < 1.0, "{corner}");
        // Harder edges soften less.
        let mut hard = raster(9, 9);
        paint(&mut hard, 2.0).disc([0.0, 4.5], 2.5, [1.0; 3]);
        assert!(hard.get(2, 2).a < corner);
        assert_eq!(row(&hard, 4), across);
        // An eye: black over the white.
        paint(&mut r, 1.0).rect([0.5, 4.0], [1.5, 5.0], BLACK);
        let eye = r.get(5, 4);
        assert_eq!((eye.r, eye.a), (0.0, 1.0));
        // A triangle, its corners either way round.
        for corners in [
            [[-3.0, 0.0], [3.0, 0.0], [0.0, 3.0]],
            [[0.0, 3.0], [3.0, 0.0], [-3.0, 0.0]],
        ] {
            let mut r = raster(9, 9);
            paint(&mut r, 1.0).convex(&corners, [1.0; 3]);
            assert!(r.get(4, 0).a > 0.9 && r.get(4, 1).a > 0.9);
            assert_eq!(r.get(0, 2).a, 0.0);
            assert!(r.get(1, 0).a > r.get(1, 1).a);
        }
    }

    #[test]
    fn a_small_figures_thin_lines_are_whole_cells() {
        let mut r = raster(9, 9);
        paint(&mut r, 2.0).line([-2.0, 0.5], [1.0, 6.5], 0.5, [1.0; 3]);
        for y in 0..9 {
            let lit: Vec<f32> = row(&r, y).into_iter().filter(|a| *a > 0.0).collect();
            assert_eq!(lit, if y < 7 { vec![1.0] } else { vec![] }, "row {y}");
        }
        // Off the raster and back: nothing but the cells on it, and no long walk for a line
        // that has gone astray.
        let mut r = raster(9, 9);
        let mut p = paint(&mut r, 2.0);
        p.line([-20.0, 4.5], [20.0, 4.5], 0.5, [1.0; 3]);
        p.line([0.0, 0.5], [0.0, 1e9], 0.5, [1.0; 3]);
        p.line([f32::NAN, 0.5], [0.0, f32::INFINITY], 0.5, [1.0; 3]);
        p.line([f32::NEG_INFINITY, 0.5], [f32::INFINITY, 0.5], 0.5, [1.0; 3]);
        assert_eq!(row(&r, 4), [1.0; 9]);
        assert_eq!(row(&r, 6), [0.0; 9]);
    }

    #[test]
    fn cuts_clear_and_stripes_keep_to_their_rows() {
        let mut r = raster(9, 6);
        let mut p = paint(&mut r, 1.0);
        p.rect([-4.5, 0.0], [4.5, 6.0], [1.0; 3]);
        p.cut([0.0, 0.5], [0.0, 5.5], 0.5);
        assert_eq!(row(&r, 3), [1.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
        // A striped line down the gap: every other pair of rows.
        paint(&mut r, 1.0).line_where([0.0, 0.5], [0.0, 5.5], 0.5, [1.0, 0.0, 0.0], |y| {
            (y / 2.0).floor() % 2.0 == 0.0
        });
        let lit: Vec<bool> = (0..6).map(|y| r.get(4, y).a > 0.0).collect();
        assert_eq!(lit, [true, true, false, false, true, true]);
    }

    #[test]
    fn a_dancer_stays_on_its_own_columns_and_a_glow_goes_behind_it() {
        let mut r = raster(9, 3);
        let mut p = paint(&mut r, 1.0);
        p.columns = (3, 6);
        p.rect([-10.0, 1.0], [10.0, 3.0], [1.0; 3]);
        p.glow_behind([0.0, 1.5], [20.0, 20.0], [0.0, 0.0, 1.0], 0.5);
        assert_eq!(row(&r, 1), [0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]);
        // Where the dancer is, the dancer; where it isn't, the glow.
        let (on, off) = (r.get(4, 1), r.get(4, 0));
        assert_eq!((on.r, on.b), (1.0, 1.0));
        assert!(
            off.a > 0.3 && off.a < 0.5 && off.b == 1.0 && off.r == 0.0,
            "{off:?}"
        );
        assert_eq!(r.get(2, 0).a, 0.0);
    }
}
