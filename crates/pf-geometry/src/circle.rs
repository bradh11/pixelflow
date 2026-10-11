use crate::OwnGrid;
use pf_model::Vec3;
use std::f64::consts::{PI, TAU};

pub(crate) struct Circle<'a> {
    pub nodes: u32,
    pub radius: f32,
    pub layers: &'a [u32],
    pub inner_percent: u32,
    pub start_inside: bool,
    pub start_at_bottom: bool,
    pub counter_clockwise: bool,
}

/// Rings as xLights lays them out (`CircleModel::SetCircleCoord`): each ring's pixels evenly
/// round it from the top (or bottom), clockwise (or counter-clockwise). With layers (innermost
/// first), the rings are evenly spaced from `radius` in to `inner_percent` of it, the string going
/// round the outermost first (or the innermost); each ring holds its own count or what's left,
/// and pixels beyond the rings sit in the middle.
pub(crate) fn positions(c: Circle) -> Vec<Vec3> {
    let single = [c.nodes];
    let rings = if c.layers.len() > 1 { c.layers } else { &single[..] };
    let lc = rings.len();
    let (outer, inner) = (
        f64::from(c.radius),
        f64::from(c.radius) * f64::from(c.inner_percent) / 100.0,
    );
    let start = if c.start_at_bottom { -PI } else { 0.0 };
    let mut out = Vec::with_capacity(c.nodes as usize);
    let mut left = c.nodes;
    for k in 0..lc {
        let ring = if c.start_inside { k } else { lc - 1 - k };
        let radius = if lc == 1 {
            outer
        } else {
            inner + (outer - inner) * ring as f64 / (lc - 1) as f64
        };
        let count = left.min(rings[ring]);
        for n in 0..count {
            let mut angle = start + TAU * f64::from(n) / f64::from(count);
            if c.counter_clockwise {
                angle = -angle;
            }
            out.push(Vec3::new(
                (angle.sin() * radius) as f32,
                (angle.cos() * radius) as f32,
                0.0,
            ));
        }
        left -= count;
    }
    out.resize(c.nodes as usize, Vec3::ZERO);
    out
}

/// The grid a circle's effects are drawn on, as xLights lays it out (`CircleModel::InitCircle`):
/// a row for each ring (the outermost at the bottom), as wide as the longest; each ring's pixels
/// run along its row in wiring order, a shorter ring's spread across it. Pixels beyond the rings
/// share the first cell.
pub(crate) fn grid(nodes: u32, layers: &[u32], start_inside: bool) -> OwnGrid {
    let single = [nodes];
    let rings = if layers.len() > 1 { layers } else { &single[..] };
    let lc = rings.len();
    // Each ring's row and how many pixels it holds (its own count, or what's left), in wiring
    // order.
    let mut left = nodes;
    let held: Vec<(u32, u32)> = (0..lc)
        .map(|k| {
            let ring = if start_inside { k } else { lc - 1 - k };
            let count = left.min(rings[ring]);
            left -= count;
            ((lc - 1 - ring) as u32, count)
        })
        .collect();
    let longest = held.iter().map(|&(_, count)| count).max().unwrap_or(0).max(1);
    let mut cells = Vec::with_capacity(nodes as usize);
    for (row, count) in held {
        if count == 0 {
            continue;
        }
        // xLights keeps a ring that doesn't divide the longest evenly off the last column.
        let fudge = 1 - i64::from(longest / count);
        let span = (i64::from(longest) - 1 + fudge) as f64;
        for n in 0..count {
            let column = if count == longest || count == 1 {
                n
            } else {
                (f64::from(n) / f64::from(count - 1) * span).floor() as u32
            };
            cells.push([column, row]);
        }
    }
    cells.resize(nodes as usize, [0, 0]);
    OwnGrid {
        columns: longest,
        rows: lc as u32,
        cells,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    #[test]
    fn the_grid_drawn_on_is_a_row_for_each_ring() {
        // One ring is a single row, its pixels in wiring order.
        let one = grid(5, &[], false);
        assert_eq!((one.columns, one.rows), (5, 1));
        assert_eq!(one.cells, [[0, 0], [1, 0], [2, 0], [3, 0], [4, 0]]);
        // Rings of 2 (inside) and 4, wired from the outside: the outer ring is the bottom row,
        // and the inner ring's pixels spread over the row but stay off its last column (xLights'
        // "fudge" for a ring that divides the longest evenly: floor(n / 1 × (4 - 1 - 1))).
        let two = grid(6, &[2, 4], false);
        assert_eq!((two.columns, two.rows), (4, 2));
        assert_eq!(two.cells, [[0, 0], [1, 0], [2, 0], [3, 0], [0, 1], [2, 1]]);
        // Wired from the inside, the inner ring still has the top row; a pixel beyond the rings
        // shares the first cell.
        let inside_out = grid(7, &[2, 4], true);
        assert_eq!(inside_out.cells[..3], [[0, 1], [2, 1], [0, 0]]);
        assert_eq!(inside_out.cells[6], [0, 0]);
    }

    fn ring(nodes: u32) -> Circle<'static> {
        Circle {
            nodes,
            radius: 1.0,
            layers: &[],
            inner_percent: 50,
            start_inside: false,
            start_at_bottom: false,
            counter_clockwise: false,
        }
    }

    #[test]
    fn starts_at_top_and_runs_clockwise() {
        let p = positions(ring(4));
        assert_close(p[0], Vec3::new(0.0, 1.0, 0.0));
        assert_close(p[1], Vec3::new(1.0, 0.0, 0.0));
        assert_close(p[2], Vec3::new(0.0, -1.0, 0.0));
        assert_close(p[3], Vec3::new(-1.0, 0.0, 0.0));
    }

    #[test]
    fn starts_at_the_bottom_and_runs_counter_clockwise() {
        let p = positions(Circle {
            start_at_bottom: true,
            counter_clockwise: true,
            ..ring(4)
        });
        assert_close(p[0], Vec3::new(0.0, -1.0, 0.0));
        assert_close(p[1], Vec3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn rings_go_from_the_outside_in_or_the_inside_out() {
        // 2 pixels inside at half the size, 4 outside.
        let layers = [2, 4];
        let p = positions(Circle {
            layers: &layers,
            ..ring(6)
        });
        assert_close(p[0], Vec3::new(0.0, 1.0, 0.0));
        assert_close(p[4], Vec3::new(0.0, 0.5, 0.0));
        assert_close(p[5], Vec3::new(0.0, -0.5, 0.0));
        let q = positions(Circle {
            layers: &layers,
            start_inside: true,
            ..ring(7)
        });
        assert_close(q[1], Vec3::new(0.0, -0.5, 0.0));
        assert_close(q[3], Vec3::new(1.0, 0.0, 0.0));
        // The pixel beyond the rings sits in the middle.
        assert_close(q[6], Vec3::ZERO);
        // Fewer pixels than the rings hold: the last ring spreads what it gets.
        let r = positions(Circle {
            layers: &layers,
            ..ring(5)
        });
        assert_close(r[4], Vec3::new(0.0, 0.5, 0.0));
    }
}
