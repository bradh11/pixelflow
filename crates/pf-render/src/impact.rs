//! Impact: a full-brightness hit at the start of the effect that fades away over the rest of it.
//! PixelFlow's own (xLights has no single effect for it; a white On with a falling curve comes
//! closest).
//!
//! The hit holds for `hold`, then fades out by the end of the effect: evenly, fast then slow, or
//! with a punch (a quick dip and a rebound before the fade). With a bloom it spreads out from its
//! point over the first `bloom` milliseconds, soft at its edge. With colors shifting, it starts in
//! its hit color and fades through the palette as it decays.

use crate::color::{Colors, Rgba, unit};
use crate::effects::{EffectTime, Shade};
use crate::geometry::Pixel;
use pf_sequence::{HitColor, ImpactDecay, ImpactParams};

/// How fast the fast-then-slow fade falls: e^-4.6 is about 1%.
const EXPONENTIAL: f32 = 4.6;

/// How bright a hit is `x` of the way through its fade (0 at the hit, 1 at the end).
pub(crate) fn decay_level(decay: ImpactDecay, x: f32) -> f32 {
    let x = unit(x);
    match decay {
        ImpactDecay::Linear => 1.0 - x,
        // Down to about 1% by the end, then the last of it trimmed to reach black.
        ImpactDecay::Exponential => {
            let floor = (-EXPONENTIAL).exp();
            ((-EXPONENTIAL * x).exp() - floor).max(0.0) / (1.0 - floor)
        }
        ImpactDecay::Punch => {
            // A quick dip to 35%, a rebound to 75%, then an eased fade to black.
            let ease = |t: f32| 1.0 - (1.0 - t) * (1.0 - t);
            if x < 0.1 {
                1.0 - 0.65 * ease(x / 0.1)
            } else if x < 0.25 {
                0.35 + 0.4 * ease((x - 0.1) / 0.15)
            } else {
                let t = (x - 0.25) / 0.75;
                0.75 * (1.0 - t) * (1.0 - t)
            }
        }
    }
}

pub struct Impact {
    color: [f32; 3],
    level: f32,
    /// The bloom's reach from the hit point (1.0 reaches the farthest corner), and its point;
    /// `None` once it covers everything.
    bloom: Option<(f32, f32, f32)>,
}

impl Impact {
    pub fn new(p: &ImpactParams, time: &EffectTime, colors: Colors) -> Self {
        let elapsed = time.elapsed_ms as f32;
        let hold = p.hold.max(0.0);
        let fade_ms = (time.length_ms as f32 - hold).max(1.0);
        let x = ((elapsed - hold) / fade_ms).max(0.0);
        let first = match p.color {
            HitColor::White => [1.0; 3],
            HitColor::Palette => colors.get(0),
        };
        let color = if p.color_shift {
            // From the hit color through the palette (after the first color when it's the hit).
            let rest = match p.color {
                HitColor::White => 0,
                HitColor::Palette => 1,
            };
            let stops: Vec<[f32; 3]> = std::iter::once(first)
                .chain((rest..colors.len()).map(|k| colors.get(k as u64)))
                .collect();
            along(&stops, unit(x))
        } else {
            first
        };
        let bloom = (p.bloom > 0.0 && elapsed < p.bloom)
            .then(|| (elapsed / p.bloom, p.center_x / 100.0, p.center_y / 100.0));
        Self {
            color,
            level: decay_level(p.decay, x),
            bloom,
        }
    }
}

/// The color `x` (0–1) of the way along `stops`.
fn along(stops: &[[f32; 3]], x: f32) -> [f32; 3] {
    if stops.len() < 2 {
        return stops.first().copied().unwrap_or([1.0; 3]);
    }
    let at = x * (stops.len() - 1) as f32;
    let i = (at as usize).min(stops.len() - 2);
    let f = at - i as f32;
    let (a, b) = (stops[i], stops[i + 1]);
    [0, 1, 2].map(|c| a[c] + (b[c] - a[c]) * f)
}

/// The share of the bloom's soft edge, as a fraction of its reach.
const BLOOM_EDGE: f32 = 0.15;

impl Shade for Impact {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let mut level = self.level;
        if let Some((reach, cx, cy)) = self.bloom {
            // Distance from the hit point, 1.0 at the farthest corner of the prop.
            let far = |c: f32| c.max(1.0 - c);
            let corner = (far(cx).powi(2) + far(cy).powi(2)).sqrt().max(1e-6);
            let r = ((px.u - cx).powi(2) + (px.v - cy).powi(2)).sqrt() / corner;
            let edge = reach * (1.0 + BLOOM_EDGE);
            level *= unit((edge - r) / BLOOM_EDGE);
        }
        if level <= 0.0 {
            return Rgba::CLEAR;
        }
        Rgba::with_alpha(self.color, level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_decay_starts_full_and_ends_dark() {
        for decay in [ImpactDecay::Linear, ImpactDecay::Exponential, ImpactDecay::Punch] {
            assert!((decay_level(decay, 0.0) - 1.0).abs() < 1e-6, "{decay:?}");
            assert!(decay_level(decay, 1.0).abs() < 1e-6, "{decay:?}");
            assert!((0.0..=1.0).contains(&decay_level(decay, 0.5)), "{decay:?}");
        }
        // Fast then slow: below the even fade early on.
        assert!(decay_level(ImpactDecay::Exponential, 0.2) < decay_level(ImpactDecay::Linear, 0.2) - 0.2);
        // A punch dips, then rebounds before it fades.
        let dip = decay_level(ImpactDecay::Punch, 0.1);
        let rebound = decay_level(ImpactDecay::Punch, 0.25);
        assert!(dip < 0.4 && rebound > 0.7, "{dip} {rebound}");
    }
}
