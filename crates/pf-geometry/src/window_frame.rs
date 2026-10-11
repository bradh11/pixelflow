//! Window frames as xLights lays them out (`WindowFrameModel::InitFrame`), scaled to the prop's
//! width and height.

use crate::OwnGrid;
use pf_model::{Corner, Vec3};

/// The settings of a window frame (see `pf_model::Generator::WindowFrame`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frame {
    pub top: u32,
    pub sides: u32,
    pub bottom: u32,
    pub width: f32,
    pub height: f32,
    pub start: Corner,
    pub counter_clockwise: bool,
}

pub(crate) fn positions(f: Frame) -> Vec<Vec3> {
    let (top, side, bottom) = (i64::from(f.top), i64::from(f.sides), i64::from(f.bottom));
    // xLights' units: the sides' pixels are one apart and the frame is two wider than its
    // longer row.
    let kx = f.width / (top.max(bottom) + 2) as f32;
    let ky = f.height / (side - 1).max(1) as f32;
    xlights_layout(top, side, bottom, f.start, f.counter_clockwise)
        .into_iter()
        .map(|(at, _)| Vec3::new(at[0] * kx, at[1] * ky, 0.0))
        .collect()
}

/// The grid a window frame's effects are drawn on, as xLights lays it out: the frame as the
/// outside of a matrix two columns wider than its longer row and as tall as its sides, the
/// sides in the first and last columns and the top and bottom along the top and bottom rows.
pub(crate) fn grid(top: u32, sides: u32, bottom: u32, start: Corner, counter_clockwise: bool) -> OwnGrid {
    let (columns, rows) = (top.max(bottom).saturating_add(2), sides.max(1));
    let cell = |at: i64, cells: u32| at.clamp(0, i64::from(cells) - 1) as u32;
    OwnGrid {
        columns,
        rows,
        cells: xlights_layout(
            i64::from(top),
            i64::from(sides),
            i64::from(bottom),
            start,
            counter_clockwise,
        )
        .into_iter()
        .map(|(_, [x, y])| [cell(x, columns), cell(y, rows)])
        .collect(),
    }
}

/// xLights' own layout, in its float arithmetic so pixels land exactly where xLights puts them
/// (including a lone top or bottom pixel sitting one step in from the corner): each pixel's
/// place on the screen, and its cell (column, row from the bottom) in xLights' buffer.
fn xlights_layout(top: i64, side: i64, bottom: i64, start: Corner, ccw: bool) -> Vec<([f32; 2], [i64; 2])> {
    let total = top + 2 * side + bottom;
    if total <= 0 {
        return Vec::new();
    }
    let ltor = matches!(start, Corner::BottomLeft | Corner::TopLeft);
    let btot = matches!(start, Corner::BottomLeft | Corner::BottomRight);
    let w = (top.max(bottom) + 2) as f32;
    let dir: f32 = if ccw { -1.0 } else { 1.0 };
    // The edge the string starts along takes the corners.
    let odd_corner = if ccw { btot == ltor } else { btot != ltor };
    let (wadj, hadj) = if odd_corner { (2, -2) } else { (0, 0) };
    let top_si = if top + wadj - 1 != 0 {
        w / (top + 1) as f32
    } else {
        1.0
    };
    let bot_si = if bottom + wadj - 1 != 0 {
        -w / (bottom + 1) as f32
    } else {
        1.0
    };
    // Edges: left (up), top (right), right (down), bottom (left), when going clockwise.
    let lengths = [side + hadj, top + wadj, side + hadj, bottom + wadj];
    let xsi = [0.0, top_si, 0.0, bot_si];
    let ysi = [1.0f32, 0.0, -1.0, 0.0];
    let hh = (side - 1) as f32 / 2.0;
    let (xs, ys): ([f32; 4], [f32; 4]) = match (ccw, odd_corner) {
        (true, true) => (
            [-w / 2.0, w / 2.0, w / 2.0, -w / 2.0],
            [hh - 1.0, hh, -hh + 1.0, -hh],
        ),
        (true, false) => (
            [-w / 2.0, w / 2.0 - top_si, w / 2.0, -w / 2.0 - bot_si],
            [hh, hh, -hh, -hh],
        ),
        (false, true) => (
            [-w / 2.0, -w / 2.0, w / 2.0, w / 2.0],
            [-hh + 1.0, hh, hh - 1.0, -hh],
        ),
        (false, false) => (
            [-w / 2.0, -w / 2.0 + top_si, w / 2.0, w / 2.0 + bot_si],
            [-hh, hh, hh, -hh],
        ),
    };
    // The order the edges are walked in, from the start corner.
    let idx: [usize; 4] = match (ltor, btot, ccw) {
        (true, true, false) => [0, 1, 2, 3],
        (true, true, true) => [3, 2, 1, 0],
        (true, false, false) => [1, 2, 3, 0],
        (true, false, true) => [0, 3, 2, 1],
        (false, true, false) => [3, 0, 1, 2],
        (false, true, true) => [2, 1, 0, 3],
        (false, false, false) => [2, 3, 0, 1],
        (false, false, true) => [1, 0, 3, 2],
    };
    let next_edge = |mut s: usize| {
        for _ in 0..4 {
            if lengths[idx[s]] != 0 {
                break;
            }
            s = (s + 1) % 4;
        }
        s
    };
    // The same walk in the buffer: where each edge starts, and how far apart its pixels are.
    let across = |pixels: i64| {
        if pixels == 0 {
            w - 1.0
        } else if pixels + wadj + 1 != 0 {
            (w + wadj as f32) / (pixels + wadj + 1) as f32
        } else {
            1.0
        }
    };
    let bxi = [0.0, across(top), 0.0, -across(bottom)];
    let (h, far) = (side as f32, w - 1.0);
    let (bxs, bys): ([f32; 4], [f32; 4]) = match (ccw, odd_corner) {
        (true, true) => ([0.0, far, far, 0.0], [h - 2.0, h - 1.0, 1.0, 0.0]),
        (true, false) => ([0.0, far - 1.0, far, 1.0], [h - 1.0, h - 1.0, 0.0, 0.0]),
        (false, true) => ([0.0, 0.0, far, far], [1.0, h - 1.0, h - 2.0, 0.0]),
        (false, false) => ([0.0, 1.0, far, far - 1.0], [0.0, h - 1.0, h - 1.0, 0.0]),
    };
    let mut s = next_edge(0);
    let (mut x, mut y) = (xs[idx[s]], ys[idx[s]]);
    let (mut bx, mut by) = (bxs[idx[s]], bys[idx[s]]);
    let mut left = lengths[idx[s]];
    let mut out = Vec::with_capacity(total as usize);
    for _ in 0..total {
        // A pixel between two columns takes the one behind it.
        let column = if bxi[idx[s]] * dir > 0.0 {
            bx.floor()
        } else {
            bx.ceil()
        };
        out.push(([x, y], [column as i64, by as i64]));
        x += xsi[idx[s]] * dir;
        y += ysi[idx[s]] * dir;
        bx += bxi[idx[s]] * dir;
        by += ysi[idx[s]] * dir;
        left -= 1;
        if left <= 0 {
            s = next_edge((s + 1) % 4);
            (x, y) = (xs[idx[s]], ys[idx[s]]);
            (bx, by) = (bxs[idx[s]], bys[idx[s]]);
            left = lengths[idx[s]];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_close;

    #[test]
    fn the_grid_drawn_on_is_the_outside_of_a_matrix() {
        // Two along the top and bottom and three up each side: four columns by three rows.
        // Clockwise from the bottom left: up the first column, along the top row from the
        // second column (4/3 of a column apart, rounded back), down the last column, and back
        // along the bottom row.
        let g = grid(2, 3, 2, Corner::BottomLeft, false);
        assert_eq!((g.columns, g.rows), (4, 3));
        assert_eq!(
            g.cells,
            [
                [0, 0],
                [0, 1],
                [0, 2],
                [1, 2],
                [2, 2],
                [3, 2],
                [3, 1],
                [3, 0],
                [2, 0],
                [1, 0]
            ]
        );
        // Counter-clockwise from the bottom left the bottom row comes first and takes the
        // corners: its pixels spread over all four columns.
        let ccw = grid(2, 3, 2, Corner::BottomLeft, true);
        assert_eq!(ccw.cells[..4], [[0, 0], [1, 0], [2, 0], [3, 0]]);
        assert_eq!(ccw.cells[4], [3, 1], "then the one pixel left for the right side");
        assert!(grid(0, 0, 0, Corner::BottomLeft, false).cells.is_empty());
    }

    fn frame(top: u32, sides: u32, bottom: u32, width: f32, height: f32) -> Frame {
        Frame {
            top,
            sides,
            bottom,
            width,
            height,
            start: Corner::BottomLeft,
            counter_clockwise: false,
        }
    }

    fn assert_all(p: &[Vec3], want: &[(f32, f32)]) {
        assert_eq!(p.len(), want.len());
        for (a, &(x, y)) in p.iter().zip(want) {
            assert_close(*a, Vec3::new(x, y, 0.0));
        }
    }

    #[test]
    fn goes_up_the_left_across_the_top_and_round_from_the_bottom_left() {
        // Three along the top and bottom, two up each side: five steps wide, one tall, doubled.
        let p = positions(frame(3, 2, 3, 10.0, 2.0));
        assert_all(
            &p,
            &[
                (-5.0, -1.0),
                (-5.0, 1.0),
                (-2.5, 1.0),
                (0.0, 1.0),
                (2.5, 1.0),
                (5.0, 1.0),
                (5.0, -1.0),
                (2.5, -1.0),
                (0.0, -1.0),
                (-2.5, -1.0),
            ],
        );
    }

    #[test]
    fn starting_along_the_top_the_top_and_bottom_take_the_corners() {
        let p = positions(Frame {
            start: Corner::TopLeft,
            ..frame(3, 3, 2, 5.0, 2.0)
        });
        let third = 5.0 / 3.0 - 2.5;
        assert_all(
            &p,
            &[
                (-2.5, 1.0),
                (-1.25, 1.0),
                (0.0, 1.0),
                (1.25, 1.0),
                (2.5, 1.0),
                (2.5, 0.0),
                (2.5, -1.0),
                (-third, -1.0),
                (third, -1.0),
                (-2.5, -1.0),
                (-2.5, 0.0),
            ],
        );
    }

    #[test]
    fn counter_clockwise_from_the_bottom_left_runs_along_the_bottom_first() {
        // Two up each side: the bottom and top take all four corners, so the sides have none.
        let p = positions(Frame {
            counter_clockwise: true,
            ..frame(2, 2, 2, 4.0, 1.0)
        });
        let t = 2.0 / 3.0;
        assert_all(
            &p,
            &[
                (-2.0, -0.5),
                (-t, -0.5),
                (t, -0.5),
                (2.0, -0.5),
                (2.0, 0.5),
                (t, 0.5),
                (-t, 0.5),
                (-2.0, 0.5),
            ],
        );
    }

    #[test]
    fn each_corner_starts_the_string_there() {
        let corners = [
            (Corner::BottomLeft, (-2.0, -0.5)),
            (Corner::BottomRight, (2.0, -0.5)),
            (Corner::TopLeft, (-2.0, 0.5)),
            (Corner::TopRight, (2.0, 0.5)),
        ];
        for (start, (x, y)) in corners {
            for counter_clockwise in [false, true] {
                let p = positions(Frame {
                    start,
                    counter_clockwise,
                    ..frame(2, 2, 2, 4.0, 1.0)
                });
                assert_eq!(p.len(), 8);
                assert_close(p[0], Vec3::new(x, y, 0.0));
            }
        }
    }

    #[test]
    fn a_lone_top_pixel_sits_one_step_in_from_the_corner_as_in_xlights() {
        let p = positions(frame(1, 2, 0, 3.0, 1.0));
        assert_all(
            &p,
            &[(-1.5, -0.5), (-1.5, 0.5), (-0.5, 0.5), (1.5, 0.5), (1.5, -0.5)],
        );
    }

    #[test]
    fn no_pixels_is_empty() {
        assert!(positions(frame(0, 0, 0, 1.0, 1.0)).is_empty());
    }
}
