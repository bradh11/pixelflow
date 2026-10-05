//! Colors as the renderer mixes them: floating point, 0.0–1.0 per channel.

use pf_sequence::{Blend, MAX_PALETTE_COLORS, Rgb};

/// A color with coverage: `a` is how much of the pixel the effect covers (0 = not at all).
/// `r`, `g`, `b` are the color itself, not yet multiplied by `a`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    /// Nothing drawn: the layers below show through.
    pub const CLEAR: Rgba = Rgba::new(0.0, 0.0, 0.0, 0.0);
    pub const BLACK: Rgba = Rgba::new(0.0, 0.0, 0.0, 1.0);

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn opaque(c: [f32; 3]) -> Self {
        Self::new(c[0], c[1], c[2], 1.0)
    }

    pub fn with_alpha(c: [f32; 3], a: f32) -> Self {
        Self::new(c[0], c[1], c[2], a)
    }

    /// The 8-bit color this shows over black.
    pub fn to_rgb8(self) -> [u8; 3] {
        let a = self.a.clamp(0.0, 1.0);
        [to_u8(self.r * a), to_u8(self.g * a), to_u8(self.b * a)]
    }
}

/// Clamps to 0.0–1.0, with NaN becoming 0 (`clamp` would keep NaN, hence `max` then `min`).
#[inline]
#[allow(clippy::manual_clamp)]
pub(crate) fn unit(v: f32) -> f32 {
    v.max(0.0).min(1.0)
}

pub(crate) fn to_u8(v: f32) -> u8 {
    (unit(v) * 255.0 + 0.5) as u8
}

/// A pixel being built up, with color already multiplied by coverage (premultiplied).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Acc {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Acc {
    pub const ZERO: Acc = Acc {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    /// Mixes an effect's color (already faded by `fade`) onto this pixel.
    #[inline]
    pub fn blend(&mut self, src: Rgba, mode: Blend, fade: f32) {
        // `unit` also turns NaN into 0, so bad numbers never spread.
        let a = unit(src.a * fade);
        if a <= 0.0 {
            return;
        }
        let (r, g, b) = (unit(src.r), unit(src.g), unit(src.b));
        match mode {
            Blend::Normal => {
                let keep = 1.0 - a;
                self.r = r * a + self.r * keep;
                self.g = g * a + self.g * keep;
                self.b = b * a + self.b * keep;
                self.a = a + self.a * keep;
            }
            Blend::Add => {
                self.r = (self.r + r * a).min(1.0);
                self.g = (self.g + g * a).min(1.0);
                self.b = (self.b + b * a).min(1.0);
                self.a = (self.a + a).min(1.0);
            }
            Blend::Max => {
                self.r = self.r.max(r * a);
                self.g = self.g.max(g * a);
                self.b = self.b.max(b * a);
                self.a = self.a.max(a);
            }
            Blend::Multiply => {
                let keep = 1.0 - a;
                self.r *= keep + r * a;
                self.g *= keep + g * a;
                self.b *= keep + b * a;
            }
        }
    }

    /// Draws `top` (a whole row's result) over this pixel.
    #[inline]
    pub fn cover_with(&mut self, top: Acc) {
        let keep = 1.0 - top.a;
        self.r = top.r + self.r * keep;
        self.g = top.g + self.g * keep;
        self.b = top.b + self.b * keep;
        self.a = top.a + self.a * keep;
    }
}

/// An effect's palette, ready for mixing. An empty palette is white.
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    list: [[f32; 3]; MAX_PALETTE_COLORS],
    len: usize,
}

impl Colors {
    pub fn new(palette: &[Rgb]) -> Self {
        let mut list = [[1.0; 3]; MAX_PALETTE_COLORS];
        let mut len = 0;
        for (slot, c) in list.iter_mut().zip(palette) {
            *slot = [
                f32::from(c.r) / 255.0,
                f32::from(c.g) / 255.0,
                f32::from(c.b) / 255.0,
            ];
            len += 1;
        }
        Self {
            list,
            len: len.max(1),
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    /// Color `k`, wrapping around the palette.
    #[inline]
    pub fn get(&self, k: u64) -> [f32; 3] {
        self.list[(k % self.len as u64) as usize]
    }

    /// The palette as a smooth ramp: 0.0 = first color, 1.0 = last.
    #[inline]
    pub fn ramp(&self, x: f32) -> [f32; 3] {
        if self.len == 1 {
            return self.list[0];
        }
        let pos = unit(x) * (self.len - 1) as f32;
        let i = (pos as usize).min(self.len - 2);
        let f = pos - i as f32;
        let (a, b) = (self.list[i], self.list[i + 1]);
        [
            a[0] + (b[0] - a[0]) * f,
            a[1] + (b[1] - a[1]) * f,
            a[2] + (b[2] - a[2]) * f,
        ]
    }

    /// Back and forth along the ramp: 0 → first, 1 → last, 2 → first again, and so on.
    #[inline]
    pub fn ping_pong(&self, x: f32) -> [f32; 3] {
        let phase = if x.is_finite() { x.rem_euclid(2.0) } else { 0.0 };
        self.ramp(if phase > 1.0 { 2.0 - phase } else { phase })
    }
}

/// Writes an 8-bit color into a show-frame pixel (canonical RGB or RGBW; white stays off).
#[inline]
pub(crate) fn write_pixel(pixel: &mut [u8], rgb: [u8; 3]) {
    pixel[0] = rgb[0];
    pixel[1] = rgb[1];
    pixel[2] = rgb[2];
    if let Some(w) = pixel.get_mut(3) {
        *w = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn ramps_interpolate_and_ping_pong_returns() {
        let c = Colors::new(&[Rgb::RED, Rgb::BLUE]);
        assert!(close(c.ramp(0.0), [1.0, 0.0, 0.0]));
        assert!(close(c.ramp(0.5), [0.5, 0.0, 0.5]));
        assert!(close(c.ramp(1.0), [0.0, 0.0, 1.0]));
        assert!(close(c.ping_pong(1.5), [0.5, 0.0, 0.5]));
        assert!(close(c.ping_pong(2.0), [1.0, 0.0, 0.0]));
        let three = Colors::new(&[Rgb::RED, Rgb::GREEN, Rgb::BLUE]);
        assert!(close(three.ramp(0.5), [0.0, 1.0, 0.0]));
        assert!(close(three.ramp(0.75), [0.0, 0.5, 0.5]));
        assert!(close(three.get(4), [0.0, 1.0, 0.0]));
        assert!(
            close(Colors::new(&[]).get(3), [1.0, 1.0, 1.0]),
            "empty palettes are white"
        );
        assert!(close(c.ramp(f32::NAN), [1.0, 0.0, 0.0]));
        assert!(close(c.ping_pong(f32::INFINITY), [1.0, 0.0, 0.0]));
    }

    #[test]
    fn blend_modes() {
        let red = Rgba::opaque([1.0, 0.0, 0.0]);
        let half_blue = Rgba::with_alpha([0.0, 0.0, 1.0], 0.5);
        let mut p = Acc::ZERO;
        p.blend(red, Blend::Normal, 1.0);
        p.blend(half_blue, Blend::Normal, 1.0);
        assert_eq!((p.r, p.g, p.b, p.a), (0.5, 0.0, 0.5, 1.0));

        let mut p = Acc::ZERO;
        p.blend(Rgba::opaque([0.6, 0.2, 0.0]), Blend::Normal, 1.0);
        p.blend(Rgba::opaque([0.6, 0.2, 0.0]), Blend::Add, 1.0);
        assert!((p.r - 1.0).abs() < 1e-6 && (p.g - 0.4).abs() < 1e-6);

        let mut p = Acc::ZERO;
        p.blend(Rgba::opaque([0.2, 0.8, 0.0]), Blend::Normal, 1.0);
        p.blend(Rgba::opaque([0.6, 0.1, 0.0]), Blend::Max, 1.0);
        assert_eq!((p.r, p.g), (0.6, 0.8));

        let mut p = Acc::ZERO;
        p.blend(Rgba::opaque([1.0, 1.0, 1.0]), Blend::Normal, 1.0);
        p.blend(Rgba::opaque([1.0, 0.5, 0.0]), Blend::Multiply, 1.0);
        assert_eq!((p.r, p.g, p.b), (1.0, 0.5, 0.0));

        // Fades scale coverage; a fully faded effect leaves the pixel alone.
        let mut p = Acc::ZERO;
        p.blend(red, Blend::Normal, 0.25);
        assert_eq!((p.r, p.a), (0.25, 0.25));
        p.blend(Rgba::BLACK, Blend::Normal, 0.0);
        assert_eq!((p.r, p.a), (0.25, 0.25));
        p.blend(Rgba::new(f32::NAN, 0.0, 0.0, f32::NAN), Blend::Normal, 1.0);
        assert_eq!((p.r, p.a), (0.25, 0.25), "garbage is ignored");
    }

    #[test]
    fn rows_cover_what_is_below_by_their_coverage() {
        let mut below = Acc {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        below.cover_with(Acc {
            r: 0.0,
            g: 0.5,
            b: 0.0,
            a: 0.5,
        });
        assert_eq!((below.r, below.g, below.a), (0.5, 0.5, 1.0));
        assert_eq!(Rgba::with_alpha([1.0, 0.5, 0.0], 0.5).to_rgb8(), [128, 64, 0]);
    }
}
