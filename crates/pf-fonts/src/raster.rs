//! Drawing a letter's outline as coverage: how much of each pixel the shape covers, 0 to 255.
//! PixelFlow's own.
//!
//! The outline's curves are cut into short straight lines, and each line adds the area it
//! bounds to the pixels it crosses, signed by whether it runs down or up. Summing a row from
//! the left then gives each pixel the share of it that lies inside the shape: exactly, with no
//! sampling, so a letter's edges come out as partial brightness. Shapes that overlap (a letter
//! drawn as several pieces, or drawn twice to thicken it) count once, and a hole drawn the other
//! way round takes its area back out.

use skrifa::PathSink;

/// A straight piece of outline, in pixels: x to the right and y down.
pub(crate) type Line = [f32; 4];

/// How far a curve may stray from the straight lines that stand in for it, in pixels.
const TOLERANCE: f32 = 0.02;
/// The most straight lines one curve is cut into.
const MAX_STEPS: f32 = 64.0;
/// The widest or tallest letter drawn, in pixels; a font claiming more is clipped.
const MAX_SIDE: i32 = 2048;

/// Collects a letter's outline as straight lines, scaled from font units to pixels and, for a
/// made-up italic, slanted. The pen skrifa draws outlines with (its `PathSink`, which it also
/// calls `OutlinePen`).
pub(crate) struct Outline {
    scale: f32,
    /// How far right a point moves per pixel above the baseline.
    slant: f32,
    pub lines: Vec<Line>,
    start: (f32, f32),
    at: (f32, f32),
}

impl Outline {
    pub fn new(scale: f32, slant: f32) -> Self {
        Self {
            scale,
            slant,
            lines: Vec::new(),
            start: (0.0, 0.0),
            at: (0.0, 0.0),
        }
    }

    /// A font point (y up from the baseline) in pixels (y down).
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        let (x, y) = (x * self.scale, y * self.scale);
        (x + y * self.slant, -y)
    }

    fn line(&mut self, to: (f32, f32)) {
        let from = self.at;
        if from != to {
            self.lines.push([from.0, from.1, to.0, to.1]);
        }
        self.at = to;
    }
}

/// How many straight lines a curve whose control points stray `bend` pixels from a straight
/// path needs.
fn steps(bend: f32) -> usize {
    (bend / TOLERANCE).sqrt().ceil().clamp(1.0, MAX_STEPS) as usize
}

impl PathSink for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.start = self.point(x, y);
        self.at = self.start;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let to = self.point(x, y);
        self.line(to);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (p0, p1, p2) = (self.at, self.point(x1, y1), self.point(x, y));
        let bend = ((p0.0 - 2.0 * p1.0 + p2.0).hypot(p0.1 - 2.0 * p1.1 + p2.1)) / 4.0;
        let n = steps(bend);
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            let q = (
                u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
                u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
            );
            self.line(q);
        }
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (p0, p1, p2, p3) = (self.at, self.point(x1, y1), self.point(x2, y2), self.point(x, y));
        let bend = (p0.0 - 2.0 * p1.0 + p2.0)
            .hypot(p0.1 - 2.0 * p1.1 + p2.1)
            .max((p1.0 - 2.0 * p2.0 + p3.0).hypot(p1.1 - 2.0 * p2.1 + p3.1))
            * 0.75;
        let n = steps(bend);
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            let q = (
                a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
                a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
            );
            self.line(q);
        }
    }

    fn close(&mut self) {
        let start = self.start;
        self.line(start);
    }
}

/// A letter drawn as coverage, row by row from its top left.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Bitmap {
    /// The drawing's top left, in pixels from where the letter starts on its baseline.
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    pub coverage: Vec<u8>,
}

/// Fills the shape the closed outline `lines` bounds.
pub(crate) fn fill(lines: &[Line]) -> Bitmap {
    let mut bounds = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    for l in lines.iter().filter(|l| l.iter().all(|v| v.is_finite())) {
        bounds[0] = bounds[0].min(l[0]).min(l[2]);
        bounds[1] = bounds[1].min(l[1]).min(l[3]);
        bounds[2] = bounds[2].max(l[0]).max(l[2]);
        bounds[3] = bounds[3].max(l[1]).max(l[3]);
    }
    if bounds[0] > bounds[2] || bounds[1] > bounds[3] {
        return Bitmap::default();
    }
    let left = bounds[0].floor().max(-(MAX_SIDE as f32)) as i32;
    let top = bounds[1].floor().max(-(MAX_SIDE as f32)) as i32;
    let width = (bounds[2].ceil() as i32 - left).clamp(1, MAX_SIDE);
    let height = (bounds[3].ceil() as i32 - top).clamp(1, MAX_SIDE);
    // Two cells more than the width per row: a line on the right edge adds to the cell past it.
    let stride = width as usize + 2;
    let mut area = vec![0.0f32; stride * height as usize];
    let (w, h) = (width as f32, height as f32);
    for l in lines.iter().filter(|l| l.iter().all(|v| v.is_finite())) {
        add_line(
            &mut area,
            stride,
            (w, h),
            (l[0] - left as f32, l[1] - top as f32),
            (l[2] - left as f32, l[3] - top as f32),
        );
    }
    let mut coverage = Vec::with_capacity((width * height) as usize);
    for row in area.chunks_exact(stride) {
        let mut sum = 0.0f32;
        for &a in &row[..width as usize] {
            sum += a;
            coverage.push((sum.abs().min(1.0) * 255.0 + 0.5) as u8);
        }
    }
    Bitmap {
        left,
        top,
        width,
        height,
        coverage,
    }
}

/// Adds the area one line bounds to the right of it, row by row, to `area`.
fn add_line(area: &mut [f32], stride: usize, (w, h): (f32, f32), a: (f32, f32), b: (f32, f32)) {
    if a.1 == b.1 {
        return;
    }
    let (sign, (x0, y0), (x1, y1)) = if a.1 < b.1 { (1.0, a, b) } else { (-1.0, b, a) };
    let (y0, y1) = (y0.clamp(0.0, h), y1.clamp(0.0, h));
    if y0 >= y1 {
        return;
    }
    let dxdy = (x1 - x0) / (b.1 - a.1).abs();
    let x_at = |y: f32| x0 + (y - a.1.min(b.1)) * dxdy;
    let mut row = y0.floor() as usize;
    while (row as f32) < y1 {
        let top = y0.max(row as f32);
        let bottom = y1.min(row as f32 + 1.0);
        if bottom > top {
            let cells = &mut area[row * stride..(row + 1) * stride];
            add_span(
                cells,
                x_at(top).clamp(0.0, w),
                x_at(bottom).clamp(0.0, w),
                (bottom - top) * sign,
            );
        }
        row += 1;
    }
}

/// Adds the area of one row's piece of a line, running from `xa` to `xb` across a height `dy`
/// (signed), to the cells it crosses and, past them, to the cell its row's sum carries on from.
fn add_span(cells: &mut [f32], xa: f32, xb: f32, dy: f32) {
    let (xl, xr) = if xa < xb { (xa, xb) } else { (xb, xa) };
    let first = xl.floor() as usize;
    let last = (xr.ceil() as usize).saturating_sub(1).max(first);
    if first == last {
        // The share of the cell to the right of the piece, times its height.
        let right = dy * ((first + 1) as f32 - (xl + xr) / 2.0);
        cells[first] += right;
        cells[first + 1] += dy - right;
        return;
    }
    let span = xr - xl;
    for cell in first..=last {
        let (from, to) = (xl.max(cell as f32), xr.min((cell + 1) as f32));
        if to <= from {
            continue;
        }
        let part = dy * (to - from) / span;
        let right = part * ((cell + 1) as f32 - (from + to) / 2.0);
        cells[cell] += right;
        cells[cell + 1] += part - right;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A closed polygon through `points`, as lines.
    fn polygon(points: &[(f32, f32)]) -> Vec<Line> {
        (0..points.len())
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % points.len()]);
                [a.0, a.1, b.0, b.1]
            })
            .collect()
    }

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Line> {
        polygon(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
    }

    fn at(b: &Bitmap, x: i32, y: i32) -> u8 {
        b.coverage[((y - b.top) * b.width + (x - b.left)) as usize]
    }

    #[test]
    fn a_square_on_whole_pixels_fills_them_and_nothing_else() {
        let b = fill(&rect(1.0, 2.0, 4.0, 5.0));
        assert_eq!((b.left, b.top, b.width, b.height), (1, 2, 3, 3));
        assert!(b.coverage.iter().all(|&c| c == 255));
    }

    #[test]
    fn edges_between_pixels_cover_them_partly() {
        let b = fill(&rect(0.5, 0.0, 2.5, 1.0));
        assert_eq!((b.left, b.width), (0, 3));
        assert_eq!(b.coverage, vec![128, 255, 128]);
        // A diagonal halves the pixels it crosses.
        let t = fill(&polygon(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0)]));
        assert_eq!(
            (at(&t, 0, 0), at(&t, 1, 0), at(&t, 1, 1), at(&t, 0, 1)),
            (128, 255, 128, 0)
        );
    }

    #[test]
    fn holes_are_left_open_and_overlaps_count_once() {
        // A ring: the hole runs the other way round.
        let mut ring = rect(0.0, 0.0, 5.0, 5.0);
        ring.extend(polygon(&[(1.0, 1.0), (1.0, 4.0), (4.0, 4.0), (4.0, 1.0)]));
        let b = fill(&ring);
        assert_eq!((at(&b, 0, 0), at(&b, 2, 2), at(&b, 4, 4)), (255, 0, 255));
        // Two squares overlapping the same way round: their union, no brighter where they meet.
        let mut both = rect(0.0, 0.0, 3.0, 1.0);
        both.extend(rect(2.0, 0.0, 5.0, 1.0));
        let b = fill(&both);
        assert_eq!(b.coverage, vec![255; 5]);
        // Either way round gives the same shape.
        let backwards = polygon(&[(0.0, 0.0), (0.0, 2.0), (2.0, 2.0), (2.0, 0.0)]);
        assert_eq!(fill(&backwards), fill(&rect(0.0, 0.0, 2.0, 2.0)));
    }

    #[test]
    fn curves_are_followed_closely() {
        // A circle of radius 10 drawn as eight quadratic curves: an octagon and the eight
        // parabolic pieces outside its sides.
        let mut o = Outline::new(1.0, 0.0);
        let k = std::f32::consts::FRAC_PI_4;
        o.move_to(10.0, 0.0);
        for i in 0..8 {
            let (a, b) = (i as f32 * k, (i + 1) as f32 * k);
            let mid = (a + b) / 2.0;
            let r = 10.0 / (k / 2.0).cos();
            o.quad_to(r * mid.cos(), r * mid.sin(), 10.0 * b.cos(), 10.0 * b.sin());
        }
        o.close();
        let b = fill(&o.lines);
        let total: f32 = b.coverage.iter().map(|&c| f32::from(c) / 255.0).sum();
        let (half, chord) = (k / 2.0, 20.0 * (k / 2.0).sin());
        let bulge = 0.5 * (10.0 / half.cos() - 10.0 * half.cos());
        let shape = 400.0 * k.sin() + 8.0 * (2.0 / 3.0) * chord * bulge;
        // Straight lines cut a little inside each curve: at most the tolerance, along the edge.
        let perimeter = 2.0 * std::f32::consts::PI * 10.0;
        assert!(
            (total - shape).abs() < perimeter * TOLERANCE,
            "{total} against {shape}"
        );
        assert_eq!((b.left, b.top, b.width, b.height), (-10, -10, 20, 20));
    }

    #[test]
    fn a_slant_leans_the_top_to_the_right() {
        let mut o = Outline::new(1.0, 0.5);
        o.move_to(0.0, 0.0);
        o.line_to(2.0, 0.0);
        o.line_to(2.0, 4.0);
        o.line_to(0.0, 4.0);
        o.close();
        let b = fill(&o.lines);
        // The top row starts 2 pixels further right than the bottom one.
        assert_eq!((b.left, b.width, b.top, b.height), (0, 4, -4, 4));
        assert_eq!(at(&b, 2, -4), 255);
        assert_eq!(at(&b, 1, -1), 255);
        assert_eq!(at(&b, 0, -4), 0);
    }

    #[test]
    fn nothing_to_fill_is_empty() {
        assert_eq!(fill(&[]), Bitmap::default());
        assert_eq!(fill(&[[f32::NAN, 0.0, 1.0, 1.0]]), Bitmap::default());
    }
}
