//! Pictures as video encoders take them: YUV 4:2:0 (I420) in BT.709's limited range, the
//! standard for HD video. Each 2×2 block of pixels shares one color sample.

use crate::raster::Canvas;

/// One picture in I420: a full-size Y plane, then quarter-size U and V planes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Yuv {
    pub width: usize,
    pub height: usize,
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
}

/// BT.709 in 16.16 fixed point: luma (scaled to 16–235) and the two color differences (scaled
/// to 16–240), from 0–255 RGB.
const Y: [i32; 3] = [11966, 40254, 4064];
const U: [i32; 3] = [-6596, -22188, 28784];
const V: [i32; 3] = [28784, -26145, -2639];
const HALF: i32 = 1 << 15;

fn dot(k: [i32; 3], rgb: [i32; 3]) -> i32 {
    k[0] * rgb[0] + k[1] * rgb[1] + k[2] * rgb[2]
}

impl Yuv {
    /// `canvas` (even width and height) converted, its colors clamped to 0–255.
    pub fn from_canvas(canvas: &Canvas) -> Self {
        let (w, h) = (canvas.width as usize, canvas.height as usize);
        let mut out = Yuv {
            width: w,
            height: h,
            y: vec![0; w * h],
            u: vec![0; w.div_ceil(2) * h.div_ceil(2)],
            v: vec![0; w.div_ceil(2) * h.div_ceil(2)],
        };
        out.fill(canvas);
        out
    }

    /// Converts `canvas` (the same size) into this picture.
    pub fn fill(&mut self, canvas: &Canvas) {
        let (w, h) = (self.width, self.height);
        let cw = w.div_ceil(2);
        let rgb = |x: usize, y: usize| {
            let at = (y * w + x) * 3;
            let c = &canvas.rgb[at..at + 3];
            [0, 1, 2].map(|i| i32::from(c[i].min(255)))
        };
        for by in (0..h).step_by(2) {
            for bx in (0..w).step_by(2) {
                let mut sum = [0i32; 3];
                let mut n = 0;
                for (x, y) in [(bx, by), (bx + 1, by), (bx, by + 1), (bx + 1, by + 1)] {
                    if x >= w || y >= h {
                        continue;
                    }
                    let c = rgb(x, y);
                    self.y[y * w + x] = (16 + ((dot(Y, c) + HALF) >> 16)) as u8;
                    for i in 0..3 {
                        sum[i] += c[i];
                    }
                    n += 1;
                }
                let avg = sum.map(|s| (s + n / 2) / n);
                let at = (by / 2) * cw + bx / 2;
                self.u[at] = (128 + ((dot(U, avg) + HALF) >> 16)).clamp(0, 255) as u8;
                self.v[at] = (128 + ((dot(V, avg) + HALF) >> 16)).clamp(0, 255) as u8;
            }
        }
    }

    /// The planes one after another, as raw `yuv420p` video.
    pub fn planes(&self) -> [&[u8]; 3] {
        [&self.y, &self.u, &self.v]
    }
}

impl openh264::formats::YUVSource for Yuv {
    fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    fn strides(&self) -> (usize, usize, usize) {
        let c = self.width.div_ceil(2);
        (self.width, c, c)
    }

    fn y(&self) -> &[u8] {
        &self.y
    }

    fn u(&self) -> &[u8] {
        &self.u
    }

    fn v(&self) -> &[u8] {
        &self.v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(w: u32, h: u32, color: [u16; 3]) -> Canvas {
        let mut c = Canvas::new(w, h);
        for px in c.rgb.as_chunks_mut::<3>().0 {
            px.copy_from_slice(&color);
        }
        c
    }

    #[test]
    fn known_colors_in_bt709_limited_range() {
        let cases = [
            ([0, 0, 0], (16, 128, 128)),
            ([255, 255, 255], (235, 128, 128)),
            ([255, 0, 0], (63, 102, 240)),
            ([0, 255, 0], (173, 42, 26)),
            ([0, 0, 255], (32, 240, 118)),
            ([400, 300, 999], (235, 128, 128)),
        ];
        for (rgb, (y, u, v)) in cases {
            let yuv = Yuv::from_canvas(&canvas(4, 2, rgb));
            assert_eq!((yuv.y[0], yuv.u[0], yuv.v[0]), (y, u, v), "{rgb:?}");
            assert_eq!(yuv.y.len(), 8);
            assert_eq!(yuv.u.len(), 2);
        }
    }

    #[test]
    fn color_is_shared_by_each_two_by_two_block() {
        let mut c = canvas(4, 2, [0, 0, 0]);
        // The left block: two white pixels and two black: luma per pixel, color averaged.
        c.rgb[..3].copy_from_slice(&[255, 255, 255]);
        c.rgb[3..6].copy_from_slice(&[255, 255, 255]);
        let yuv = Yuv::from_canvas(&c);
        assert_eq!(&yuv.y[..4], &[235, 235, 16, 16]);
        assert_eq!(&yuv.y[4..], &[16, 16, 16, 16]);
        assert_eq!((yuv.u[0], yuv.v[0]), (128, 128));
    }
}
