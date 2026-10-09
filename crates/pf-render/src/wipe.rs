//! Wipe: color sweeping across the target's buffer, PixelFlow's own (xLights' Fill and Curtain
//! come close). Each pixel's place along the sweep runs 0 (reached first) to 1 (last); the edge
//! crosses from 0 to 1 over the wipe time, soft over `softness`. Behind the edge the target fills
//! (or, with a bar width, only a bar that wide is lit), in the palette spread along the sweep.
//!
//! On a group drawn Per Preview each pixel is where it is in the layout, so a sweep crosses the
//! house.

use crate::color::{Colors, Rgba, unit};
use crate::effects::{Canvas, EffectTime, Shade};
use crate::geometry::Pixel;
use pf_sequence::{Sweep, WipeMode, WipeParams};
use std::f32::consts::FRAC_1_SQRT_2;

/// Where a pixel is along a sweep: 0 where it starts, 1 where it ends. On a target one pixel
/// high (or wide), sweeps up or down go along it (and across, up it).
pub(crate) fn sweep_position(px: &Pixel, sweep: Sweep, canvas: Canvas) -> f32 {
    let flat = canvas.rows <= 1 && canvas.columns > 1;
    let tall = canvas.columns <= 1 && canvas.rows > 1;
    let (u, v) = (px.u, px.v);
    let (across, up) = match (flat, tall) {
        (true, _) => (u, u),
        (_, true) => (v, v),
        _ => (u, v),
    };
    match sweep {
        Sweep::LeftToRight => across,
        Sweep::RightToLeft => 1.0 - across,
        Sweep::Up => up,
        Sweep::Down => 1.0 - up,
        Sweep::CenterOut => (across - 0.5).abs() * 2.0,
        Sweep::EdgesIn => 1.0 - (across - 0.5).abs() * 2.0,
        Sweep::Diagonal => (u + v) / 2.0,
        Sweep::Radial => ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() / FRAC_1_SQRT_2,
    }
}

/// Where the edge is in its crossing (0–1), and whether it's clearing instead of filling.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    /// Filling: the edge `progress` of the way across.
    On(f32),
    /// Clearing behind the edge.
    Off(f32),
}

/// The wipe's phase `t` of the way through the effect, each wipe taking `d` of it.
fn phase(mode: WipeMode, t: f32, d: f32) -> Phase {
    match mode {
        WipeMode::On => Phase::On(unit(t / d)),
        WipeMode::Off => Phase::Off(unit(t / d)),
        WipeMode::OnOff => {
            // Each wipe takes at most half the effect.
            let d = d.min(0.5);
            if t < 1.0 - d {
                Phase::On(unit(t / d))
            } else {
                Phase::Off(unit((t - (1.0 - d)) / d))
            }
        }
    }
}

pub struct Wipe {
    colors: Colors,
    sweep: Sweep,
    canvas: Canvas,
    phase: Phase,
    soft: f32,
    band: f32,
}

impl Wipe {
    pub fn new(p: &WipeParams, time: &EffectTime, colors: Colors, canvas: Canvas) -> Self {
        Self {
            colors,
            sweep: p.direction,
            canvas,
            phase: phase(p.mode, time.t_norm, (p.duration / 100.0).max(0.01)),
            soft: p.softness.max(1e-3),
            band: p.band,
        }
    }

    /// How much of a soft edge at `edge` covers the place `s` behind it (1 well behind, 0 ahead).
    #[inline]
    fn behind(&self, edge: f32, s: f32) -> f32 {
        unit((edge - s) / self.soft + 0.5)
    }
}

impl Shade for Wipe {
    #[inline]
    fn shade(&self, px: &Pixel) -> Rgba {
        let s = sweep_position(px, self.sweep, self.canvas);
        let level = if self.band > 0.0 {
            // A bar sweeping across: its leading edge goes from just before 0 to past 1 plus the
            // bar, so it enters and leaves whole. Clearing sweeps it back.
            let (progress, back) = match self.phase {
                Phase::On(x) => (x, false),
                Phase::Off(x) => (x, true),
            };
            let travel = 1.0 + self.band + self.soft;
            let lead = progress * travel - self.soft / 2.0;
            let lead = if back { 1.0 + self.band - lead } else { lead };
            self.behind(lead, s) * (1.0 - self.behind(lead - self.band, s))
        } else {
            // The edge goes from half its softness before 0 to half past 1, so the wipe starts
            // dark and ends lit everywhere.
            let edge = |x: f32| x * (1.0 + self.soft) - self.soft / 2.0;
            match self.phase {
                Phase::On(x) => self.behind(edge(x), s),
                Phase::Off(x) => 1.0 - self.behind(edge(x), s),
            }
        };
        if level <= 0.0 {
            return Rgba::CLEAR;
        }
        Rgba::with_alpha(self.colors.ramp(s), level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn wipes_on_hold_and_clear_in_their_share_of_the_effect() {
        let is = |found: Phase, on: bool, x: f32| match found {
            Phase::On(at) => on && near(at, x),
            Phase::Off(at) => !on && near(at, x),
        };
        assert!(is(phase(WipeMode::On, 0.25, 0.5), true, 0.5));
        assert!(is(phase(WipeMode::On, 0.9, 0.5), true, 1.0), "then holds");
        assert!(is(phase(WipeMode::Off, 0.25, 0.5), false, 0.5));
        // On, then off: each wipe at most half the effect.
        assert!(is(phase(WipeMode::OnOff, 0.4, 0.8), true, 0.8));
        assert!(is(phase(WipeMode::OnOff, 0.6, 0.2), true, 1.0));
        assert!(is(phase(WipeMode::OnOff, 0.9, 0.2), false, 0.5));
    }

    #[test]
    fn sweeps_measure_from_where_they_start() {
        let canvas = Canvas {
            columns: 10,
            rows: 10,
        };
        let px = |u, v| Pixel {
            u,
            v,
            index: 0,
            count: 1,
        };
        let at = |sweep, u, v| sweep_position(&px(u, v), sweep, canvas);
        assert!(near(at(Sweep::LeftToRight, 0.25, 0.9), 0.25));
        assert!(near(at(Sweep::RightToLeft, 0.25, 0.9), 0.75));
        assert!(near(at(Sweep::Down, 0.25, 0.9), 0.1));
        assert!(near(at(Sweep::CenterOut, 0.5, 0.0), 0.0));
        assert!(near(at(Sweep::EdgesIn, 0.0, 0.0), 0.0));
        assert!(near(at(Sweep::Diagonal, 1.0, 0.0), 0.5));
        assert!(near(at(Sweep::Radial, 1.0, 1.0), 1.0));
        // A line one pixel high sweeps "up" along itself.
        let line = Canvas { columns: 10, rows: 1 };
        assert!(near(sweep_position(&px(0.3, 0.5), Sweep::Up, line), 0.3));
    }
}
