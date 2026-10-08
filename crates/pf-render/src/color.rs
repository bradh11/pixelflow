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

/// Darker than this shows as black on an 8-bit pixel (where xLights' layer methods test for it).
pub(crate) const BLACK_LEVEL: f32 = 0.5 / 255.0;

/// Brighter than black on an 8-bit pixel.
#[inline]
pub(crate) fn lit(rgb: [f32; 3]) -> bool {
    rgb[0].max(rgb[1]).max(rgb[2]) >= BLACK_LEVEL
}

/// Hue, saturation, and value (each 0–1), worked out as xLights' `toHSV` does.
pub(crate) fn to_hsv([mut r, mut g, mut b]: [f32; 3]) -> [f32; 3] {
    let mut k = 0.0f32;
    if g < b {
        std::mem::swap(&mut g, &mut b);
        k = -1.0;
    }
    let mut min_gb = b;
    if r < g {
        std::mem::swap(&mut r, &mut g);
        k = -2.0 / 6.0 - k;
        min_gb = g.min(b);
    }
    let chroma = r - min_gb;
    [
        (k + (g - b) / (6.0 * chroma + 1e-20)).abs(),
        chroma / (r + 1e-20),
        r,
    ]
}

/// A color from hue, saturation, and value, as xLights' `fromHSV` makes it: a hue of 1 or more
/// counts as the last sector, and one below 0 gives grey at the lowest level.
pub(crate) fn from_hsv([h, s, v]: [f32; 3]) -> [f32; 3] {
    if s == 0.0 {
        return [v, v, v];
    }
    let hue = h * 6.0;
    let i = hue.floor();
    let f = hue - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i.min(5.0) as i32 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        5 => [v, p, q],
        _ => [p, p, p],
    }
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

    /// Mixes an effect's color (already faded by `fade`) onto this pixel. `first_half` says
    /// whether the pixel is on the bottom half (for [`Blend::BottomHalf`]) or the left half (for
    /// [`Blend::LeftHalf`]) of its target; other blends ignore it.
    ///
    /// The xLights layer methods (every blend but Normal, Add, Max, and Multiply, which keep
    /// PixelFlow's coverage rules) follow xLights' `LayerBlendingFunctions.ispc`, on the colors as
    /// they show: this effect's color times its coverage and fade (xLights fades those layers by
    /// brightness), and the layers below as built so far. Where the result is this effect's light,
    /// it covers as much as this effect or the layers below did; where it's what's below, or
    /// black, it covers as much as the layers below did.
    #[inline]
    pub fn blend(&mut self, src: Rgba, mode: Blend, fade: f32, first_half: bool) {
        // `unit` also turns NaN into 0, so bad numbers never spread.
        let a = unit(src.a * fade);
        let (r, g, b) = (unit(src.r), unit(src.g), unit(src.b));
        match mode {
            Blend::Normal => {
                if a > 0.0 {
                    let keep = 1.0 - a;
                    self.r = r * a + self.r * keep;
                    self.g = g * a + self.g * keep;
                    self.b = b * a + self.b * keep;
                    self.a = a + self.a * keep;
                }
                return;
            }
            Blend::Add => {
                if a > 0.0 {
                    self.r = (self.r + r * a).min(1.0);
                    self.g = (self.g + g * a).min(1.0);
                    self.b = (self.b + b * a).min(1.0);
                    self.a = (self.a + a).min(1.0);
                }
                return;
            }
            Blend::Max => {
                // xLights also multiplies by the effect's alpha, which blacks out what's below
                // wherever the effect leaves a pixel clear; PixelFlow keeps what's below there.
                if a > 0.0 {
                    self.r = self.r.max(r * a);
                    self.g = self.g.max(g * a);
                    self.b = self.b.max(b * a);
                    self.a = self.a.max(a);
                }
                return;
            }
            Blend::Multiply => {
                if a > 0.0 {
                    let keep = 1.0 - a;
                    self.r *= keep + r * a;
                    self.g *= keep + g * a;
                    self.b *= keep + b * a;
                }
                return;
            }
            _ => {}
        }
        let fg = [r * a, g * a, b * a];
        let bg = [self.r, self.g, self.b];
        let (fg_lit, bg_lit) = (lit(fg), lit(bg));
        let below = self.a;
        // `own`: the result is this effect's light, so it covers at least as much as the effect.
        let mut show = |rgb: [f32; 3], own: bool| {
            [self.r, self.g, self.b] = rgb.map(unit);
            if own {
                self.a = below.max(a);
            }
        };
        const BLACK: [f32; 3] = [0.0; 3];
        match mode {
            Blend::Normal | Blend::Add | Blend::Max | Blend::Multiply => {}
            Blend::Subtract => show([bg[0] - fg[0], bg[1] - fg[1], bg[2] - fg[2]], false),
            Blend::Min => show([bg[0].min(fg[0]), bg[1].min(fg[1]), bg[2].min(fg[2])], false),
            Blend::Average => {
                if !bg_lit {
                    show(fg, true);
                } else if fg_lit {
                    show(
                        [
                            (fg[0] + bg[0]) / 2.0,
                            (fg[1] + bg[1]) / 2.0,
                            (fg[2] + bg[2]) / 2.0,
                        ],
                        true,
                    );
                }
            }
            Blend::Over => {
                if fg_lit {
                    show(fg, true);
                }
            }
            Blend::Behind => {
                if !bg_lit {
                    show(fg, true);
                }
            }
            Blend::Mask => {
                if fg_lit {
                    show(BLACK, false);
                }
            }
            Blend::Reveal => {
                if !fg_lit {
                    show(BLACK, false);
                }
            }
            Blend::RevealBrightness => {
                if fg_lit {
                    let mut hsv = to_hsv(bg);
                    hsv[2] = to_hsv(fg)[2];
                    show(from_hsv(hsv), true);
                } else {
                    show(BLACK, false);
                }
            }
            Blend::CutOut => {
                if bg_lit {
                    show(BLACK, false);
                } else {
                    show(fg, true);
                }
            }
            Blend::Clip => {
                if bg_lit {
                    show(fg, true);
                } else {
                    show(BLACK, false);
                }
            }
            Blend::ClipBrightness => {
                if bg_lit {
                    let mut hsv = to_hsv(fg);
                    hsv[2] = to_hsv(bg)[2];
                    show(from_hsv(hsv), true);
                } else {
                    show(BLACK, false);
                }
            }
            Blend::Shadow => {
                let (h0, mut h1) = (to_hsv(fg), to_hsv(bg));
                if h0[2] > 0.0 {
                    h1[0] += h0[2] * (h1[0] - h0[0]) / 5.0;
                }
                show(from_hsv(h1), false);
            }
            Blend::ShadowBelow => {
                let (mut h0, h1) = (to_hsv(fg), to_hsv(bg));
                if h1[2] > 0.0 {
                    h0[0] += h1[2] * (h0[0] - h1[0]) / 2.0;
                }
                show(from_hsv(h0), true);
            }
            Blend::Highlight => {
                if fg_lit && bg_lit {
                    show(fg, true);
                }
            }
            Blend::HighlightAdd => {
                if bg_lit {
                    show([fg[0] + bg[0], fg[1] + bg[1], fg[2] + bg[2]], true);
                }
            }
            Blend::BottomHalf | Blend::LeftHalf => {
                if first_half {
                    show(fg, true);
                }
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
        p.blend(red, Blend::Normal, 1.0, false);
        p.blend(half_blue, Blend::Normal, 1.0, false);
        assert_eq!((p.r, p.g, p.b, p.a), (0.5, 0.0, 0.5, 1.0));

        let mut p = Acc::ZERO;
        p.blend(Rgba::opaque([0.6, 0.2, 0.0]), Blend::Normal, 1.0, false);
        p.blend(Rgba::opaque([0.6, 0.2, 0.0]), Blend::Add, 1.0, false);
        assert!((p.r - 1.0).abs() < 1e-6 && (p.g - 0.4).abs() < 1e-6);

        let mut p = Acc::ZERO;
        p.blend(Rgba::opaque([0.2, 0.8, 0.0]), Blend::Normal, 1.0, false);
        p.blend(Rgba::opaque([0.6, 0.1, 0.0]), Blend::Max, 1.0, false);
        assert_eq!((p.r, p.g), (0.6, 0.8));

        let mut p = Acc::ZERO;
        p.blend(Rgba::opaque([1.0, 1.0, 1.0]), Blend::Normal, 1.0, false);
        p.blend(Rgba::opaque([1.0, 0.5, 0.0]), Blend::Multiply, 1.0, false);
        assert_eq!((p.r, p.g, p.b), (1.0, 0.5, 0.0));

        // Fades scale coverage; a fully faded effect leaves the pixel alone.
        let mut p = Acc::ZERO;
        p.blend(red, Blend::Normal, 0.25, false);
        assert_eq!((p.r, p.a), (0.25, 0.25));
        p.blend(Rgba::BLACK, Blend::Normal, 0.0, false);
        assert_eq!((p.r, p.a), (0.25, 0.25));
        p.blend(Rgba::new(f32::NAN, 0.0, 0.0, f32::NAN), Blend::Normal, 1.0, false);
        assert_eq!((p.r, p.a), (0.25, 0.25), "garbage is ignored");
    }

    /// `top` mixed by `mode` over a pixel lit `below` (opaque), or over nothing when `below` is
    /// `None`: the color it shows and its coverage.
    fn mix(below: Option<[f32; 3]>, top: Rgba, mode: Blend, first_half: bool) -> ([f32; 3], f32) {
        let mut p = Acc::ZERO;
        if let Some(c) = below {
            p.blend(Rgba::opaque(c), Blend::Normal, 1.0, false);
        }
        p.blend(top, mode, 1.0, first_half);
        ([p.r, p.g, p.b], p.a)
    }

    #[test]
    fn xlights_layer_methods() {
        const RED: [f32; 3] = [1.0, 0.0, 0.0];
        const BLUE: [f32; 3] = [0.0, 0.0, 1.0];
        const DARK: Option<[f32; 3]> = Some([0.0; 3]);
        let blue = Rgba::opaque(BLUE);
        let lit_below = Some(RED);
        // (blend, below, this effect, result)
        type Case = (Blend, Option<[f32; 3]>, Rgba, [f32; 3]);
        let cases: [Case; 36] = [
            // (blend, below, this effect, result)
            (
                Blend::Subtract,
                Some([1.0, 0.5, 0.2]),
                Rgba::opaque([0.5, 1.0, 0.0]),
                [0.5, 0.0, 0.2],
            ),
            (
                Blend::Min,
                Some([1.0, 0.5, 0.2]),
                Rgba::opaque([0.5, 1.0, 0.0]),
                [0.5, 0.5, 0.0],
            ),
            (Blend::Min, lit_below, Rgba::CLEAR, [0.0; 3]),
            (
                Blend::Average,
                Some([1.0, 0.5, 0.0]),
                Rgba::opaque([0.0, 0.5, 1.0]),
                [0.5, 0.5, 0.5],
            ),
            (Blend::Average, DARK, blue, BLUE),
            (Blend::Average, lit_below, Rgba::CLEAR, RED),
            // 1 reveals 2: this effect where it's lit, what's below elsewhere.
            (Blend::Over, lit_below, blue, BLUE),
            (Blend::Over, lit_below, Rgba::CLEAR, RED),
            // 2 reveals 1: what's below where it's lit; this effect only where that's dark.
            (Blend::Behind, lit_below, blue, RED),
            (Blend::Behind, DARK, blue, BLUE),
            (Blend::Behind, None, blue, BLUE),
            // 1 is Mask: black where this effect is lit.
            (Blend::Mask, lit_below, blue, [0.0; 3]),
            (Blend::Mask, lit_below, Rgba::CLEAR, RED),
            // 1 is True Unmask: what's below only where this effect is lit.
            (Blend::Reveal, lit_below, blue, RED),
            (Blend::Reveal, lit_below, Rgba::CLEAR, [0.0; 3]),
            // 1 is Unmask: what's below at this effect's brightness.
            (
                Blend::RevealBrightness,
                lit_below,
                Rgba::opaque([0.0, 0.0, 0.5]),
                [0.5, 0.0, 0.0],
            ),
            (Blend::RevealBrightness, lit_below, Rgba::CLEAR, [0.0; 3]),
            // ... and where nothing is below, black becomes white.
            (
                Blend::RevealBrightness,
                None,
                Rgba::opaque([0.0, 0.0, 0.5]),
                [0.5, 0.5, 0.5],
            ),
            // 2 is Mask: this effect where what's below is dark, black where it's lit.
            (Blend::CutOut, lit_below, blue, [0.0; 3]),
            (Blend::CutOut, DARK, blue, BLUE),
            // 2 is True Unmask: this effect only where what's below is lit.
            (Blend::Clip, lit_below, blue, BLUE),
            (Blend::Clip, DARK, blue, [0.0; 3]),
            (Blend::Clip, lit_below, Rgba::CLEAR, [0.0; 3]),
            // 2 is Unmask: this effect's colors at the brightness of what's below.
            (
                Blend::ClipBrightness,
                Some([0.5, 0.0, 0.0]),
                blue,
                [0.0, 0.0, 0.5],
            ),
            (
                Blend::ClipBrightness,
                Some([0.5, 0.0, 0.0]),
                Rgba::CLEAR,
                [0.5, 0.5, 0.5],
            ),
            (Blend::ClipBrightness, DARK, blue, [0.0; 3]),
            // Shadow 1 on 2: green (hue 1/3) below, blue (2/3) on top at full value: the hue moves
            // by (1/3 - 2/3) / 5 toward yellow.
            (Blend::Shadow, Some([0.0, 1.0, 0.0]), blue, [0.4, 1.0, 0.0]),
            (Blend::Shadow, lit_below, Rgba::CLEAR, RED),
            // Shadow 2 on 1: blue (2/3) on green (1/3): the hue moves by (2/3 - 1/3) / 2 to 5/6.
            (Blend::ShadowBelow, Some([0.0, 1.0, 0.0]), blue, [1.0, 0.0, 1.0]),
            (Blend::ShadowBelow, DARK, blue, BLUE),
            // Highlight: this effect only where both are lit.
            (Blend::Highlight, lit_below, blue, BLUE),
            (Blend::Highlight, DARK, blue, [0.0; 3]),
            // Highlight Vibrant: adds where what's below is lit.
            (Blend::HighlightAdd, lit_below, blue, [1.0, 0.0, 1.0]),
            (Blend::HighlightAdd, DARK, blue, [0.0; 3]),
            (Blend::BottomHalf, lit_below, blue, BLUE),
            (Blend::LeftHalf, lit_below, Rgba::CLEAR, [0.0; 3]),
        ];
        for (mode, below, top, want) in cases {
            let (got, _) = mix(below, top, mode, true);
            assert!(
                close(got, want),
                "{mode:?} on {below:?} with {top:?}: {got:?}, want {want:?}"
            );
        }
        // Off its half, a half blend leaves what's below.
        assert!(close(mix(lit_below, blue, Blend::BottomHalf, false).0, RED));
        assert!(close(mix(lit_below, blue, Blend::LeftHalf, false).0, RED));
    }

    #[test]
    fn xlights_methods_mix_the_colors_as_they_show_and_keep_coverage() {
        // A faded effect shows dimmer: it's judged lit or not, and mixed, as it shows.
        let mut p = Acc::ZERO;
        p.blend(Rgba::opaque([1.0, 0.0, 0.0]), Blend::Normal, 1.0, false);
        p.blend(Rgba::opaque([0.0, 0.0, 1.0]), Blend::HighlightAdd, 0.5, false);
        assert_eq!((p.r, p.b, p.a), (1.0, 0.5, 1.0));
        // Too dim to light an 8-bit pixel counts as dark.
        let mut p = Acc::ZERO;
        p.blend(Rgba::opaque([1.0, 0.0, 0.0]), Blend::Normal, 0.001, false);
        p.blend(Rgba::opaque([0.0, 0.0, 1.0]), Blend::Behind, 1.0, false);
        assert_eq!((p.r, p.b, p.a), (0.0, 1.0, 1.0));
        // Taking light away keeps the coverage of what's below; adding this effect's light
        // covers at least as much as the effect.
        let mut p = Acc::ZERO;
        p.blend(Rgba::with_alpha([1.0, 1.0, 1.0], 0.5), Blend::Normal, 1.0, false);
        p.blend(Rgba::opaque([1.0, 1.0, 1.0]), Blend::Mask, 1.0, false);
        assert_eq!((p.r, p.a), (0.0, 0.5));
        let (_, a) = mix(None, Rgba::opaque([0.0, 0.0, 1.0]), Blend::Behind, false);
        assert_eq!(a, 1.0);
        let (_, a) = mix(None, Rgba::CLEAR, Blend::Clip, false);
        assert_eq!(a, 0.0, "black over nothing still lets other rows show");
    }

    #[test]
    fn hsv_round_trips_like_xlights() {
        for c in [
            [1.0, 0.5, 0.0],
            [0.2, 0.4, 0.8],
            [0.5, 0.5, 0.5],
            [0.0, 0.0, 0.0],
            [0.9, 0.1, 0.6],
        ] {
            assert!(close(from_hsv(to_hsv(c)), c), "{c:?}");
        }
        assert!(close(to_hsv([0.0, 1.0, 0.0]), [1.0 / 3.0, 1.0, 1.0]));
        // Past the ends: a hue of 1 is the last sector, below 0 is grey at the lowest level.
        assert!(close(from_hsv([1.0, 1.0, 1.0]), [1.0, 0.0, 1.0]));
        assert!(close(from_hsv([-0.1, 0.5, 1.0]), [0.5, 0.5, 0.5]));
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
