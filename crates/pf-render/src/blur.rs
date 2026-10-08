//! Blur, the way xLights blurs a layer (`PixelBufferClass::Blur` in `PixelBuffer.cpp`).
//!
//! xLights draws every effect on a grid (the model's buffer) and blurs the grid before the
//! layers are mixed, so a blurred pixel takes in the colors drawn between pixels too. PixelFlow
//! draws on the pixels themselves, so a blurred effect is drawn on a [`Grid`] laid over the
//! target's buffer (its columns × rows), blurred there, and each pixel takes its cell's color.
//!
//! xLights' Blur setting `b` (1 = none; PixelFlow stores `b - 1`):
//!
//! - `b > 2` on a grid wider and taller than 6 cells: an approximate Gaussian, three box blurs
//!   (each across, then up and down) whose sizes come from `boxesForGauss(b - 1)`, with the
//!   edge cells repeated past the edges;
//! - otherwise: one box `b` cells wide (centered, an even width reaching further left and down),
//!   averaging only the cells inside the grid.
//!
//! Red, green, blue, and coverage are blurred separately, as xLights blurs its color channels and
//! alpha. Under Normal (and for the lowest effect on a row) the colors are blurred as drawn and
//! the blurred coverage then dims them again when the layer is mixed, as in xLights, where those
//! layers draw with alpha. Under every other blend, xLights' effects draw dim colors instead of
//! alpha and the mix ignores alpha, so there the colors are blurred as they show (`premultiplied`).
//! xLights rounds the result to whole 8-bit levels; PixelFlow keeps fractions.

use crate::color::Rgba;
use crate::geometry::{Pixel, PixelBuffer};
use std::collections::VecDeque;

/// Most cells in a blur grid (a target with more columns × rows is blurred on a coarser grid).
const MAX_CELLS: usize = 1 << 22;

/// A target's buffer laid on a grid: each pixel's cell, and the pixel each cell is drawn as.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Grid {
    pub columns: usize,
    pub rows: usize,
    /// The cell (`row * columns + column`) of each of the buffer's pixels, in buffer order.
    pub cell_of: Vec<u32>,
    /// What each cell is drawn as: the first of the buffer's pixels in it, or, for a cell with
    /// none, its own position with the wiring place of the nearest pixel (so effects that run
    /// along the wiring carry on between pixels).
    pub cells: Vec<Pixel>,
}

impl Grid {
    pub fn new(buffer: &PixelBuffer) -> Self {
        let (mut columns, mut rows) = (buffer.columns.max(1) as usize, buffer.rows.max(1) as usize);
        if columns.saturating_mul(rows) > MAX_CELLS {
            let shrink = (MAX_CELLS as f64 / (columns as f64 * rows as f64)).sqrt();
            columns = ((columns as f64 * shrink) as usize).max(1);
            rows = ((rows as f64 * shrink) as usize).max(1);
        }
        let place = |at: f32, cells: usize| -> usize {
            if cells <= 1 || !at.is_finite() {
                0
            } else {
                ((at.clamp(0.0, 1.0) * (cells - 1) as f32).round() as usize).min(cells - 1)
            }
        };
        let total = columns * rows;
        let mut first: Vec<Option<u32>> = vec![None; total];
        let cell_of: Vec<u32> = buffer
            .pixels
            .iter()
            .enumerate()
            .map(|(k, px)| {
                let cell = place(px.v, rows) * columns + place(px.u, columns);
                first[cell].get_or_insert(k as u32);
                cell as u32
            })
            .collect();
        // Every empty cell takes the wiring place of the nearest pixel (breadth first from the
        // pixels' cells, so it's the nearest in grid steps).
        let mut nearest = first.clone();
        let mut queue: VecDeque<usize> = (0..total).filter(|&c| first[c].is_some()).collect();
        while let Some(cell) = queue.pop_front() {
            let (row, column) = (cell / columns, cell % columns);
            let from = nearest[cell];
            let mut visit = |next: usize| {
                if nearest[next].is_none() {
                    nearest[next] = from;
                    queue.push_back(next);
                }
            };
            if column > 0 {
                visit(cell - 1);
            }
            if column + 1 < columns {
                visit(cell + 1);
            }
            if row > 0 {
                visit(cell - columns);
            }
            if row + 1 < rows {
                visit(cell + columns);
            }
        }
        let at = |i: usize, cells: usize| {
            if cells <= 1 {
                0.5
            } else {
                i as f32 / (cells - 1) as f32
            }
        };
        let count = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
        let cells = (0..total)
            .map(|cell| match first[cell] {
                Some(k) => buffer.pixels[k as usize],
                None => Pixel {
                    u: at(cell % columns, columns),
                    v: at(cell / columns, rows),
                    index: nearest[cell].map_or(0, |k| buffer.pixels[k as usize].index),
                    count,
                },
            })
            .collect();
        Self {
            columns,
            rows,
            cell_of,
            cells,
        }
    }
}

/// The box sizes xLights uses for a Gaussian of size `d` (`boxesForGauss`, sizes 2 to 15).
fn boxes_for_gauss(d: u32) -> [usize; 3] {
    let d = d.clamp(2, 15);
    let b = match d {
        2 | 3 => 1,
        4..=6 => 3,
        7..=9 => 5,
        10..=12 => 7,
        _ => 9,
    };
    let second = if matches!(d, 2 | 4 | 5 | 7 | 8 | 10 | 11 | 13 | 14) {
        b
    } else {
        b + 2
    };
    let third = if matches!(d, 4 | 7 | 10 | 13) { b } else { b + 2 };
    [b, second, third]
}

type Px = [f32; 4];

fn to_px(c: Rgba, premultiplied: bool) -> Px {
    let k = if premultiplied { c.a } else { 1.0 };
    [c.r * k, c.g * k, c.b * k, c.a]
}

fn from_px(px: Px, premultiplied: bool) -> Rgba {
    let k = match (premultiplied, px[3] > 0.0) {
        (false, _) => 1.0,
        (true, true) => 1.0 / px[3],
        (true, false) => 0.0,
    };
    Rgba::new(px[0] * k, px[1] * k, px[2] * k, px[3])
}

/// Blurs `cells` (a `columns` × `rows` grid, row by row from the bottom) by xLights' Blur
/// setting `amount` (1 or less: no blur), the colors as drawn or, `premultiplied`, as they show.
/// `scratch` is reused between calls.
pub(crate) fn blur(
    cells: &mut [Rgba],
    columns: usize,
    rows: usize,
    amount: u32,
    premultiplied: bool,
    scratch: &mut Vec<Px>,
) {
    if amount < 2 || (columns <= 1 && rows <= 1) || cells.len() != columns * rows {
        return;
    }
    let mut grid: Vec<Px> = cells.iter().map(|&c| to_px(c, premultiplied)).collect();
    scratch.clear();
    scratch.resize(grid.len(), [0.0; 4]);
    if amount > 2 && columns > 6 && rows > 6 {
        for size in boxes_for_gauss(amount - 1) {
            let radius = (size - 1) / 2;
            box_across(&grid, scratch, columns, rows, radius);
            box_up(scratch, &mut grid, columns, rows, radius);
        }
    } else {
        // An even width reaches one further left (and down) than right (and up).
        let b = amount as usize;
        let (before, after) = if b.is_multiple_of(2) {
            (b / 2, (b - 1) / 2)
        } else {
            ((b - 1) / 2, (b - 1) / 2)
        };
        small_box(&grid, scratch, columns, rows, before, after);
        std::mem::swap(&mut grid, scratch);
    }
    for (cell, px) in cells.iter_mut().zip(&grid) {
        *cell = from_px(*px, premultiplied);
    }
}

fn add(sum: &mut Px, px: Px, k: f32) {
    for (s, v) in sum.iter_mut().zip(px) {
        *s += v * k;
    }
}

/// One box blur across each row, `2 * radius + 1` cells wide, edge cells repeated past the edges.
fn box_across(src: &[Px], dst: &mut [Px], columns: usize, rows: usize, radius: usize) {
    let scale = 1.0 / (2 * radius + 1) as f32;
    for row in 0..rows {
        let line = &src[row * columns..(row + 1) * columns];
        let at = |i: isize| line[i.clamp(0, columns as isize - 1) as usize];
        let r = radius as isize;
        let mut sum = [0.0f32; 4];
        for i in -r..=r {
            add(&mut sum, at(i), 1.0);
        }
        for x in 0..columns {
            dst[row * columns + x] = sum.map(|s| s * scale);
            let x = x as isize;
            add(&mut sum, at(x + r + 1), 1.0);
            add(&mut sum, at(x - r), -1.0);
        }
    }
}

/// One box blur up each column (see [`box_across`]).
fn box_up(src: &[Px], dst: &mut [Px], columns: usize, rows: usize, radius: usize) {
    let scale = 1.0 / (2 * radius + 1) as f32;
    for column in 0..columns {
        let at = |i: isize| src[i.clamp(0, rows as isize - 1) as usize * columns + column];
        let r = radius as isize;
        let mut sum = [0.0f32; 4];
        for i in -r..=r {
            add(&mut sum, at(i), 1.0);
        }
        for y in 0..rows {
            dst[y * columns + column] = sum.map(|s| s * scale);
            let y = y as isize;
            add(&mut sum, at(y + r + 1), 1.0);
            add(&mut sum, at(y - r), -1.0);
        }
    }
}

/// The average of the cells from `before` cells left (down) to `after` cells right (up), counting
/// only cells inside the grid.
fn small_box(src: &[Px], dst: &mut [Px], columns: usize, rows: usize, before: usize, after: usize) {
    for y in 0..rows {
        let (y0, y1) = (y.saturating_sub(before), (y + after).min(rows - 1));
        for x in 0..columns {
            let (x0, x1) = (x.saturating_sub(before), (x + after).min(columns - 1));
            let mut sum = [0.0f32; 4];
            for j in y0..=y1 {
                for i in x0..=x1 {
                    add(&mut sum, src[j * columns + i], 1.0);
                }
            }
            let n = ((y1 - y0 + 1) * (x1 - x0 + 1)) as f32;
            dst[y * columns + x] = sum.map(|s| s / n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    fn grid(columns: usize, rows: usize, lit: &[(usize, usize)]) -> Vec<Rgba> {
        let mut cells = vec![Rgba::CLEAR; columns * rows];
        for &(x, y) in lit {
            cells[y * columns + x] = Rgba::opaque([1.0, 0.5, 0.0]);
        }
        cells
    }

    #[test]
    fn gauss_boxes_match_xlights_table() {
        assert_eq!(boxes_for_gauss(2), [1, 1, 3]);
        assert_eq!(boxes_for_gauss(6), [3, 5, 5]);
        assert_eq!(boxes_for_gauss(7), [5, 5, 5]);
        assert_eq!(boxes_for_gauss(11), [7, 7, 9]);
        assert_eq!(boxes_for_gauss(14), [9, 9, 11]);
        assert_eq!(boxes_for_gauss(40), [9, 11, 11], "past 15 counts as 15");
    }

    #[test]
    fn one_means_no_blur() {
        let mut cells = grid(10, 10, &[(5, 5)]);
        let before = cells.clone();
        blur(&mut cells, 10, 10, 1, false, &mut Vec::new());
        assert_eq!(cells, before);
    }

    #[test]
    fn a_blur_of_two_averages_each_cell_with_the_ones_left_and_below() {
        // b = 2: a 2 × 2 box from one cell left and down to the cell itself.
        let mut cells = grid(4, 4, &[(1, 1)]);
        blur(&mut cells, 4, 4, 2, false, &mut Vec::new());
        for (x, y) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
            let c = cells[y * 4 + x];
            assert!(
                close(c.a, 0.25) && close(c.r, 0.25) && close(c.g, 0.125),
                "{x},{y}: {c:?}"
            );
        }
        assert_eq!(cells[0], Rgba::CLEAR);
        // At the corner only the cells inside count: (0, 0) averages itself alone.
        let mut corner = grid(4, 4, &[(0, 0)]);
        blur(&mut corner, 4, 4, 2, false, &mut Vec::new());
        assert!(close(corner[0].a, 1.0));
    }

    #[test]
    fn small_grids_use_one_box_even_for_big_blurs() {
        // 1 × 9 (a line): b = 5 averages 5 cells, two each side, inside the grid only.
        let mut cells = grid(9, 1, &[(4, 0)]);
        blur(&mut cells, 9, 1, 5, false, &mut Vec::new());
        let a: Vec<f32> = cells.iter().map(|c| c.a).collect();
        for (x, want) in [(1, 0.0), (2, 0.2), (4, 0.2), (6, 0.2), (7, 0.0)] {
            assert!(close(a[x], want), "{x}: {a:?}");
        }
    }

    #[test]
    fn big_grids_get_a_smooth_symmetric_blur_that_keeps_the_light() {
        let mut cells = grid(21, 21, &[(10, 10)]);
        blur(&mut cells, 21, 21, 8, false, &mut Vec::new());
        let a = |x: usize, y: usize| cells[y * 21 + x].a;
        // Three 5-wide boxes: the light spreads 6 cells each way, peaking at the center.
        assert!(a(10, 10) > a(11, 10) && a(11, 10) > a(12, 10));
        assert!(close(a(9, 10), a(11, 10)) && close(a(10, 9), a(10, 11)) && close(a(9, 10), a(10, 9)));
        assert!(a(4, 10) > 0.0 && close(a(3, 10), 0.0));
        let total: f32 = cells.iter().map(|c| c.a).sum();
        assert!(close(total, 1.0), "the light is spread, not lost: {total}");
        // Each box is (5 / 25 per axis): the center gets (19/125)² of it, as three 5-boxes give.
        assert!(close(a(10, 10), (19.0f32 / 125.0).powi(2)), "{}", a(10, 10));
    }

    #[test]
    fn colors_blur_as_drawn_or_as_they_show() {
        // A half-covered white cell beside a clear one, blurred with b = 2 across a 2 × 1 grid.
        let cells = || vec![Rgba::CLEAR, Rgba::with_alpha([1.0; 3], 0.5)];
        let mut drawn = cells();
        blur(&mut drawn, 2, 1, 2, false, &mut Vec::new());
        // As drawn: color and coverage each average with the clear cell (shows 0.5 × 0.25).
        assert!(close(drawn[1].r, 0.5) && close(drawn[1].a, 0.25));
        let mut shown = cells();
        blur(&mut shown, 2, 1, 2, true, &mut Vec::new());
        // As they show: what shows averages (0.5 → 0.25), the color keeps full strength.
        assert!(close(shown[1].r * shown[1].a, 0.25) && close(shown[1].a, 0.25));
        assert_eq!(shown[0], Rgba::CLEAR);
    }

    #[test]
    fn edges_repeat_past_the_edge_in_the_gaussian() {
        // A lit left column stays fully lit at the edge (the edge repeats outward).
        let lit: Vec<(usize, usize)> = (0..10).map(|y| (0, y)).collect();
        let mut cells = grid(10, 10, &lit);
        blur(&mut cells, 10, 10, 3, false, &mut Vec::new());
        // b = 3: boxes 1, 1, 3, so one 3-wide pass: the edge keeps 2/3, the next column 1/3.
        assert!(close(cells[5 * 10].a, 2.0 / 3.0), "{:?}", cells[5 * 10]);
        assert!(close(cells[5 * 10 + 1].a, 1.0 / 3.0));
        assert!(close(cells[5 * 10 + 2].a, 0.0));
    }
}
