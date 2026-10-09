//! Pulse: the target breathing between a lowest and a highest brightness, PixelFlow's own.
//!
//! - **On marks:** each mark of a timing track starts a pulse that lasts until the next mark (or
//!   the mark's own end, for the last), shaped smooth (a cosine, brightest on the mark), as a hit
//!   and fade, on and off, or as a heartbeat's two beats. Each mark takes the next palette color.
//!   Without a timing track it pulses twice a second; with one the sequence doesn't have, it
//!   stays at its lowest.
//! - **With the music:** its level, bass, or new sounds, smoothed by the attack (how fast it
//!   rises) and release (how fast it falls). The smoothing is worked out over the frames just
//!   before the one drawn, so any frame renders on its own. Without the music, it stays at its
//!   lowest, as in silence.

use crate::audio::{Audio, RenderContext};
use crate::color::{Colors, Rgba, unit};
use crate::effects::{EffectTime, Shade};
use crate::geometry::Pixel;
use pf_sequence::{Mark, PulseParams, PulseShape, PulseSource};
use std::f32::consts::TAU;

/// The pulse length without a timing track.
const FREE_PULSE_MS: u64 = 500;
/// The smoothing is worked out over this many attacks plus releases before the frame drawn
/// (what came earlier has faded below a hundredth).
const SETTLE: f32 = 5.0;
/// Most frames the smoothing looks back over.
const MAX_SETTLE_FRAMES: u64 = 2_000;

/// How bright (0–1) a pulse of `shape` is `phase` of the way to the next mark.
pub(crate) fn pulse_shape(shape: PulseShape, phase: f32) -> f32 {
    if !(0.0..1.0).contains(&phase) {
        return 0.0;
    }
    match shape {
        PulseShape::Sine => 0.5 + 0.5 * (TAU * phase).cos(),
        PulseShape::Saw => 1.0 - phase,
        PulseShape::Square => {
            if phase < 0.5 {
                1.0
            } else {
                0.0
            }
        }
        PulseShape::Heartbeat => {
            let beat = |at: f32, width: f32| (-((phase - at) / width).powi(2)).exp();
            beat(0.0, 0.08).max(0.6 * beat(0.28, 0.08))
        }
    }
}

/// Where `t_ms` is in the pulses of `marks`: the mark's number and how far it is to the next
/// (`None` before the first mark).
fn mark_phase(marks: &[Mark], t_ms: u64) -> Option<(usize, f32)> {
    let i = marks.partition_point(|m| m.start_ms <= t_ms).checked_sub(1)?;
    let mark = &marks[i];
    let until = marks
        .get(i + 1)
        .map_or(mark.end_ms, |next| next.start_ms)
        .max(mark.start_ms + 1);
    Some((i, (t_ms - mark.start_ms) as f32 / (until - mark.start_ms) as f32))
}

/// The music `source` follows, smoothed by the attack and release, at the frame playing at
/// `time` (from the effect's first frame at most).
pub(crate) fn follow(p: &PulseParams, audio: Audio, time: &EffectTime) -> f32 {
    let frame_ms = time.frame_ms.max(1) as f32;
    let read = |frame: u64| -> f32 {
        unit(match p.source {
            PulseSource::Bass => audio.bass(frame),
            PulseSource::Onsets => audio.onset(frame),
            PulseSource::Level | PulseSource::Marks => audio.level(frame),
        })
    };
    let rate = |ms: f32| {
        if ms <= 0.0 {
            1.0
        } else {
            1.0 - (-frame_ms / ms).exp()
        }
    };
    let (rise, fall) = (rate(p.attack), rate(p.release));
    let now = audio.frame_at(time.start_ms + time.elapsed_ms);
    let first = audio.frame_at(time.start_ms);
    let back = (((p.attack + p.release) * SETTLE / frame_ms).ceil() as u64).min(MAX_SETTLE_FRAMES);
    let from = now.saturating_sub(back).max(first);
    let mut level = read(from);
    for frame in from + 1..=now {
        let x = read(frame);
        level += (x - level) * if x > level { rise } else { fall };
    }
    level
}

pub struct Pulse {
    color: [f32; 3],
    level: f32,
}

impl Pulse {
    pub fn new(p: &PulseParams, time: &EffectTime, colors: Colors, cx: &RenderContext) -> Self {
        let (low, high) = (unit(p.min), unit(p.max));
        let t_ms = time.start_ms + time.elapsed_ms;
        let (color, amount) = match p.source {
            PulseSource::Marks => match (p.timing_track, cx.marks(p.timing_track)) {
                (None, _) => {
                    let pulse = time.elapsed_ms / FREE_PULSE_MS;
                    let phase = (time.elapsed_ms % FREE_PULSE_MS) as f32 / FREE_PULSE_MS as f32;
                    (colors.get(pulse), pulse_shape(p.shape, phase))
                }
                (Some(_), Some(marks)) => match mark_phase(marks, t_ms) {
                    Some((i, phase)) => (colors.get(i as u64), pulse_shape(p.shape, phase)),
                    None => (colors.get(0), 0.0),
                },
                (Some(_), None) => (colors.get(0), 0.0),
            },
            _ => (
                colors.get(0),
                cx.audio.map_or(0.0, |audio| follow(p, audio, time)),
            ),
        };
        Self {
            color,
            level: low + (high - low) * amount,
        }
    }

    /// The brightness this frame (0–1); for tests.
    pub fn level(&self) -> f32 {
        self.level
    }
}

impl Shade for Pulse {
    #[inline]
    fn shade(&self, _px: &Pixel) -> Rgba {
        if self.level <= 0.0 {
            return Rgba::CLEAR;
        }
        Rgba::with_alpha(self.color, self.level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_peak_on_the_mark() {
        for shape in [
            PulseShape::Sine,
            PulseShape::Saw,
            PulseShape::Square,
            PulseShape::Heartbeat,
        ] {
            assert!((pulse_shape(shape, 0.0) - 1.0).abs() < 1e-6, "{shape:?}");
            assert_eq!(pulse_shape(shape, 1.0), 0.0, "{shape:?}: past the pulse");
        }
        assert!(
            pulse_shape(PulseShape::Sine, 0.5) < 1e-6,
            "breathes out between marks"
        );
        assert!(pulse_shape(PulseShape::Square, 0.75) == 0.0);
        // A heartbeat's second, softer beat.
        let dub = pulse_shape(PulseShape::Heartbeat, 0.28);
        assert!(dub > 0.55 && dub < 0.65, "{dub}");
        assert!(
            pulse_shape(PulseShape::Heartbeat, 0.15) < 0.3,
            "{}",
            pulse_shape(PulseShape::Heartbeat, 0.15)
        );
    }

    #[test]
    fn marks_pulse_until_the_next_mark() {
        let marks = [
            Mark::new(1000, 1100, ""),
            Mark::new(1500, 1600, ""),
            Mark::new(3000, 3400, ""),
        ];
        assert_eq!(mark_phase(&marks, 900), None);
        let (i, phase) = mark_phase(&marks, 1250).unwrap();
        assert_eq!(i, 0);
        assert!((phase - 0.5).abs() < 1e-6);
        let (i, phase) = mark_phase(&marks, 3200).unwrap();
        assert_eq!(i, 2, "the last mark pulses over its own length");
        assert!((phase - 0.5).abs() < 1e-6);
        assert!(mark_phase(&marks, 3500).unwrap().1 >= 1.0);
    }
}
