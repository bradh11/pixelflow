//! Color Shift: the target in one palette color, changing to the next (and on through the
//! palette), PixelFlow's own. For key changes, section changes, and drops.
//!
//! With `n` colors there are `n - 1` changes: the first at the start of the effect, the rest
//! spread evenly over it. Each takes the change time (at most the time until the next), instantly,
//! evenly, or eased. Staggered, each pixel starts its change later by where it is along the
//! sweep, so the new color travels across the prop: a stagger of 100% takes the whole change
//! time to cross, each pixel changing at once.

use crate::color::{Colors, Rgba, unit};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use crate::wipe::sweep_position;
use pf_sequence::{ColorShiftParams, ShiftEase, Sweep};

pub struct ColorShift {
    from: [f32; 3],
    to: [f32; 3],
    ease: ShiftEase,
    /// How far into the change under way (effect time, 0–1 of its length).
    into: f32,
    window: f32,
    stagger: f32,
    sweep: Sweep,
    canvas: Canvas,
}

impl ColorShift {
    pub fn new(p: &ColorShiftParams, time: &EffectTime, colors: Colors, canvas: Canvas) -> Self {
        let changes = colors.len().saturating_sub(1).max(1);
        let slot = 1.0 / changes as f32;
        let t = time.t_norm;
        let k = ((t / slot) as usize).min(changes - 1);
        let (from, to) = if colors.len() < 2 {
            (colors.get(0), colors.get(0))
        } else {
            (colors.get(k as u64), colors.get(k as u64 + 1))
        };
        Self {
            from,
            to,
            ease: p.ease,
            into: t - k as f32 * slot,
            window: (p.duration / 100.0).min(slot),
            stagger: unit(p.stagger / 100.0),
            sweep: p.direction,
            canvas,
        }
    }

    /// How far (0–1) a pixel `along` the sweep is through the change.
    #[inline]
    fn progress(&self, along: f32) -> f32 {
        let delay = self.stagger * self.window * along;
        let own = self.window * (1.0 - self.stagger);
        let x = self.into - delay;
        if own <= 0.0 {
            // No time to change in: it changes the moment its turn comes.
            return if x >= 0.0 { 1.0 } else { 0.0 };
        }
        let x = unit(x / own);
        match self.ease {
            ShiftEase::Instant => {
                if x > 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
            ShiftEase::Linear => x,
            ShiftEase::Smooth => x * x * (3.0 - 2.0 * x),
        }
    }
}

impl Shade for ColorShift {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let along = if self.stagger > 0.0 {
            sweep_position(px, self.sweep, self.canvas)
        } else {
            0.0
        };
        let f = self.progress(along);
        let (a, b) = (self.from, self.to);
        Rgba::opaque([0, 1, 2].map(|c| a[c] + (b[c] - a[c]) * f))
    }
}
