//! Resizing a picture's frame: every output cell is a weighted mix of the input pixels it covers.
//!
//! Shrinking averages the whole area a cell covers, so a line a pixel wide in a large picture
//! still shows on a small matrix; enlarging blends between neighbors. Crisp takes the nearest
//! pixel either way, which keeps pixel art sharp. Clear pixels add no color to the mix, so a
//! picture's edges don't darken.
//!
//! Two more ways are xLights' own from before it changed how it resizes (see [`Resize`]), for
//! sequences imported from those versions: they pick the pixels its `wxImage::Scale` picked.

/// How a frame is resized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum Resize {
    /// Averaged over the area each cell covers when shrinking, blended between the nearest
    /// pixels when enlarging.
    #[default]
    Smooth,
    /// The pixel nearest each cell's middle.
    Crisp,
    /// As xLights before 2024.16 (`wxImage::Scale` at its normal quality, then
    /// `ResampleNearest`): the pixel at each cell's top left corner, counted in 16.16 fixed
    /// point; a shrink by a whole number both ways averages each block instead (`ShrinkBy`).
    Corner,
    /// As xLights 2024.16 to 2025.05: shrinking (neither side enlarged) mixes the four pixels
    /// round each cell's place, the first and last cells on the picture's first and last pixels
    /// (`ResampleBilinear`); otherwise the corner pixel, as [`Resize::Corner`].
    Mixed,
}

impl Resize {
    /// Whether the pixels are picked the way an xLights version picked them, which needs the
    /// picture at its own size to pick them from.
    pub fn needs_full_size(self) -> bool {
        matches!(self, Resize::Corner | Resize::Mixed)
    }

    /// How frames are shrunk as they're read to save memory.
    pub fn on_reading(self) -> Resize {
        match self {
            Resize::Crisp => Resize::Crisp,
            _ => Resize::Smooth,
        }
    }
}

/// Which input pixels an output cell takes along one axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sample {
    /// Every pixel the cell covers (shrinking), or the two nearest its middle (enlarging).
    Area,
    /// The one at the cell's middle.
    Middle,
    /// The one at the cell's first edge.
    Corner,
    /// The two round the cell's place when the first and last cells sit on the first and last
    /// pixels.
    Ends,
}

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
    fn new(from: u32, to: u32, sample: Sample) -> Self {
        let mut axis = Axis {
            spans: Vec::with_capacity(to as usize),
            weights: Vec::new(),
        };
        let step = f64::from(from) / f64::from(to);
        let last = from - 1;
        // wxWidgets' steps: a 16.16 fixed-point stride from the corner, and the stride that puts
        // the last cell on the last pixel (one cell alone takes the picture's middle).
        let stride = (u64::from(from) << 16) / u64::from(to);
        let between = if to > 1 {
            f64::from(last) / f64::from(to - 1)
        } else {
            0.0
        };
        for i in 0..to {
            let start = axis.weights.len() as u32;
            let first = if from == to {
                axis.weights.push(1.0);
                i
            } else if sample == Sample::Middle {
                axis.weights.push(1.0);
                (((f64::from(i) + 0.5) * step) as u32).min(last)
            } else if sample == Sample::Corner {
                axis.weights.push(1.0);
                (((u64::from(i) * stride) >> 16) as u32).min(last)
            } else if sample == Sample::Ends {
                let at = if to > 1 {
                    f64::from(i) * between
                } else {
                    f64::from(last) / 2.0
                };
                let first = (at as u32).min(last);
                let part = (at - f64::from(first)) as f32;
                if first == last {
                    axis.weights.push(1.0);
                } else {
                    axis.weights.extend([1.0 - part, part]);
                }
                first
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
pub(crate) fn resize(source: &Bitmap, width: u32, height: u32, how: Resize) -> Bitmap {
    let (width, height) = (width.max(1), height.max(1));
    if source.width == 0 || source.height == 0 {
        return Bitmap {
            width,
            height,
            pixels: vec![[0; 4]; (width * height) as usize],
        };
    }
    let shrinks = width <= source.width && height <= source.height;
    let by_whole_numbers =
        shrinks && source.width.is_multiple_of(width) && source.height.is_multiple_of(height);
    // `straight`: color and coverage are each mixed on their own, as wxWidgets mixes them.
    let (sample, straight) = match how {
        Resize::Smooth => (Sample::Area, false),
        Resize::Crisp => (Sample::Middle, false),
        Resize::Corner if by_whole_numbers => (Sample::Area, false),
        Resize::Mixed if shrinks => (Sample::Ends, true),
        Resize::Corner | Resize::Mixed => (Sample::Corner, false),
    };
    let (across, down) = (
        Axis::new(source.width, width, sample),
        Axis::new(source.height, height, sample),
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
    if straight {
        for &(y0, ya, yb) in &down.spans {
            for &(x0, xa, xb) in &across.spans {
                let mut mixed = [0.0f64; 4];
                for (dy, wy) in down.weights[ya as usize..yb as usize].iter().enumerate() {
                    let row = ((y0 + dy as u32) * source.width + x0) as usize;
                    for (dx, wx) in across.weights[xa as usize..xb as usize].iter().enumerate() {
                        let w = f64::from(*wx) * f64::from(*wy);
                        for (sum, channel) in mixed.iter_mut().zip(source.pixels[row + dx]) {
                            *sum += w * f64::from(channel);
                        }
                    }
                }
                pixels.push(mixed.map(|v| (v + 0.5).clamp(0.0, 255.0) as u8));
            }
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
        let out = resize(&src, 2, 1, Resize::Smooth);
        assert_eq!(out.pixels, vec![[191, 0, 0, 255], [50, 50, 50, 255]]);
        // A one-pixel line survives a shrink it would fall between with nearest-pixel sampling.
        let mut line = vec![K; 9];
        line[4] = [255, 255, 255, 255];
        let src = bitmap(9, &[&line]);
        assert_eq!(resize(&src, 3, 1, Resize::Smooth).pixels[1], [85, 85, 85, 255]);
        assert_eq!(resize(&src, 3, 1, Resize::Crisp).pixels[1], [255, 255, 255, 255]);
        let mut off = vec![K; 9];
        off[3] = [255, 255, 255, 255];
        let src = bitmap(9, &[&off]);
        assert_eq!(
            resize(&src, 3, 1, Resize::Crisp).pixels,
            vec![K; 3],
            "crisp drops it"
        );
        assert_eq!(resize(&src, 3, 1, Resize::Smooth).pixels[1], [85, 85, 85, 255]);
        // An uneven shrink shares the pixels on a boundary: 3 into 2 gives 1.5 pixels each.
        let src = bitmap(3, &[&[[90, 0, 0, 255], [0, 0, 0, 255], [0, 0, 90, 255]]]);
        assert_eq!(
            resize(&src, 2, 1, Resize::Smooth).pixels,
            vec![[60, 0, 0, 255], [0, 0, 60, 255]]
        );
    }

    /// A row of greys, a pixel for each level given.
    fn greys(levels: &[u8]) -> Bitmap {
        let row: Vec<[u8; 4]> = levels.iter().map(|&v| [v, v, v, 255]).collect();
        bitmap(levels.len() as u32, &[&row])
    }

    fn levels(b: &Bitmap) -> Vec<u8> {
        b.pixels.iter().map(|p| p[0]).collect()
    }

    #[test]
    fn xlights_before_2024_16_takes_each_cells_corner_pixel() {
        // 5 into 3: a stride of (5 << 16) / 3 in fixed point lands on pixels 0, 1, and 3
        // (the middles would be 0, 2, and 4).
        let src = greys(&[10, 20, 30, 40, 50]);
        assert_eq!(levels(&resize(&src, 3, 1, Resize::Corner)), [10, 20, 40]);
        assert_eq!(levels(&resize(&src, 3, 1, Resize::Crisp)), [10, 30, 50]);
        // Enlarging 2 to 5: the stride of 0.4 gives pixels 0, 0, 0, 1, 1.
        let src = greys(&[10, 20]);
        assert_eq!(levels(&resize(&src, 5, 1, Resize::Corner)), [10, 10, 10, 20, 20]);
        assert_eq!(levels(&resize(&src, 5, 1, Resize::Crisp)), [10, 10, 20, 20, 20]);
        // A shrink by a whole number both ways averages each block, as its `ShrinkBy` does.
        let src = greys(&[10, 30, 50, 90]);
        assert_eq!(levels(&resize(&src, 2, 1, Resize::Corner)), [20, 70]);
        // The same size is a copy.
        assert_eq!(resize(&src, 4, 1, Resize::Corner), src);
    }

    #[test]
    fn xlights_from_2024_16_mixes_the_pixels_round_each_cell_when_shrinking() {
        // 5 into 3: the ends on the ends, so the cells sit on pixels 0, 2, and 4 exactly.
        let src = greys(&[10, 20, 30, 40, 50]);
        assert_eq!(levels(&resize(&src, 3, 1, Resize::Mixed)), [10, 30, 50]);
        // 4 into 3: at 0, 1.5, and 3; the middle cell is half of pixels 1 and 2.
        let src = greys(&[10, 20, 40, 80]);
        assert_eq!(levels(&resize(&src, 3, 1, Resize::Mixed)), [10, 30, 80]);
        // A long way down it reads only the pixels round each place: no averaging of the rest.
        let mut wide = vec![0u8; 101];
        wide[50] = 200;
        assert_eq!(levels(&resize(&greys(&wide), 3, 1, Resize::Mixed)), [0, 200, 0]);
        wide[50] = 0;
        wide[49] = 200;
        assert_eq!(levels(&resize(&greys(&wide), 3, 1, Resize::Mixed)), [0, 0, 0]);
        // Down to one cell: the picture's middle (between pixels 1 and 2 of 4).
        assert_eq!(levels(&resize(&src, 1, 1, Resize::Mixed)), [30]);
        // Color and coverage are mixed each on their own: half red, half clear is dark red
        // half covered.
        let src = bitmap(4, &[&[R, R, CLEAR, CLEAR]]);
        assert_eq!(
            resize(&src, 3, 1, Resize::Mixed).pixels,
            vec![R, [128, 0, 0, 128], CLEAR]
        );
        // Both ways at once: 4 × 4 into 3 × 3 mixes four pixels at the middle cell.
        let rows: Vec<Vec<[u8; 4]>> = (0..4u8)
            .map(|y| (0..4u8).map(|x| [x * 40, y * 40, 0, 255]).collect())
            .collect();
        let refs: Vec<&[[u8; 4]]> = rows.iter().map(Vec::as_slice).collect();
        let out = resize(&bitmap(4, &refs), 3, 3, Resize::Mixed);
        assert_eq!(out.get(1, 1), [60, 60, 0, 255]);
        assert_eq!(out.get(2, 0), [120, 0, 0, 255]);
    }

    #[test]
    fn xlights_from_2024_16_still_takes_the_corner_pixel_when_either_side_grows() {
        let src = greys(&[10, 20]);
        assert_eq!(levels(&resize(&src, 5, 1, Resize::Mixed)), [10, 10, 10, 20, 20]);
        // Wider but shorter: the whole picture is resized the one way.
        let tall = bitmap(2, &[&[R, G], &[B, K], &[G, R], &[K, B]]);
        let out = resize(&tall, 4, 2, Resize::Mixed);
        assert_eq!(out.pixels, vec![R, R, G, G, G, G, R, R]);
    }

    #[test]
    fn enlarging_blends_or_keeps_pixels_crisp() {
        let src = bitmap(2, &[&[K, [200, 200, 200, 255]]]);
        assert_eq!(
            resize(&src, 4, 1, Resize::Crisp).pixels,
            vec![K, K, [200, 200, 200, 255], [200, 200, 200, 255]]
        );
        assert_eq!(
            resize(&src, 4, 1, Resize::Smooth).pixels,
            vec![K, [50, 50, 50, 255], [150, 150, 150, 255], [200, 200, 200, 255]]
        );
        // The same size is a copy either way.
        assert_eq!(resize(&src, 2, 1, Resize::Smooth), src);
        assert_eq!(resize(&src, 2, 1, Resize::Crisp), src);
    }

    #[test]
    fn clear_pixels_add_no_color_and_black_can_count_as_clear() {
        // Half clear, half red: red at half coverage, not dark red.
        let src = bitmap(2, &[&[R, CLEAR]]);
        assert_eq!(resize(&src, 1, 1, Resize::Smooth).pixels, vec![[255, 0, 0, 128]]);
        // Black made clear first: the same. Left black, it darkens the green instead.
        let src = bitmap(2, &[&[G, K]]);
        assert_eq!(resize(&src, 1, 1, Resize::Smooth).pixels, vec![[0, 128, 0, 255]]);
        let mut cleared = src.clone();
        clear_black(&mut cleared, 0);
        assert_eq!(cleared.pixels, vec![G, CLEAR]);
        assert_eq!(
            resize(&cleared, 1, 1, Resize::Smooth).pixels,
            vec![[0, 255, 0, 128]]
        );
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
