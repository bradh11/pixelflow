//! Resizing a picture's frame: every output cell is a weighted mix of the input pixels it covers.
//!
//! Shrinking averages the whole area a cell covers, so a line a pixel wide in a large picture
//! still shows on a small matrix; enlarging blends between neighbors. `crisp` takes the nearest
//! pixel either way, which keeps pixel art sharp. Clear pixels add no color to the mix, so a
//! picture's edges don't darken.

/// One frame: RGBA, a byte each, rows from the top, color not multiplied by alpha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

impl Bitmap {
    pub fn bytes(&self) -> usize {
        self.pixels.len() * 4
    }

    #[inline]
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        self.pixels[(y * self.width + x) as usize]
    }
}

/// The input pixels each output cell along one axis mixes, and how much of each.
struct Axis {
    /// Per output cell: its first input pixel, and where its weights start and end.
    spans: Vec<(u32, u32, u32)>,
    weights: Vec<f32>,
}

impl Axis {
    fn new(from: u32, to: u32, crisp: bool) -> Self {
        let mut axis = Axis {
            spans: Vec::with_capacity(to as usize),
            weights: Vec::new(),
        };
        let step = f64::from(from) / f64::from(to);
        let last = from - 1;
        for i in 0..to {
            let start = axis.weights.len() as u32;
            let first = if from == to {
                axis.weights.push(1.0);
                i
            } else if crisp {
                axis.weights.push(1.0);
                (((f64::from(i) + 0.5) * step) as u32).min(last)
            } else if to < from {
                // The stretch of input this cell covers, and each pixel's share of it.
                let (a, b) = (f64::from(i) * step, f64::from(i + 1) * step);
                let first = (a.floor() as u32).min(last);
                let end = (b.ceil() as u32).clamp(first + 1, from);
                for px in first..end {
                    let covered = (f64::from(px + 1).min(b) - f64::from(px).max(a)).max(0.0);
                    axis.weights.push((covered / step) as f32);
                }
                first
            } else {
                // Between the two nearest pixel middles.
                let at = ((f64::from(i) + 0.5) * step - 0.5).clamp(0.0, f64::from(last));
                let first = (at.floor() as u32).min(last);
                let part = (at - f64::from(first)) as f32;
                if first == last || part <= 0.0 {
                    axis.weights.push(1.0);
                } else {
                    axis.weights.extend([1.0 - part, part]);
                }
                first
            };
            axis.spans.push((first, start, axis.weights.len() as u32));
        }
        axis
    }

    /// Whether every output cell is one input pixel.
    fn copies(&self) -> bool {
        self.weights.len() == self.spans.len()
    }
}

/// Makes pixels no brighter than `level` (red + green + blue, 0–765) clear.
pub(crate) fn clear_black(bitmap: &mut Bitmap, level: u16) {
    for p in &mut bitmap.pixels {
        if u16::from(p[0]) + u16::from(p[1]) + u16::from(p[2]) <= level {
            *p = [0; 4];
        }
    }
}

/// `source` at `width` × `height`.
pub(crate) fn resize(source: &Bitmap, width: u32, height: u32, crisp: bool) -> Bitmap {
    let (width, height) = (width.max(1), height.max(1));
    if source.width == 0 || source.height == 0 {
        return Bitmap {
            width,
            height,
            pixels: vec![[0; 4]; (width * height) as usize],
        };
    }
    let (across, down) = (
        Axis::new(source.width, width, crisp),
        Axis::new(source.height, height, crisp),
    );
    let mut pixels = Vec::with_capacity((width * height) as usize);
    if across.copies() && down.copies() {
        for &(y, ..) in &down.spans {
            pixels.extend(across.spans.iter().map(|&(x, ..)| source.get(x, y)));
        }
        return Bitmap {
            width,
            height,
            pixels,
        };
    }
    for &(y0, ya, yb) in &down.spans {
        for &(x0, xa, xb) in &across.spans {
            // Color weighted by coverage, so clear pixels don't darken their neighbors.
            let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for (dy, wy) in down.weights[ya as usize..yb as usize].iter().enumerate() {
                let row = ((y0 + dy as u32) * source.width + x0) as usize;
                for (dx, wx) in across.weights[xa as usize..xb as usize].iter().enumerate() {
                    let p = source.pixels[row + dx];
                    let w = wx * wy * f32::from(p[3]);
                    r += w * f32::from(p[0]);
                    g += w * f32::from(p[1]);
                    b += w * f32::from(p[2]);
                    a += w;
                }
            }
            pixels.push(if a <= 0.0 {
                [0; 4]
            } else {
                let byte = |v: f32| (v + 0.5).clamp(0.0, 255.0) as u8;
                [byte(r / a), byte(g / a), byte(b / a), byte(a)]
            });
        }
    }
    Bitmap {
        width,
        height,
        pixels,
    }
}

/// `source` turned by quarter turns to the right (clockwise).
pub(crate) fn turned(source: Bitmap, quarters: u8) -> Bitmap {
    let (w, h) = (source.width, source.height);
    let (width, height) = if quarters % 2 == 1 { (h, w) } else { (w, h) };
    let from = |x: u32, y: u32| match quarters % 4 {
        1 => source.get(y, h - 1 - x),
        2 => source.get(w - 1 - x, h - 1 - y),
        3 => source.get(w - 1 - y, x),
        _ => source.get(x, y),
    };
    if quarters.is_multiple_of(4) {
        return source;
    }
    let pixels = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| from(x, y))
        .collect();
    Bitmap {
        width,
        height,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bitmap(width: u32, rows: &[&[[u8; 4]]]) -> Bitmap {
        Bitmap {
            width,
            height: rows.len() as u32,
            pixels: rows.iter().flat_map(|r| r.iter().copied()).collect(),
        }
    }

    const R: [u8; 4] = [255, 0, 0, 255];
    const G: [u8; 4] = [0, 255, 0, 255];
    const B: [u8; 4] = [0, 0, 255, 255];
    const K: [u8; 4] = [0, 0, 0, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    #[test]
    fn shrinking_averages_the_area_each_cell_covers() {
        // 4 × 2 into 2 × 1: each cell is the average of a 2 × 2 block.
        let src = bitmap(
            4,
            &[&[R, R, K, [100, 100, 100, 255]], &[R, K, K, [100, 100, 100, 255]]],
        );
        let out = resize(&src, 2, 1, false);
        assert_eq!(out.pixels, vec![[191, 0, 0, 255], [50, 50, 50, 255]]);
        // A one-pixel line survives a shrink it would fall between with nearest-pixel sampling.
        let mut line = vec![K; 9];
        line[4] = [255, 255, 255, 255];
        let src = bitmap(9, &[&line]);
        assert_eq!(resize(&src, 3, 1, false).pixels[1], [85, 85, 85, 255]);
        assert_eq!(resize(&src, 3, 1, true).pixels[1], [255, 255, 255, 255]);
        let mut off = vec![K; 9];
        off[3] = [255, 255, 255, 255];
        let src = bitmap(9, &[&off]);
        assert_eq!(resize(&src, 3, 1, true).pixels, vec![K; 3], "crisp drops it");
        assert_eq!(resize(&src, 3, 1, false).pixels[1], [85, 85, 85, 255]);
        // An uneven shrink shares the pixels on a boundary: 3 into 2 gives 1.5 pixels each.
        let src = bitmap(3, &[&[[90, 0, 0, 255], [0, 0, 0, 255], [0, 0, 90, 255]]]);
        assert_eq!(
            resize(&src, 2, 1, false).pixels,
            vec![[60, 0, 0, 255], [0, 0, 60, 255]]
        );
    }

    #[test]
    fn enlarging_blends_or_keeps_pixels_crisp() {
        let src = bitmap(2, &[&[K, [200, 200, 200, 255]]]);
        assert_eq!(
            resize(&src, 4, 1, true).pixels,
            vec![K, K, [200, 200, 200, 255], [200, 200, 200, 255]]
        );
        assert_eq!(
            resize(&src, 4, 1, false).pixels,
            vec![K, [50, 50, 50, 255], [150, 150, 150, 255], [200, 200, 200, 255]]
        );
        // The same size is a copy either way.
        assert_eq!(resize(&src, 2, 1, false), src);
        assert_eq!(resize(&src, 2, 1, true), src);
    }

    #[test]
    fn clear_pixels_add_no_color_and_black_can_count_as_clear() {
        // Half clear, half red: red at half coverage, not dark red.
        let src = bitmap(2, &[&[R, CLEAR]]);
        assert_eq!(resize(&src, 1, 1, false).pixels, vec![[255, 0, 0, 128]]);
        // Black made clear first: the same. Left black, it darkens the green instead.
        let src = bitmap(2, &[&[G, K]]);
        assert_eq!(resize(&src, 1, 1, false).pixels, vec![[0, 128, 0, 255]]);
        let mut cleared = src.clone();
        clear_black(&mut cleared, 0);
        assert_eq!(cleared.pixels, vec![G, CLEAR]);
        assert_eq!(resize(&cleared, 1, 1, false).pixels, vec![[0, 255, 0, 128]]);
        // Nearly black counts once the level allows it.
        let dim = bitmap(2, &[&[[4, 4, 4, 255], B]]);
        let mut strict = dim.clone();
        clear_black(&mut strict, 0);
        assert_eq!(strict, dim);
        let mut loose = dim.clone();
        clear_black(&mut loose, 12);
        assert_eq!(loose.pixels, vec![CLEAR, B]);
    }

    #[test]
    fn quarter_turns_go_clockwise() {
        // R G
        // B K
        let src = bitmap(2, &[&[R, G], &[B, K]]);
        assert_eq!(turned(src.clone(), 0), src);
        assert_eq!(turned(src.clone(), 1), bitmap(2, &[&[B, R], &[K, G]]));
        assert_eq!(turned(src.clone(), 2), bitmap(2, &[&[K, B], &[G, R]]));
        assert_eq!(turned(src.clone(), 3), bitmap(2, &[&[G, K], &[R, B]]));
        // A wide picture stands up.
        let wide = bitmap(3, &[&[R, G, B]]);
        let up = turned(wide, 1);
        assert_eq!((up.width, up.height), (1, 3));
        assert_eq!(up.pixels, vec![R, G, B]);
    }
}
