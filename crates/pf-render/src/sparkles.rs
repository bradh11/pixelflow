//! Sparkles, the way xLights adds them to a layer (`ApplySparkles` in its layer blending).
//!
//! Each pixel has its own random phase (0–9999). On frame `f` of the effect, a pixel the effect
//! lights is at step `(phase + f) % (208 - sparkles)` of its cycle: steps 2 to 6 flash the
//! sparkle color at 53%, 75%, 100%, 75%, 53%, and every other step leaves the effect's color. So
//! with Sparkles at `s`, each lit pixel flashes for 5 frames out of every `208 - s`, and about
//! `5 / (208 - s)` of the lit pixels are flashing at any moment.
//!
//! xLights keys the phase to the model and node; PixelFlow keys it to the effect (its id) and
//! the pixel's place along the target, like its other random effects, so a frame depends only on
//! the document. Sparkles go on after blur and before the effect is faded and mixed, as in
//! xLights, and flash only where the effect is lit.

use crate::color::{Rgba, lit};
use crate::effects::hash;

/// Mixed into the effect's seed so sparkles don't line up with the effect's own randomness.
const SALT: u64 = 0x5BA2_C1E5;

/// Sparkles for one effect on one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Sparkles {
    seed: u64,
    frame: u64,
    cycle: u64,
    color: [f32; 3],
}

impl Sparkles {
    /// `amount`: the effect's Sparkles setting (0, none, to 200); `frame`: frames since the
    /// effect started. `None` when there are none.
    pub fn new(amount: u32, color: [f32; 3], seed: u64, frame: u64) -> Option<Self> {
        let amount = amount.min(pf_sequence::MAX_SPARKLES);
        (amount > 0).then(|| Self {
            seed: seed ^ SALT,
            frame,
            cycle: u64::from(208 - amount),
            color,
        })
    }

    /// The effect's color at the pixel `index` along the target, with its sparkle if it has one
    /// this frame.
    #[inline]
    pub fn apply(&self, color: Rgba, index: u32) -> Rgba {
        if color.a <= 0.0 || !lit([color.r * color.a, color.g * color.a, color.b * color.a]) {
            return color;
        }
        let phase = hash(self.seed, u64::from(index), 0) % 10_000;
        let level = match (phase + self.frame) % self.cycle {
            4 => 1.0,
            3 | 5 => 0.75,
            2 | 6 => 0.53,
            _ => return color,
        };
        Rgba::opaque(self.color.map(|c| c * level))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgba = Rgba::new(1.0, 0.0, 0.0, 1.0);

    #[test]
    fn none_means_none() {
        assert_eq!(Sparkles::new(0, [1.0; 3], 7, 0), None);
    }

    #[test]
    fn each_lit_pixel_flashes_five_frames_per_cycle_ramping_up_and_down() {
        let s = Sparkles::new(200, [1.0, 1.0, 1.0], 42, 0).unwrap();
        // A cycle is 8 frames: follow pixel 3 through two cycles.
        let levels: Vec<f32> = (0..16)
            .map(|f| Sparkles { frame: f, ..s }.apply(RED, 3))
            .map(|c| if c == RED { 0.0 } else { c.g })
            .collect();
        let peak = levels.iter().position(|&l| l == 1.0).unwrap();
        let around: Vec<f32> = (0..8).map(|k| levels[(peak + 14 + k) % 16]).collect();
        assert_eq!(around, [0.53, 0.75, 1.0, 0.75, 0.53, 0.0, 0.0, 0.0], "{levels:?}");
        assert_eq!(levels.iter().filter(|&&l| l > 0.0).count(), 10, "{levels:?}");
    }

    #[test]
    fn density_is_five_in_208_minus_the_setting() {
        for amount in [8, 54, 150] {
            let s = Sparkles::new(amount, [1.0; 3], 9, 1234).unwrap();
            let n = 20_000;
            let flashing = (0..n).filter(|&i| s.apply(RED, i) != RED).count();
            let want = 5.0 / (208.0 - f64::from(amount));
            let got = flashing as f64 / f64::from(n);
            assert!((got - want).abs() < want * 0.15, "{amount}: {got} vs {want}");
        }
    }

    #[test]
    fn unlit_and_black_pixels_never_sparkle() {
        let s = Sparkles::new(200, [1.0; 3], 1, 0).unwrap();
        for i in 0..100 {
            assert_eq!(s.apply(Rgba::CLEAR, i), Rgba::CLEAR);
            assert_eq!(s.apply(Rgba::BLACK, i), Rgba::BLACK);
            let faint = Rgba::new(1.0, 1.0, 1.0, 0.001);
            assert_eq!(s.apply(faint, i), faint);
        }
    }

    #[test]
    fn the_same_effect_and_frame_always_sparkle_the_same() {
        let a = Sparkles::new(100, [0.0, 0.0, 1.0], 77, 30).unwrap();
        let b = Sparkles::new(100, [0.0, 0.0, 1.0], 77, 30).unwrap();
        let other = Sparkles::new(100, [0.0, 0.0, 1.0], 78, 30).unwrap();
        let pattern = |s: &Sparkles| (0..2000).map(|i| s.apply(RED, i)).collect::<Vec<_>>();
        assert_eq!(pattern(&a), pattern(&b));
        assert_ne!(pattern(&a), pattern(&other), "another effect sparkles elsewhere");
        let c = a.apply(RED, (0..2000).find(|&i| a.apply(RED, i) != RED).unwrap());
        assert_eq!((c.r, c.a), (0.0, 1.0), "in the sparkle color, fully covering");
    }
}
