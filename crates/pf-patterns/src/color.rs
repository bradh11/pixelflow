//! Colors with an optional white channel.

/// An 8-bit color. `w` is only sent to RGBW pixels; RGB pixels ignore it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgbw {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub w: u8,
}

impl Rgbw {
    pub const OFF: Rgbw = Rgbw::rgb(0, 0, 0);
    pub const RED: Rgbw = Rgbw::rgb(255, 0, 0);
    pub const GREEN: Rgbw = Rgbw::rgb(0, 255, 0);
    pub const BLUE: Rgbw = Rgbw::rgb(0, 0, 255);
    /// Full white: all three color channels on, plus the white channel on RGBW pixels.
    pub const WHITE: Rgbw = Rgbw {
        r: 255,
        g: 255,
        b: 255,
        w: 255,
    };

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, w: 0 }
    }

    /// Parses `rrggbb` or `rrggbbww` hex, with or without a leading `#`.
    pub fn from_hex(text: &str) -> Option<Self> {
        let hex = text.strip_prefix('#').unwrap_or(text);
        if !(hex.len() == 6 || hex.len() == 8) || !hex.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Self {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            w: if hex.len() == 8 { byte(6)? } else { 0 },
        })
    }

    /// Each channel scaled by `level` (clamped to 0.0–1.0).
    pub fn scaled(self, level: f32) -> Self {
        let level = level.clamp(0.0, 1.0);
        let s = |v: u8| (f32::from(v) * level).round() as u8;
        Self {
            r: s(self.r),
            g: s(self.g),
            b: s(self.b),
            w: s(self.w),
        }
    }

    /// Writes the color into one pixel of `channels_per_pixel` bytes (3 = RGB, 4 = RGBW).
    pub fn write(self, pixel: &mut [u8]) {
        pixel[0] = self.r;
        pixel[1] = self.g;
        pixel[2] = self.b;
        if let Some(w) = pixel.get_mut(3) {
            *w = self.w;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_with_and_without_white() {
        assert_eq!(Rgbw::from_hex("#ff8000"), Some(Rgbw::rgb(255, 128, 0)));
        assert_eq!(
            Rgbw::from_hex("000000ff"),
            Some(Rgbw {
                r: 0,
                g: 0,
                b: 0,
                w: 255
            })
        );
        assert_eq!(Rgbw::from_hex("fff"), None);
        assert_eq!(Rgbw::from_hex("gg0000"), None);
    }

    #[test]
    fn writes_rgb_and_rgbw_pixels() {
        let mut rgb = [0u8; 3];
        Rgbw::WHITE.write(&mut rgb);
        assert_eq!(rgb, [255, 255, 255]);
        let mut rgbw = [0u8; 4];
        Rgbw::WHITE.write(&mut rgbw);
        assert_eq!(rgbw, [255, 255, 255, 255]);
    }

    #[test]
    fn scaling_rounds_and_clamps() {
        assert_eq!(Rgbw::rgb(255, 100, 0).scaled(0.5), Rgbw::rgb(128, 50, 0));
        assert_eq!(Rgbw::RED.scaled(2.0), Rgbw::RED);
    }
}
