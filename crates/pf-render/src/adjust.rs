//! An effect's color adjustments, applied to what it drew as xLights applies its color panel's
//! (`PixelBufferClass::GetMixedColor`): hue, saturation, and value shifts first, then (after the
//! sparkles) brightness and contrast, all before the effect fades and mixes with the layers below.

use crate::color::{Rgba, from_hsv, to_hsv, unit};
use pf_sequence::{Blend, Effect};

/// One effect's adjustments for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Adjust {
    hue: f32,
    saturation: f32,
    value: f32,
    brightness: f32,
    contrast: f32,
    /// The effect mixes by a blend that doesn't look at coverage, so where it drew nothing it
    /// counts as black, and a lighter value lights it.
    lights_unlit: bool,
}

impl Adjust {
    /// The adjustments of `effect` as it is now (its curves read), mixing by `blend`; `None`
    /// when it has none, as most effects don't.
    pub fn new(effect: &Effect, blend: Blend) -> Option<Self> {
        let adjust = Self {
            hue: effect.hue_adjust,
            saturation: effect.saturation_adjust,
            value: effect.value_adjust,
            brightness: effect.brightness,
            contrast: effect.contrast,
            lights_unlit: blend != Blend::Normal && effect.value_adjust > 0.0,
        };
        (adjust.shifts() || adjust.brightness != 1.0 || adjust.contrast != 0.0).then_some(adjust)
    }

    fn shifts(&self) -> bool {
        self.hue != 0.0 || self.saturation != 0.0 || self.value != 0.0
    }

    /// The hue, saturation, and value shifts, on a color as the effect drew it: the hue turns
    /// (and wraps), saturation and value are added to and stop at their ends. Coverage is kept.
    #[inline]
    pub fn shift(&self, mut color: Rgba) -> Rgba {
        if !self.shifts() {
            return color;
        }
        if color.a <= 0.0 {
            if !self.lights_unlit {
                return color;
            }
            color = Rgba::BLACK;
        }
        let [mut h, mut s, mut v] = to_hsv([unit(color.r), unit(color.g), unit(color.b)]);
        if self.hue != 0.0 {
            h += self.hue;
            if h < 0.0 {
                h += 1.0;
            } else if h > 1.0 {
                h -= 1.0;
            }
        }
        if self.saturation != 0.0 {
            s = unit(s + self.saturation);
        }
        if self.value != 0.0 {
            v = unit(v + self.value);
        }
        let [r, g, b] = from_hsv([h, s, v]);
        Rgba::new(r, g, b, color.a)
    }

    /// Brightness and contrast, on a color after its sparkles. Brightness scales each channel
    /// (up to full). With a contrast it scales the color's value instead, which then moves away
    /// from the middle: values under a half lose `contrast` of themselves, the rest gain it.
    #[inline]
    pub fn level(&self, color: Rgba) -> Rgba {
        if color.a <= 0.0 {
            return color;
        }
        if self.contrast != 0.0 {
            let [h, s, mut v] = to_hsv([unit(color.r), unit(color.g), unit(color.b)]);
            v *= self.brightness;
            if v < 0.5 {
                v -= v * self.contrast;
            } else {
                v += v * self.contrast;
            }
            let [r, g, b] = from_hsv([h, s, unit(v)]);
            return Rgba::new(r, g, b, color.a);
        }
        if self.brightness != 1.0 {
            let scale = |c: f32| unit(c * self.brightness);
            return Rgba::new(scale(color.r), scale(color.g), scale(color.b), color.a);
        }
        color
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_sequence::EffectKind;

    fn effect() -> Effect {
        Effect::new(EffectKind::On, 0, 1000)
    }

    fn adjust(set: impl FnOnce(&mut Effect)) -> Adjust {
        let mut e = effect();
        set(&mut e);
        Adjust::new(&e, Blend::Normal).expect("an adjustment")
    }

    fn close(c: Rgba, [r, g, b, a]: [f32; 4]) -> bool {
        [c.r - r, c.g - g, c.b - b, c.a - a]
            .iter()
            .all(|d| d.abs() < 1e-5)
    }

    const RED: Rgba = Rgba::new(1.0, 0.0, 0.0, 1.0);

    #[test]
    fn an_effect_left_alone_has_no_adjustment() {
        assert_eq!(Adjust::new(&effect(), Blend::Normal), None);
        assert_eq!(Adjust::new(&effect(), Blend::Add), None);
    }

    #[test]
    fn hue_turns_round_the_wheel_and_wraps() {
        // A third of the way round: red to green; back a third: red to blue.
        assert!(close(
            adjust(|e| e.hue_adjust = 1.0 / 3.0).shift(RED),
            [0.0, 1.0, 0.0, 1.0]
        ));
        assert!(close(
            adjust(|e| e.hue_adjust = -1.0 / 3.0).shift(RED),
            [0.0, 0.0, 1.0, 1.0]
        ));
        // Blue (two thirds round) and half a turn more wraps to yellow.
        let blue = Rgba::new(0.0, 0.0, 1.0, 0.5);
        assert!(close(
            adjust(|e| e.hue_adjust = 0.5).shift(blue),
            [1.0, 1.0, 0.0, 0.5]
        ));
        // White and grey have no hue to turn.
        let grey = Rgba::new(0.5, 0.5, 0.5, 1.0);
        assert!(close(
            adjust(|e| e.hue_adjust = 0.3).shift(grey),
            [0.5, 0.5, 0.5, 1.0]
        ));
    }

    #[test]
    fn saturation_and_value_are_added_and_stop_at_their_ends() {
        let pink = Rgba::new(1.0, 0.5, 0.5, 1.0);
        assert!(close(
            adjust(|e| e.saturation_adjust = -0.25).shift(pink),
            [1.0, 0.75, 0.75, 1.0]
        ));
        assert!(close(
            adjust(|e| e.saturation_adjust = 1.0).shift(pink),
            [1.0, 0.0, 0.0, 1.0]
        ));
        assert!(close(
            adjust(|e| e.saturation_adjust = -1.0).shift(pink),
            [1.0, 1.0, 1.0, 1.0]
        ));
        let dim = Rgba::new(0.5, 0.0, 0.0, 1.0);
        assert!(close(
            adjust(|e| e.value_adjust = 0.25).shift(dim),
            [0.75, 0.0, 0.0, 1.0]
        ));
        assert!(close(
            adjust(|e| e.value_adjust = 1.0).shift(dim),
            [1.0, 0.0, 0.0, 1.0]
        ));
        assert!(close(
            adjust(|e| e.value_adjust = -1.0).shift(dim),
            [0.0, 0.0, 0.0, 1.0]
        ));
        // Black the effect drew is lightened like any other color.
        assert!(close(
            adjust(|e| e.value_adjust = 0.25).shift(Rgba::BLACK),
            [0.25, 0.25, 0.25, 1.0]
        ));
    }

    #[test]
    fn where_nothing_was_drawn_only_a_blend_that_ignores_coverage_is_lightened() {
        let mut e = effect();
        e.value_adjust = 0.25;
        let normal = Adjust::new(&e, Blend::Normal).unwrap();
        assert_eq!(normal.shift(Rgba::CLEAR), Rgba::CLEAR);
        let add = Adjust::new(&e, Blend::Add).unwrap();
        assert!(close(add.shift(Rgba::CLEAR), [0.25, 0.25, 0.25, 1.0]));
        // Darkening, or turning the hue, leaves it clear under any blend.
        e.value_adjust = -0.25;
        e.hue_adjust = 0.5;
        assert_eq!(
            Adjust::new(&e, Blend::Add).unwrap().shift(Rgba::CLEAR),
            Rgba::CLEAR
        );
    }

    #[test]
    fn brightness_scales_each_channel_up_to_full() {
        let orange = Rgba::new(1.0, 0.5, 0.0, 0.5);
        assert!(close(
            adjust(|e| e.brightness = 0.5).level(orange),
            [0.5, 0.25, 0.0, 0.5]
        ));
        assert!(close(
            adjust(|e| e.brightness = 3.0).level(orange),
            [1.0, 1.0, 0.0, 0.5]
        ));
        assert!(close(
            adjust(|e| e.brightness = 0.0).level(orange),
            [0.0, 0.0, 0.0, 0.5]
        ));
        // Shifts happen before the sparkles and brightness after: each leaves the other's alone.
        assert_eq!(adjust(|e| e.brightness = 0.5).shift(orange), orange);
        assert_eq!(adjust(|e| e.hue_adjust = 0.5).level(orange), orange);
    }

    #[test]
    fn contrast_moves_values_away_from_the_middle_after_brightness() {
        let level = |v: f32, set: fn(&mut Effect)| adjust(set).level(Rgba::new(v, 0.0, 0.0, 1.0)).r;
        // Under a half: 0.4 loses a quarter of itself. From a half up: 0.6 gains a quarter.
        assert!((level(0.4, |e| e.contrast = 0.25) - 0.3).abs() < 1e-6);
        assert!((level(0.6, |e| e.contrast = 0.25) - 0.75).abs() < 1e-6);
        assert!(
            (level(0.9, |e| e.contrast = 0.25) - 1.0).abs() < 1e-6,
            "and stops at full"
        );
        // Brightness first: 0.8 at 70% is 0.56, which then gains 13%.
        let both = |e: &mut Effect| {
            e.brightness = 0.7;
            e.contrast = 0.13;
        };
        assert!((level(0.8, both) - 0.56 * 1.13).abs() < 1e-6);
        // 0.6 at 70% is 0.42, under a half, so it loses 13%.
        assert!((level(0.6, both) - 0.42 * 0.87).abs() < 1e-6);
        // With a contrast, brightness scales the color's value, which keeps its hue.
        let c = adjust(both).level(Rgba::new(1.0, 0.5, 0.0, 1.0));
        assert!((c.g / c.r - 0.5).abs() < 1e-5 && c.b == 0.0);
    }
}
