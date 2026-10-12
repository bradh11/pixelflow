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
//!
//! Lit, that is, before the effect's own Transparency setting (an On's) makes it see-through:
//! Transparency hides the effect, not its sparkles. In xLights' frames an On whose Transparency
//! is at 100% is dark and still flashes its sparkles on as many pixels as it would at 0%, each
//! at full strength, so a fully see-through On with sparkles is a layer of sparkles alone. That
//! is the only thing found in those frames that sparkles look past. They dim with the effect's
//! fade (through a two-second fade-in they rise with the effect, and show at their three full
//! levels only once it is over); the faint parts of an effect (a fan's blades, a morph's tail)
//! flash as often in those frames as they already do in PixelFlow's; and pixels an effect never
//! reaches never flash. So an effect with a Transparency is drawn without it, and
//! [`Sparkles::see_through`] takes the Transparency off the pixels that aren't flashing.

use crate::color::{Rgba, lit, unit};
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
    /// How much of the effect its own Transparency setting lets show where it isn't flashing.
    cover: f32,
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
            cover: 1.0,
        })
    }

    /// The same sparkles over an effect whose own Transparency setting is `transparency` (0,
    /// none, to 1). [`Sparkles::apply`] is then given the effect's colors as they are without
    /// that Transparency, and takes it off the ones that aren't flashing.
    pub fn see_through(self, transparency: f32) -> Self {
        Self {
            cover: 1.0 - unit(transparency),
            ..self
        }
    }

    /// Whether the effect's own Transparency is left to these sparkles to take off (see
    /// [`Sparkles::see_through`]).
    pub fn takes_transparency(&self) -> bool {
        self.cover < 1.0
    }

    /// The effect's color at the pixel `index` along the target, with its sparkle if it has one
    /// this frame.
    #[inline]
    pub fn apply(&self, color: Rgba, index: u32) -> Rgba {
        if color.a <= 0.0 || !lit([color.r * color.a, color.g * color.a, color.b * color.a]) {
            return self.covered(color);
        }
        let phase = hash(self.seed, u64::from(index), 0) % 10_000;
        let level = match (phase + self.frame) % self.cycle {
            4 => 1.0,
            3 | 5 => 0.75,
            2 | 6 => 0.53,
            _ => return self.covered(color),
        };
        Rgba::opaque(self.color.map(|c| c * level))
    }

    /// The effect's own color, showing as much as its Transparency lets it.
    #[inline]
    fn covered(&self, color: Rgba) -> Rgba {
        Rgba {
            a: color.a * self.cover,
            ..color
        }
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

    #[test]
    fn an_effects_own_transparency_hides_the_effect_and_not_its_sparkles() {
        let solid = Sparkles::new(150, [0.0, 0.0, 1.0], 5, 12).unwrap();
        assert!(!solid.takes_transparency());
        let flashing =
            |s: &Sparkles| -> Vec<u32> { (0..4000).filter(|&i| s.apply(RED, i).b > 0.0).collect() };
        let want = flashing(&solid);
        assert!(want.len() > 250, "{}", want.len());
        for cover in [0.75, 0.25, 0.0] {
            let sheer = solid.see_through(1.0 - cover);
            assert!(sheer.takes_transparency());
            // The same pixels flash, as brightly and as fully covering, however little shows.
            assert_eq!(flashing(&sheer), want, "{cover}");
            for i in 0..4000 {
                let (through, whole) = (sheer.apply(RED, i), solid.apply(RED, i));
                if want.contains(&i) {
                    assert_eq!(through, whole, "{cover}: pixel {i}");
                    assert_eq!(through.a, 1.0);
                } else {
                    // The rest is the effect, showing as much as its Transparency lets it.
                    assert_eq!(through, Rgba::new(1.0, 0.0, 0.0, cover), "{cover}: pixel {i}");
                }
            }
        }
        // No Transparency, less than none, or nonsense is none; more than all of it is all.
        for none in [0.0, -1.0, f32::NAN] {
            assert_eq!(solid.see_through(none), solid);
        }
        assert_eq!(solid.see_through(7.0), solid.see_through(1.0));
    }

    #[test]
    fn a_see_through_effect_sparkles_only_where_it_would_be_lit() {
        // Where the effect draws nothing, is black, or has all but faded away on its own, its
        // Transparency changes nothing: no sparkle, and no more showing than before.
        let s = Sparkles::new(200, [1.0; 3], 1, 0).unwrap().see_through(0.5);
        for i in 0..100 {
            assert_eq!(s.apply(Rgba::CLEAR, i), Rgba::CLEAR);
            assert_eq!(s.apply(Rgba::BLACK, i), Rgba::new(0.0, 0.0, 0.0, 0.5));
            let faint = Rgba::new(1.0, 1.0, 1.0, 0.001);
            assert_eq!(s.apply(faint, i), Rgba::new(1.0, 1.0, 1.0, 0.0005));
        }
    }
}
