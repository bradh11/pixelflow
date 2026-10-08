//! Effects xLights works out frame by frame from the frame before: falling snowflakes, Lines,
//! Life, and Tendril. Their state at frame N is the state at frame N − 1 moved on a step (with
//! the settings at frame N, so curves apply as they do in xLights), starting from the effect's
//! first frame. Every step's randomness is keyed to the effect's seed and the frame, so frame N
//! depends only on the document, however it's reached.
//!
//! Drawn on its own ([`Shader::new`]), such an effect steps from its first frame every time. The
//! renderer keeps each one's state between frames instead ([`Sims`]), so playing and exporting
//! take one step a frame; seeking backwards starts again from the first frame.

use crate::color::Colors;
use crate::effects::{Canvas, EffectTime, Shader};
use crate::life::{Colony, Life};
use crate::lines::{Lines, Moving};
use crate::snowflakes::{Fall, Snowflakes};
use crate::tendril::{Tendril, Tendrils};
use pf_sequence::{Effect, EffectParams, SnowflakesMotion};
use std::collections::HashMap;
use std::hash::Hash;

/// Whether an effect with these settings is worked out frame by frame.
pub(crate) fn simulated(params: &EffectParams) -> bool {
    match params {
        EffectParams::Snowflakes(p) => p.motion != SnowflakesMotion::Blowing,
        EffectParams::Lines(_) | EffectParams::Life(_) | EffectParams::Tendril(_) => true,
        _ => false,
    }
}

/// What an effect carries from frame to frame.
#[derive(Debug, Clone)]
pub(crate) enum Sim {
    Snowflakes(Fall),
    Lines(Moving),
    Life(Colony),
    Tendril(Tendrils),
}

/// What every step needs besides the settings.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Context {
    pub seed: u64,
    pub colors: Colors,
    pub canvas: Canvas,
    pub frame_ms: u32,
    /// The effect's first frame, counted from the start of the sequence.
    pub first_frame: u64,
}

impl Sim {
    /// The state before the first frame, for these (first-frame) settings; `None` for an effect
    /// that isn't worked out frame by frame.
    pub fn start(params: &EffectParams, cx: &Context) -> Option<Self> {
        Some(match params {
            EffectParams::Snowflakes(p) if p.motion != SnowflakesMotion::Blowing => {
                Sim::Snowflakes(Fall::scatter(p, cx.seed, cx.canvas))
            }
            EffectParams::Lines(_) => Sim::Lines(Moving::new(cx.canvas)),
            EffectParams::Life(p) => Sim::Life(Colony::seed(p, &cx.colors, cx.seed, cx.canvas)),
            EffectParams::Tendril(p) => Sim::Tendril(Tendrils::new(p, cx.seed, cx.canvas)),
            _ => return None,
        })
    }

    /// Moves on to frame `frame` (0 the first) with the settings then (clamped).
    pub fn step(&mut self, params: &EffectParams, frame: u64, cx: &Context) {
        match (self, params) {
            (Sim::Snowflakes(fall), EffectParams::Snowflakes(p)) => fall.step(p, cx.seed, frame),
            (Sim::Lines(lines), EffectParams::Lines(p)) => lines.step(p, cx.seed, frame),
            (Sim::Life(colony), EffectParams::Life(p)) => {
                colony.step(p, &cx.colors, cx.seed, frame, cx.frame_ms);
            }
            (Sim::Tendril(tendrils), EffectParams::Tendril(p)) => {
                tendrils.step(p, cx.seed, frame, cx.first_frame + frame);
            }
            _ => {}
        }
    }

    /// The frame as it stands, drawn with the settings now (clamped).
    pub fn shader(&self, params: &EffectParams, time: &EffectTime, cx: &Context) -> Shader {
        match (self, params) {
            (Sim::Snowflakes(fall), _) => Shader::Snowflakes(Snowflakes::falling(
                fall,
                cx.colors,
                cx.seed,
                time.frame(),
                cx.canvas,
            )),
            (Sim::Lines(lines), EffectParams::Lines(p)) => {
                Shader::Lines(Lines::new(lines, p, cx.colors, cx.canvas))
            }
            (Sim::Life(colony), _) => Shader::Life(Life::new(colony)),
            (Sim::Tendril(tendrils), EffectParams::Tendril(p)) => {
                Shader::Tendril(Tendril::new(tendrils, p, time, cx.colors, cx.canvas))
            }
            _ => Shader::Off(crate::effects::Off),
        }
    }
}

/// An effect worked out frame by frame from its first frame to `time`, with the same settings
/// throughout (clamped); `None` for one that isn't worked out that way.
pub(crate) fn run(
    params: &EffectParams,
    time: &EffectTime,
    colors: Colors,
    seed: u64,
    canvas: Canvas,
) -> Option<Shader> {
    let cx = Context {
        seed,
        colors,
        canvas,
        frame_ms: time.frame_ms,
        first_frame: time.start_ms / u64::from(time.frame_ms.max(1)),
    };
    let mut sim = Sim::start(params, &cx)?;
    for frame in 0..=time.frame() {
        sim.step(params, frame, &cx);
    }
    Some(sim.shader(params, time, &cx))
}

/// One effect's state as the renderer keeps it: the effect it was worked out for, and how far.
#[derive(Debug, Clone)]
struct Kept {
    effect: Effect,
    canvas: Canvas,
    frame_ms: u32,
    /// Frames stepped so far (the next to step).
    stepped: u64,
    sim: Sim,
    /// Drawn in the frame being rendered.
    used: bool,
}

/// The renderer's frame-by-frame effects, by effect and the buffer they draw on (`K`).
#[derive(Debug, Clone)]
pub(crate) struct Sims<K> {
    kept: HashMap<K, Kept>,
}

impl<K> Default for Sims<K> {
    fn default() -> Self {
        Self { kept: HashMap::new() }
    }
}

impl<K: Hash + Eq> Sims<K> {
    /// `effect` (as written, curves and all) at `time` on `canvas`: its state moved on from
    /// where it was kept, or worked out from its first frame when it was kept for another effect,
    /// grid, frame time, or a later frame. `None` for an effect that isn't worked out frame by
    /// frame.
    pub fn shader(&mut self, key: K, effect: &Effect, time: &EffectTime, canvas: Canvas) -> Option<Shader> {
        let first = effect.at(effect.start_ms);
        let first = first.params.sanitized();
        if !simulated(&first) {
            return None;
        }
        let frame_ms = time.frame_ms.max(1);
        let cx = Context {
            seed: effect.id.seed(),
            colors: Colors::new(&effect.palette.colors),
            canvas,
            frame_ms,
            first_frame: effect.start_ms / u64::from(frame_ms),
        };
        let frame = time.frame();
        let kept = match self.kept.entry(key) {
            std::collections::hash_map::Entry::Occupied(slot) => {
                let kept = slot.into_mut();
                let usable = kept.stepped <= frame + 1
                    && kept.canvas == canvas
                    && kept.frame_ms == frame_ms
                    && kept.effect == *effect;
                if !usable {
                    *kept = Kept {
                        effect: effect.clone(),
                        canvas,
                        frame_ms,
                        stepped: 0,
                        sim: Sim::start(&first, &cx)?,
                        used: false,
                    };
                }
                kept
            }
            std::collections::hash_map::Entry::Vacant(slot) => slot.insert(Kept {
                effect: effect.clone(),
                canvas,
                frame_ms,
                stepped: 0,
                sim: Sim::start(&first, &cx)?,
                used: false,
            }),
        };
        while kept.stepped <= frame {
            let at = effect.at(effect.start_ms + kept.stepped * u64::from(frame_ms));
            kept.sim.step(&at.params.sanitized(), kept.stepped, &cx);
            kept.stepped += 1;
        }
        kept.used = true;
        let now = effect.at(effect.start_ms + time.elapsed_ms);
        Some(kept.sim.shader(&now.params.sanitized(), time, &cx))
    }

    /// Forgets the effects not drawn since the last sweep.
    pub fn sweep(&mut self) {
        self.kept
            .retain(|_, kept| std::mem::replace(&mut kept.used, false));
    }
}
