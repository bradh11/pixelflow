//! A grid to draw on, with xLights' drawing primitives (`RenderBuffer`'s `SetPixel`, `DrawLine`,
//! `DrawCircle`, and friends in `RenderBuffer.cpp`), for the effects xLights draws shape by shape
//! (Shape, Morph, Circles) rather than pixel by pixel.
//!
//! The grid is the target's columns × rows, the same grid blur works on, with (0, 0) at the
//! bottom left as in xLights. An effect draws its frame on it once, then each pixel takes the color
//! of its cell. Later drawing covers earlier drawing, as `SetPixel` does.

use crate::color::Rgba;
use crate::effects::Canvas;
use crate::geometry::Pixel;

/// Most cells in a raster (a target with more columns × rows draws on a coarser grid).
const MAX_CELLS: usize = 1 << 20;

/// A target's size in cells, kept under [`MAX_CELLS`].
pub(crate) fn grid_size(canvas: Canvas) -> (i32, i32) {
    let (mut columns, mut rows) = (canvas.columns.max(1) as usize, canvas.rows.max(1) as usize);
    if columns.saturating_mul(rows) > MAX_CELLS {
        let shrink = (MAX_CELLS as f64 / (columns as f64 * rows as f64)).sqrt();
        columns = ((columns as f64 * shrink) as usize).max(1);
        rows = ((rows as f64 * shrink) as usize).max(1);
    }
    (columns as i32, rows as i32)
}

/// The cell a pixel falls in on a `columns` × `rows` grid (as blur's grid places pixels).
#[inline]
pub(crate) fn cell_of(px: &Pixel, columns: i32, rows: i32) -> (i32, i32) {
    let place = |at: f32, cells: i32| -> i32 {
        if cells <= 1 || !at.is_finite() {
            0
        } else {
            ((at.clamp(0.0, 1.0) * (cells - 1) as f32).round() as i32).min(cells - 1)
        }
    };
    (place(px.u, columns), place(px.v, rows))
}

#[derive(Debug, Clone)]
pub(crate) struct Raster {
    pub width: i32,
    pub height: i32,
    /// Each cell's color and coverage, as plain numbers so a new grid comes from zeroed memory
    /// (most effects light few of its cells).
    cells: Vec<[f32; 4]>,
}

#[inline]
fn color([r, g, b, a]: [f32; 4]) -> Rgba {
    Rgba::new(r, g, b, a)
}

#[inline]
fn stored(c: Rgba) -> [f32; 4] {
    [c.r, c.g, c.b, c.a]
}

impl Raster {
    pub fn new(canvas: Canvas) -> Self {
        let (width, height) = grid_size(canvas);
        Self {
            width,
            height,
            cells: vec![[0.0; 4]; (width * height) as usize],
        }
    }

    /// The color drawn where `px` is.
    #[inline]
    pub fn at(&self, px: &Pixel) -> Rgba {
        let (x, y) = cell_of(px, self.width, self.height);
        color(self.cells[(y * self.width + x) as usize])
    }

    /// The color drawn at a cell (clear outside the grid).
    pub fn get(&self, x: i32, y: i32) -> Rgba {
        if self.inside(x, y) {
            color(self.cells[(y * self.width + x) as usize])
        } else {
            Rgba::CLEAR
        }
    }

    fn inside(&self, x: i32, y: i32) -> bool {
        (0..self.width).contains(&x) && (0..self.height).contains(&y)
    }

    /// `SetPixel`: colors one cell; nothing outside the grid.
    #[inline]
    pub fn set(&mut self, x: i32, y: i32, color: Rgba) {
        if self.inside(x, y) {
            self.cells[(y * self.width + x) as usize] = stored(color);
        }
    }

    /// Draws a color partly covering a cell over what's there (the topmost coverage wins where
    /// it's whole, as `SetPixel` with alpha does).
    pub fn cover(&mut self, x: i32, y: i32, top: Rgba) {
        if !self.inside(x, y) {
            return;
        }
        let cell = &mut self.cells[(y * self.width + x) as usize];
        let a = top.a.clamp(0.0, 1.0);
        if a >= 1.0 || cell[3] <= 0.0 {
            *cell = stored(top);
            return;
        }
        let below = cell[3] * (1.0 - a);
        let out = a + below;
        let mix = |over: f32, under: f32| (over * a + under * below) / out;
        *cell = [mix(top.r, cell[0]), mix(top.g, cell[1]), mix(top.b, cell[2]), out];
    }

    /// `SetPixel` with wrapping: a cell off one side comes back on the other. As in xLights, one
    /// just past the right (top) edge is dropped rather than wrapped.
    pub fn set_wrapped(&mut self, mut x: i32, mut y: i32, color: Rgba) {
        if self.width > 0 && self.height > 0 {
            if x < 0 {
                x = x.rem_euclid(self.width);
            }
            if y < 0 {
                y = y.rem_euclid(self.height);
            }
            while x > self.width {
                x -= self.width;
            }
            while y > self.height {
                y -= self.height;
            }
        }
        self.set(x, y, color);
    }

    fn put(&mut self, x: i32, y: i32, color: Rgba, wrap: bool) {
        if wrap {
            self.set_wrapped(x, y, color);
        } else {
            self.set(x, y, color);
        }
    }

    /// `DrawLine`: Bresenham's line, both ends included.
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba) {
        // Wholly off one side: nothing to draw.
        if (x0 < 0 && x1 < 0)
            || (y0 < 0 && y1 < 0)
            || (x0 >= self.width && x1 >= self.width)
            || (y0 >= self.height && y1 >= self.height)
        {
            return;
        }
        let (dx, sx) = ((x1 - x0).abs(), if x0 < x1 { 1 } else { -1 });
        let (dy, sy) = ((y1 - y0).abs(), if y0 < y1 { 1 } else { -1 });
        let mut err = (if dx > dy { dx } else { -dy }) / 2;
        let (mut x, mut y) = (x0, y0);
        loop {
            self.set(x, y, color);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = err;
            if e2 > -dx {
                err -= dy;
                x += sx;
            }
            if e2 < dy {
                err += dx;
                y += sy;
            }
        }
    }

    /// The thick `DrawThickLine`: Bresenham's line with a second pixel at each diagonal step, on
    /// the side `direction` picks, so the line has no gaps to see through.
    pub fn line_without_gaps(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba, direction: bool) {
        let (dx, sx) = ((x1 - x0).abs(), if x0 < x1 { 1 } else { -1 });
        let (dy, sy) = ((y1 - y0).abs(), if y0 < y1 { 1 } else { -1 });
        let mut err = (if dx > dy { dx } else { -dy }) / 2;
        let (mut x, mut y) = (x0, y0);
        let (mut last_x, mut last_y) = (x, y);
        loop {
            self.set(x, y, color);
            if x != last_x && y != last_y && x0 != x1 && y0 != y1 {
                let fix = i32::from(x > last_x) + 2 * i32::from(y > last_y) + 4 * i32::from(direction);
                match fix {
                    2 | 4 if x < self.width - 2 => self.set(x + 1, y, color),
                    3 | 5 if x > 0 => self.set(x - 1, y, color),
                    0 | 1 if y < self.height - 2 => self.set(x, y + 1, color),
                    6 | 7 if y > 0 => self.set(x, y - 1, color),
                    _ => {}
                }
            }
            (last_x, last_y) = (x, y);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = err;
            if e2 > -dx {
                err -= dy;
                x += sx;
            }
            if e2 < dy {
                err += dx;
                y += sy;
            }
        }
    }

    /// `DrawCircle`: the midpoint circle, as an outline or filled.
    pub fn circle(&mut self, x0: i32, y0: i32, radius: i32, color: Rgba, filled: bool, wrap: bool) {
        let (mut x, mut y) = (radius, 0);
        let mut error = 1 - x;
        while x >= y {
            if filled {
                for (cx, from, to) in [
                    (x0 - x, y0 - y, y0 + y),
                    (x0 + x, y0 - y, y0 + y),
                    (x0 - y, y0 - x, y0 + x),
                    (x0 + y, y0 - x, y0 + x),
                ] {
                    for cy in from..=to {
                        self.put(cx, cy, color, wrap);
                    }
                }
            } else {
                for (cx, cy) in [
                    (x + x0, y + y0),
                    (y + x0, x + y0),
                    (-x + x0, y + y0),
                    (-y + x0, x + y0),
                    (-x + x0, -y + y0),
                    (-y + x0, -x + y0),
                    (x + x0, -y + y0),
                    (y + x0, -x + y0),
                ] {
                    self.put(cx, cy, color, wrap);
                }
            }
            y += 1;
            if error < 0 {
                error += 2 * y + 1;
            } else {
                x -= 1;
                error += 2 * (y - x) + 1;
            }
        }
    }

    /// `DrawFadingCircle`: a filled circle, full at its center and fading to nothing at its edge.
    pub fn fading_circle(&mut self, x0: i32, y0: i32, radius: i32, color: [f32; 3], wrap: bool) {
        if radius <= 0 {
            return;
        }
        for x in -radius..radius {
            for y in -radius..radius {
                let d = f64::from(x * x + y * y).sqrt();
                if d <= f64::from(radius) {
                    let level = 1.0 - d / f64::from(radius);
                    if level > 0.0 {
                        self.put(x + x0, y + y0, Rgba::with_alpha(color, level as f32), wrap);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raster(columns: u32, rows: u32) -> Raster {
        Raster::new(Canvas { columns, rows })
    }

    fn lit(r: &Raster) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        for y in 0..r.height {
            for x in 0..r.width {
                if r.get(x, y).a > 0.0 {
                    out.push((x, y));
                }
            }
        }
        out
    }

    #[test]
    fn lines_include_both_ends_and_clip_to_the_grid() {
        let mut r = raster(10, 10);
        r.line(0, 0, 3, 3, Rgba::opaque([1.0; 3]));
        assert_eq!(lit(&r), vec![(0, 0), (1, 1), (2, 2), (3, 3)]);
        let mut r = raster(5, 5);
        r.line(-3, 2, 8, 2, Rgba::opaque([1.0; 3]));
        assert_eq!(lit(&r).len(), 5);
    }

    #[test]
    fn circles_are_midpoint_circles() {
        let mut r = raster(11, 11);
        r.circle(5, 5, 2, Rgba::opaque([1.0; 3]), false, false);
        assert!(r.get(7, 5).a > 0.0 && r.get(5, 7).a > 0.0 && r.get(5, 5).a == 0.0);
        let mut filled = raster(11, 11);
        filled.circle(5, 5, 2, Rgba::opaque([1.0; 3]), true, false);
        assert_eq!(filled.get(5, 5).a, 1.0);
        assert_eq!(lit(&filled).len(), 21, "a radius-2 disc");
    }

    #[test]
    fn wrapping_brings_cells_back_on_the_other_side() {
        let mut r = raster(10, 4);
        r.set_wrapped(-1, 0, Rgba::opaque([1.0; 3]));
        r.set_wrapped(11, 1, Rgba::opaque([1.0; 3]));
        r.set_wrapped(10, 2, Rgba::opaque([1.0; 3]));
        assert_eq!(
            lit(&r),
            vec![(9, 0), (1, 1)],
            "one just past the edge is dropped, as in xLights"
        );
    }

    #[test]
    fn fading_circles_fade_to_their_edge() {
        let mut r = raster(11, 11);
        r.fading_circle(5, 5, 4, [1.0; 3], false);
        assert_eq!(r.get(5, 5).a, 1.0);
        assert!((r.get(7, 5).a - 0.5).abs() < 1e-6);
        assert_eq!(r.get(9, 5).a, 0.0);
    }

    #[test]
    fn huge_targets_draw_on_a_coarser_grid() {
        let (w, h) = grid_size(Canvas {
            columns: 1 << 16,
            rows: 1 << 16,
        });
        assert!((w as usize) * (h as usize) <= MAX_CELLS && w > 0 && h > 0);
    }
}
