//! Lightning: strikes at random moments, each flickering like real lightning, worked out from the
//! effect's seed and the time alone (any frame renders on its own).
//!
//! Time is split into slots of one strike each (`1 / density` seconds), and each slot's strike
//! lands at a random moment in it. A strike is a bright main stroke, one to three quick
//! re-strikes of the same channel 40–130 ms apart (each a little dimmer), then a fading tail. On a
//! matrix-like target the bolt is drawn: a jagged line from the top toward the bottom, with forks
//! by `branches`, and the whole target glows faintly with each stroke. On a line or outline (or
//! with Flash only) the whole target flashes instead.

use crate::color::{Colors, Rgba, unit};
use crate::effects::{Canvas, EffectTime, Rng, Shade, hash, hash01};
use crate::geometry::Pixel;
use pf_sequence::LightningParams;

/// How long one stroke takes to fade, and the last stroke's slower tail.
const STROKE_FADE_MS: f32 = 45.0;
const TAIL_FADE_MS: f32 = 70.0;
/// Strikes that started longer ago than this have faded away.
const STRIKE_MS: u64 = 1_500;
/// Bolts drawn at once (the newest, when strikes overlap).
const MAX_BOLTS: usize = 4;

/// One straight piece of a bolt, in cells, and how bright it is next to the main channel.
#[derive(Debug, Clone, Copy)]
struct Segment {
    a: [f32; 2],
    b: [f32; 2],
    level: f32,
}

/// A strike's bolt, and how bright it is now.
#[derive(Debug, Clone)]
struct Bolt {
    segments: Vec<Segment>,
    /// The bolt's bounding box in cells (with its width), to skip far pixels quickly.
    min: [f32; 2],
    max: [f32; 2],
    level: f32,
}

/// How bright a strike that began `since_ms` ago is, by its strokes (worked out from `key`).
pub(crate) fn strike_level(key: u64, since_ms: f32) -> f32 {
    let strokes = 2 + (key % 3) as usize;
    let mut at = 0.0f32;
    let mut level = 0.0f32;
    for k in 0..strokes {
        if k > 0 {
            at += 40.0 + 90.0 * hash01(key, k as u64, 1);
        }
        if since_ms < at {
            break;
        }
        let peak = if k == 0 {
            1.0
        } else {
            0.55 + 0.4 * hash01(key, k as u64, 2)
        };
        let fade = if k + 1 == strokes {
            TAIL_FADE_MS
        } else {
            STROKE_FADE_MS
        };
        level = level.max(peak * (-(since_ms - at) / fade).exp());
    }
    level
}

pub struct Lightning {
    color: [f32; 3],
    /// The brightest flash on the whole target now.
    flash: f32,
    glow: f32,
    bolts: Vec<Bolt>,
    /// Half the bolt's width, in cells.
    half_width: f32,
    canvas: Canvas,
    flash_only: bool,
}

impl Lightning {
    pub fn new(p: &LightningParams, time: &EffectTime, colors: Colors, seed: u64, canvas: Canvas) -> Self {
        let slot_ms = (1000.0 / f64::from(p.density.max(0.01))).max(1.0);
        let now = time.elapsed_ms as f64;
        let first = ((now - STRIKE_MS as f64) / slot_ms).floor().max(0.0) as u64;
        let last = (now / slot_ms).floor() as u64;
        let flash_only = p.flash_only || canvas.rows < 3 || canvas.columns < 3;
        let mut strikes: Vec<(u64, f32)> = Vec::new();
        for slot in first..=last {
            let key = hash(seed, slot, 0x11);
            // Somewhere in the first 80% of its slot, so strikes don't bunch up.
            let start = (slot as f64 + 0.8 * f64::from(hash01(seed, slot, 0x12))) * slot_ms;
            if start > now {
                continue;
            }
            let level = strike_level(key, (now - start) as f32);
            if level > 0.004 {
                strikes.push((key, level));
            }
        }
        let flash = strikes.iter().fold(0.0f32, |m, &(_, l)| m.max(l));
        let bolts = if flash_only {
            Vec::new()
        } else {
            strikes
                .iter()
                .rev()
                .take(MAX_BOLTS)
                .map(|&(key, level)| bolt(key, level, p, canvas))
                .collect()
        };
        Self {
            color: colors.get(0),
            flash,
            glow: p.glow,
            bolts,
            half_width: p.thickness.max(1) as f32 / 2.0,
            canvas,
            flash_only,
        }
    }
}

/// The bolt of the strike `key`: from a random place along the top down toward the bottom, in
/// `segments` jags, with forks.
fn bolt(key: u64, level: f32, p: &LightningParams, canvas: Canvas) -> Bolt {
    let (w, h) = ((canvas.columns - 1) as f32, (canvas.rows - 1) as f32);
    let mut rng = Rng::new(key);
    let n = p.segments.max(2) as usize;
    let step = h / n as f32;
    let jag = (w * 0.12).max(1.0);
    let mut segments = Vec::with_capacity(n * 2);
    let mut at = [w * (0.15 + 0.7 * rng.unit() as f32), h];
    for i in 0..n {
        let next = [
            (at[0] + (rng.unit() as f32 - 0.5) * 2.0 * jag).clamp(0.0, w),
            h - (i + 1) as f32 * step,
        ];
        segments.push(Segment {
            a: at,
            b: next,
            level: 1.0,
        });
        // A fork off this joint, running down and away for a few jags.
        if i + 1 < n && (rng.unit() as f32) < p.branches * 0.6 {
            let side = if rng.unit() < 0.5 { -1.0 } else { 1.0 };
            let mut from = next;
            for _ in 0..(1 + rng.int(1, 3)) {
                let to = [
                    (from[0] + side * jag * (0.5 + rng.unit() as f32)).clamp(0.0, w),
                    (from[1] - step * (0.6 + 0.6 * rng.unit() as f32)).max(0.0),
                ];
                segments.push(Segment {
                    a: from,
                    b: to,
                    level: 0.6,
                });
                from = to;
            }
        }
        at = next;
    }
    let reach = p.thickness.max(1) as f32 / 2.0 + 1.0;
    let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
    for s in &segments {
        for c in 0..2 {
            min[c] = min[c].min(s.a[c].min(s.b[c]) - reach);
            max[c] = max[c].max(s.a[c].max(s.b[c]) + reach);
        }
    }
    Bolt {
        segments,
        min,
        max,
        level,
    }
}

/// Distance from `p` to the segment `a`–`b`.
fn distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx * dx + dy * dy;
    let t = if len > 0.0 {
        unit(((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len)
    } else {
        0.0
    };
    let (ex, ey) = (a[0] + t * dx - p[0], a[1] + t * dy - p[1]);
    (ex * ex + ey * ey).sqrt()
}

impl Shade for Lightning {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        if self.flash <= 0.0 {
            return Rgba::CLEAR;
        }
        if self.flash_only {
            return Rgba::with_alpha(self.color, self.flash);
        }
        let p = [
            px.u * (self.canvas.columns - 1) as f32,
            px.v * (self.canvas.rows - 1) as f32,
        ];
        let mut level = self.glow * self.flash;
        for bolt in &self.bolts {
            if p[0] < bolt.min[0] || p[0] > bolt.max[0] || p[1] < bolt.min[1] || p[1] > bolt.max[1] {
                continue;
            }
            for s in &bolt.segments {
                // Lit within half the width of the line, softening over the next cell.
                let d = distance(p, s.a, s.b);
                let on = unit(self.half_width + 0.5 - d);
                level = level.max(on * s.level * bolt.level);
            }
        }
        if level <= 0.0 {
            return Rgba::CLEAR;
        }
        Rgba::with_alpha(self.color, unit(level))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_strike_flashes_restrikes_and_fades() {
        for key in [1u64, 2, 3, 99, 12345] {
            assert!(
                (strike_level(key, 0.0) - 1.0).abs() < 1e-6,
                "the main stroke is full"
            );
            assert!(strike_level(key, 30.0) < 0.6, "and falls fast");
            assert!(
                strike_level(key, 1_400.0) < 0.004,
                "gone well within a strike's time"
            );
        }
        // Some strike re-strikes: brighter again after its first stroke has faded.
        let restrikes = (0..20u64).any(|key| {
            let low = strike_level(key, 38.0);
            (40..140).any(|ms| strike_level(key, ms as f32) > low + 0.2)
        });
        assert!(restrikes);
    }
}
