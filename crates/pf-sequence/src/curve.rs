//! Settings that change over an effect (xLights' value curves): a shape going between two values,
//! sampled each frame at the effect's own time (0 at its start, 1 at its end), or following the
//! music or a timing track's marks (see [`CurveInputs`]).

use crate::{Mark, TimingTrack, TimingTrackId};
use serde::{Deserialize, Serialize};

/// The most points a custom curve has (an xLights curve has at most 201 positions, each of which
/// can be a step, so every xLights curve fits).
pub const MAX_CURVE_POINTS: usize = 512;
/// The fewest and most times a curve's shape repeats over its effect.
pub const MIN_CURVE_CYCLES: f32 = 0.1;
pub const MAX_CURVE_CYCLES: f32 = 100.0;

/// How a curve goes between its two values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum CurveShape {
    /// Straight from `from` to `to`.
    #[default]
    Ramp,
    /// From `from` to `to` and back, smoothly.
    Sine,
    /// `from` for half of each cycle, then `to`.
    Square,
    /// From `from` to `to`, then straight back.
    Saw,
    /// Through `points`.
    Custom,
    /// `from` in silence to `to` at the music's loudest, boosted by `gain`.
    Music,
    /// The other way round: `to` in silence, `from` at the loudest.
    InvertedMusic,
    /// Jumps to `to` while the music is louder than `trigger` (0-100), then fades to `from` over
    /// `fade` frames.
    MusicTrigger,
    /// Switches between `from` and `to` at each start and end of a mark on `timingTrack`.
    TimingToggle,
    /// Jumps to `to` at each mark on `timingTrack`, fading to `from` over `fade` frames.
    TimingFade,
    /// Like timingFade, over `fade` percent of the time to the next mark.
    TimingFadeSpan,
}

// The music and timing shapes are xLights' "Music", "Inverted Music", "Music Trigger Fade",
// "Timing Track Toggle", "Timing Track Fade Fixed", and "Timing Track Fade Proportional"
// (`ValueCurve::GetValueAt`), read at the effect's frame. Without the music (or while it's
// still being worked out), or without the track, they sit halfway between their values.

impl CurveShape {
    pub const ALL: [CurveShape; 11] = [
        CurveShape::Ramp,
        CurveShape::Sine,
        CurveShape::Square,
        CurveShape::Saw,
        CurveShape::Custom,
        CurveShape::Music,
        CurveShape::InvertedMusic,
        CurveShape::MusicTrigger,
        CurveShape::TimingToggle,
        CurveShape::TimingFade,
        CurveShape::TimingFadeSpan,
    ];

    /// Whether the shape follows the music.
    pub fn follows_music(self) -> bool {
        matches!(
            self,
            CurveShape::Music | CurveShape::InvertedMusic | CurveShape::MusicTrigger
        )
    }

    /// Whether the shape follows a timing track's marks.
    pub fn follows_marks(self) -> bool {
        matches!(
            self,
            CurveShape::TimingToggle | CurveShape::TimingFade | CurveShape::TimingFadeSpan
        )
    }
}

/// The most frames (or percent) a music or timing fade lasts.
pub const MAX_CURVE_FADE: f32 = 1000.0;

/// What curves that follow the music or a timing track read while an effect plays. The default
/// has neither: such curves sit halfway between their values.
#[derive(Clone, Copy, Default)]
pub struct CurveInputs<'a> {
    /// The music's peak in frame `n` (0–1, xLights' `FrameData::max`); `None` without music.
    pub peak: Option<&'a dyn Fn(u64) -> f32>,
    /// The sequence's frame time (50 ms when 0).
    pub frame_ms: u32,
    /// The sequence's timing tracks.
    pub tracks: &'a [TimingTrack],
}

impl CurveInputs<'_> {
    fn frame_ms(&self) -> u64 {
        u64::from(if self.frame_ms == 0 { 50 } else { self.frame_ms })
    }

    fn marks(&self, track: Option<TimingTrackId>) -> Option<&[Mark]> {
        let track = track?;
        self.tracks
            .iter()
            .find(|t| t.id == track)
            .map(|t| t.marks.as_slice())
    }
}

/// A setting that changes over its effect.
// The value at time `t` is `from + (to - from) * level(t)`, where the shape gives `level` (0 to
// 1): a ramp is `t`; sine, square, and saw repeat `cycles` times; custom goes straight from point
// to point (`[t, level]`, sorted by `t`), holds the first and last levels before and after them,
// and two points at one time make a step (the later one counts from that time on).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct Curve {
    pub shape: CurveShape,
    pub from: f32,
    pub to: f32,
    /// Repeats over the effect (sine, square, saw).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub cycles: f32,
    /// Custom: `[time 0-1, level 0-1]` pairs; level 0 is `from` and 1 is `to`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<[f32; 2]>,
    /// music, invertedMusic: -100 to 100 (%).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub gain: f32,
    /// musicTrigger: 0-100.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub trigger: f32,
    /// musicTrigger, timingFade: frames; timingFadeSpan: %.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub fade: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing_track: Option<TimingTrackId>,
}

fn one() -> f32 {
    1.0
}

fn is_one(v: &f32) -> bool {
    *v == 1.0
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

/// Where an effect is when a curve is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurveTime {
    pub start_ms: u64,
    pub end_ms: u64,
    pub t_ms: u64,
}

impl CurveTime {
    /// Where the effect is in its own time: 0 at its start, approaching 1 at its end.
    pub fn progress(&self) -> f32 {
        let length = self.end_ms.saturating_sub(self.start_ms).max(1);
        let elapsed = self.t_ms.saturating_sub(self.start_ms);
        (elapsed as f64 / length as f64).clamp(0.0, 1.0) as f32
    }
}

impl Curve {
    /// A straight ramp from `from` to `to`.
    pub fn ramp(from: f32, to: f32) -> Self {
        Self::shaped(CurveShape::Ramp, from, to, 1.0)
    }

    /// `shape` between `from` and `to`, repeating `cycles` times.
    pub fn shaped(shape: CurveShape, from: f32, to: f32, cycles: f32) -> Self {
        Self {
            shape,
            from,
            to,
            cycles,
            points: Vec::new(),
            gain: 0.0,
            trigger: 0.0,
            fade: 0.0,
            timing_track: None,
        }
    }

    /// From `from` in silence to `to` at the music's loudest (the other way round when
    /// `inverted`), the loudness boosted by `gain` percent.
    pub fn music(from: f32, to: f32, gain: f32, inverted: bool) -> Self {
        let shape = if inverted {
            CurveShape::InvertedMusic
        } else {
            CurveShape::Music
        };
        Self {
            gain,
            ..Self::shaped(shape, from, to, 1.0)
        }
    }

    /// Through `points` (`[t, level]`), level 0 being `from` and 1 `to`.
    pub fn custom(from: f32, to: f32, points: Vec<[f32; 2]>) -> Self {
        Self {
            points,
            ..Self::shaped(CurveShape::Custom, from, to, 1.0)
        }
    }

    /// Whether the curve follows the music or a timing track (and so needs [`CurveInputs`]).
    pub fn is_driven(&self) -> bool {
        self.shape.follows_music() || self.shape.follows_marks()
    }

    /// Where the curve is between `from` (0) and `to` (1) at `at`, reading the music and timing
    /// tracks from `inputs` (halfway when it follows what isn't there).
    pub fn level_in(&self, at: CurveTime, inputs: &CurveInputs) -> f32 {
        let frame_ms = inputs.frame_ms();
        let level = match self.shape {
            CurveShape::Music | CurveShape::InvertedMusic => inputs.peak.map(|peak| {
                let f = gained(peak(at.t_ms / frame_ms), self.gain);
                if self.shape == CurveShape::InvertedMusic {
                    1.0 - f
                } else {
                    f
                }
            }),
            CurveShape::MusicTrigger => inputs.peak.map(|peak| self.trigger_fade(at, frame_ms, peak)),
            CurveShape::TimingToggle | CurveShape::TimingFade | CurveShape::TimingFadeSpan => inputs
                .marks(self.timing_track)
                .map(|marks| self.marks_level(at, frame_ms, marks)),
            _ => return self.level_at(at.progress()),
        };
        level.unwrap_or(0.5)
    }

    /// The setting's value at `at` (see [`Curve::level_in`]).
    pub fn value_in(&self, at: CurveTime, inputs: &CurveInputs) -> f32 {
        self.from + (self.to - self.from) * self.level_in(at, inputs)
    }

    /// xLights' "Music Trigger Fade": the curve it builds across the effect from the music (a
    /// point every 200th of the effect, or every frame when that's shorter), read at `at`.
    fn trigger_fade(&self, at: CurveTime, frame_ms: u64, peak: &dyn Fn(u64) -> f32) -> f32 {
        let length = at.end_ms.saturating_sub(at.start_ms);
        if length == 0 {
            return 0.0;
        }
        let grid = |x: f64| ((x * 200.0).round() / 200.0) as f32;
        let x_of = |ms: i64| grid(ms as f64 / length as f64);
        let per_point = ((length as f32 / 200.0) as u64).max(frame_ms);
        // Falls a fade's share a point; a curve going down instead of up drops straight away.
        let step = if self.to > self.from {
            1.0 / self.fade.max(0.0)
        } else {
            1.0
        };
        let step = if step.is_finite() && step > 0.0 { step } else { 1.0 };
        let query = at.progress();
        let mut points: Vec<(f32, f32)> = vec![(0.0, 0.0)];
        let mut running = 0.0f32;
        let mut time = at.start_ms;
        // Points past the one after `query` don't change the reading.
        while time < at.end_ms && points.last().is_none_or(|p| p.0 <= query) {
            let rel = (time - at.start_ms) as i64;
            let (x, prex) = (x_of(rel), x_of(rel - per_point as i64));
            let mut loudest = 0.0f32;
            let mut ms = time;
            while ms < time + per_point {
                loudest = loudest.max(peak((ms + frame_ms) / frame_ms));
                ms += frame_ms;
            }
            if loudest * 100.0 > self.trigger {
                if time == at.start_ms {
                    running = 1.0;
                    if let Some(last) = points.last_mut() {
                        last.1 = running;
                    }
                } else if running != 1.0 {
                    points.push((x, running));
                    running = 1.0;
                    points.push((x, running));
                }
            } else if running <= 0.0 {
                running = 0.0;
            } else {
                if running == 1.0 && points.last().is_some_and(|p| p.0 < prex || p.1 != running) {
                    points.push((prex, running));
                }
                running -= step;
                if running <= 0.0 {
                    running = 0.0;
                    points.push((x, running));
                }
            }
            time += per_point;
        }
        if time >= at.end_ms {
            points.push((1.0, running));
        }
        // Read as xLights reads a curve's points: straight from one to the next.
        let next = points[1..].iter().position(|p| p.0 >= query).map(|i| i + 1);
        match next {
            None => points.last().map_or(0.0, |p| p.1),
            Some(i) => {
                let (a, b) = (points[i - 1], points[i]);
                if b.0 == a.0 || b.0 == query {
                    b.1
                } else {
                    a.1 + (b.1 - a.1) * (query - a.0) / (b.0 - a.0)
                }
            }
        }
    }

    /// xLights' timing track curves, at `at` on a track with `marks`.
    fn marks_level(&self, at: CurveTime, frame_ms: u64, marks: &[Mark]) -> f32 {
        let time = at.t_ms;
        // The last mark starting at or before `t`, and the first starting after it.
        let prior = |t: u64| marks.iter().rev().find(|m| m.start_ms <= t).map(|m| m.start_ms);
        let after = |t: u64| marks.iter().find(|m| m.start_ms > t).map(|m| m.start_ms);
        // The end of the mark at `t` (its end included), else the next start.
        let next_edge = |t: u64| {
            marks
                .iter()
                .find(|m| m.start_ms <= t && t <= m.end_ms)
                .map(|m| m.end_ms)
                .or_else(|| after(t))
        };
        let fade = |since: u64, frames: i64| {
            let frame = (since / frame_ms) as i64;
            if frame < frames {
                (frames - frame) as f32 / frames as f32
            } else {
                0.0
            }
        };
        match self.shape {
            CurveShape::TimingToggle => {
                let mut up = false;
                let mut next = marks
                    .iter()
                    .find(|m| m.start_ms >= at.start_ms)
                    .map(|m| m.start_ms);
                while let Some(edge) = next.filter(|&e| e <= time) {
                    up = !up;
                    next = next_edge(edge + 1);
                }
                if up { 1.0 } else { 0.0 }
            }
            CurveShape::TimingFade => match prior(time) {
                Some(p) => fade(time - p, self.fade.round() as i64),
                None => 0.0,
            },
            _ => match (prior(time), next_edge(time + 1)) {
                (Some(p), Some(n)) => {
                    let frames = ((n.saturating_sub(p) / frame_ms) as i64 * self.fade.round() as i64) / 100;
                    fade(time - p, frames)
                }
                _ => 0.0,
            },
        }
    }

    /// Where the curve is between `from` (0) and `to` (1) at time `t` (0–1 over the effect). A
    /// curve that follows the music or a timing track is halfway (see [`Curve::level_in`]).
    pub fn level_at(&self, t: f32) -> f32 {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        let cycles = if self.cycles.is_finite() {
            self.cycles.clamp(MIN_CURVE_CYCLES, MAX_CURVE_CYCLES)
        } else {
            1.0
        };
        let phase = || (f64::from(t) * f64::from(cycles)).fract() as f32;
        match self.shape {
            CurveShape::Ramp => t,
            CurveShape::Sine => {
                let x = f64::from(t) * f64::from(cycles) * std::f64::consts::TAU;
                (0.5 - 0.5 * x.cos()) as f32
            }
            CurveShape::Square => {
                if phase() < 0.5 {
                    0.0
                } else {
                    1.0
                }
            }
            CurveShape::Saw => phase(),
            CurveShape::Custom => custom_level(&self.points, t),
            _ => 0.5,
        }
    }

    /// The setting's value at time `t` (0–1 over the effect).
    pub fn value_at(&self, t: f32) -> f32 {
        self.from + (self.to - self.from) * self.level_at(t)
    }

    /// Pulls the curve into a setting's range (`min` to `max`): values clamped (NaN becomes
    /// `min`), cycles kept to what's allowed, and points sorted, inside 0–1, and at most
    /// [`MAX_CURVE_POINTS`].
    pub fn sanitize(&mut self, min: f32, max: f32) {
        let fit = |v: f32| if v.is_nan() { min } else { v.clamp(min, max) };
        self.from = fit(self.from);
        self.to = fit(self.to);
        self.cycles = if self.cycles.is_nan() {
            1.0
        } else {
            self.cycles.clamp(MIN_CURVE_CYCLES, MAX_CURVE_CYCLES)
        };
        self.points.retain(|p| p.iter().all(|v| !v.is_nan()));
        for p in &mut self.points {
            *p = p.map(|v| v.clamp(0.0, 1.0));
        }
        // Stable, so the two points of a step keep their order.
        self.points.sort_by(|a, b| a[0].total_cmp(&b[0]));
        self.points.truncate(MAX_CURVE_POINTS);
        let within = |v: f32, lo: f32, hi: f32| if v.is_nan() { 0.0 } else { v.clamp(lo, hi) };
        self.gain = within(self.gain, -100.0, 100.0);
        self.trigger = within(self.trigger, 0.0, 100.0);
        self.fade = within(self.fade, 0.0, MAX_CURVE_FADE);
    }

    /// Why the curve doesn't fit a setting's range (`min` to `max`), if it doesn't.
    pub fn problem(&self, min: f32, max: f32) -> Option<String> {
        for (name, v) in [("starts", self.from), ("ends", self.to)] {
            if !v.is_finite() || !(min..=max).contains(&v) {
                return Some(format!("{name} at {v}; use {min} to {max}"));
            }
        }
        if !(MIN_CURVE_CYCLES..=MAX_CURVE_CYCLES).contains(&self.cycles) {
            return Some(format!(
                "repeats {} times; use {MIN_CURVE_CYCLES} to {MAX_CURVE_CYCLES}",
                self.cycles
            ));
        }
        if self.points.len() > MAX_CURVE_POINTS {
            return Some(format!(
                "has {} points; use at most {MAX_CURVE_POINTS}",
                self.points.len()
            ));
        }
        if self.points.iter().flatten().any(|v| !(0.0..=1.0).contains(v)) {
            return Some("has a point outside 0 to 1".into());
        }
        if self.points.windows(2).any(|w| w[1][0] < w[0][0]) {
            return Some("has points out of time order".into());
        }
        for (name, v, lo, hi) in [
            ("gain", self.gain, -100.0, 100.0),
            ("trigger", self.trigger, 0.0, 100.0),
            ("fade", self.fade, 0.0, MAX_CURVE_FADE),
        ] {
            if !v.is_finite() || !(lo..=hi).contains(&v) {
                return Some(format!("has a {name} of {v}; use {lo} to {hi}"));
            }
        }
        None
    }
}

/// xLights' `ApplyGain`: the level boosted by `gain` percent, at most 1.
fn gained(level: f32, gain: f32) -> f32 {
    ((100.0 + gain) * level / 100.0).min(1.0)
}

/// A custom curve's level at `t`: straight between points, held before the first and after the
/// last; at a step (points sharing a time) the later point counts from that time on. No points:
/// level 0.
fn custom_level(points: &[[f32; 2]], t: f32) -> f32 {
    let Some(first) = points.first() else {
        return 0.0;
    };
    // The last point at or before `t`.
    let after = points.partition_point(|p| p[0] <= t);
    if after == 0 {
        return first[1];
    }
    let a = points[after - 1];
    let Some(b) = points.get(after) else {
        return a[1];
    };
    let span = b[0] - a[0];
    if span <= 0.0 {
        return a[1];
    }
    a[1] + (b[1] - a[1]) * ((t - a[0]) / span)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn shapes_go_between_from_and_to() {
        let ramp = Curve::ramp(10.0, 20.0);
        assert_eq!([0.0, 0.25, 1.0].map(|t| ramp.value_at(t)), [10.0, 12.5, 20.0]);
        let down = Curve::ramp(1.0, 0.0);
        assert!(close(down.value_at(0.75), 0.25));

        let sine = Curve::shaped(CurveShape::Sine, 0.0, 1.0, 2.0);
        assert!(close(sine.value_at(0.0), 0.0));
        assert!(close(sine.value_at(0.25), 1.0), "the top halfway through a cycle");
        assert!(close(sine.value_at(0.5), 0.0));
        assert!(close(sine.value_at(0.125), 0.5));

        let square = Curve::shaped(CurveShape::Square, 2.0, 8.0, 2.0);
        assert_eq!(
            [0.0, 0.2, 0.25, 0.49, 0.5, 0.8].map(|t| square.value_at(t)),
            [2.0, 2.0, 8.0, 8.0, 2.0, 8.0]
        );

        let saw = Curve::shaped(CurveShape::Saw, 0.0, 10.0, 4.0);
        assert!(close(saw.value_at(0.125), 5.0));
        assert!(close(saw.value_at(0.25), 0.0), "back down at each cycle");
    }

    #[test]
    fn custom_curves_go_point_to_point_and_step_where_points_share_a_time() {
        let c = Curve::custom(0.0, 100.0, vec![[0.2, 0.0], [0.5, 1.0], [0.5, 0.25], [1.0, 0.25]]);
        assert_eq!(c.value_at(0.0), 0.0, "held before the first point");
        assert!(close(c.value_at(0.35), 50.0));
        assert!(c.value_at(0.4999) > 99.0);
        assert_eq!(c.value_at(0.5), 25.0, "the later point counts at a step");
        assert_eq!(c.value_at(0.9), 25.0);
        assert_eq!(Curve::custom(3.0, 9.0, vec![]).value_at(0.5), 3.0);
    }

    #[test]
    fn sanitize_fits_a_curve_to_its_setting() {
        let mut c = Curve {
            points: vec![[0.5, 2.0], [f32::NAN, 0.0], [-1.0, 0.5]],
            ..Curve::shaped(CurveShape::Custom, f32::NAN, 70.0, 0.0)
        };
        assert!(c.problem(0.0, 50.0).is_some());
        c.sanitize(0.0, 50.0);
        assert_eq!((c.from, c.to, c.cycles), (0.0, 50.0, MIN_CURVE_CYCLES));
        assert_eq!(c.points, vec![[0.0, 0.5], [0.5, 1.0]]);
        assert_eq!(c.problem(0.0, 50.0), None);
    }

    #[test]
    fn curves_read_and_write_compactly() {
        let json = serde_json::to_value(Curve::ramp(0.0, 1.0)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "shape": "ramp", "from": 0.0, "to": 1.0 })
        );
        let c: Curve =
            serde_json::from_str(r#"{ "shape": "custom", "from": 1, "to": 3, "points": [[0, 0], [1, 1]] }"#)
                .unwrap();
        assert_eq!(c.cycles, 1.0);
        assert_eq!(c.value_at(0.5), 2.0);
        let text = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<Curve>(&text).unwrap(), c);
        let music: Curve = serde_json::from_str(
            r#"{ "shape": "musicTrigger", "from": 0, "to": 1, "trigger": 50, "fade": 4 }"#,
        )
        .unwrap();
        assert_eq!((music.trigger, music.fade, music.gain), (50.0, 4.0, 0.0));
        let json = serde_json::to_value(Curve::music(0.0, 1.0, 0.0, true)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "shape": "invertedMusic", "from": 0.0, "to": 1.0 })
        );
    }

    /// An effect from 0 to 1000 ms at 25 ms frames.
    fn at(t_ms: u64) -> CurveTime {
        CurveTime {
            start_ms: 0,
            end_ms: 1000,
            t_ms,
        }
    }

    fn inputs<'a>(peak: &'a dyn Fn(u64) -> f32, tracks: &'a [TimingTrack]) -> CurveInputs<'a> {
        CurveInputs {
            peak: Some(peak),
            frame_ms: 25,
            tracks,
        }
    }

    #[test]
    fn music_curves_follow_the_peak_with_gain_as_xlights_does() {
        // xLights: "Type=Music|Min=0|Max=200|P1=20|P2=180|P3=50": P1 and P2 are 0.1 and 0.9 of
        // the range; at a peak of 0.4 the gain makes 0.6, so 0.1 + 0.6 × 0.8 = 0.58 → 116.
        let peak = |frame: u64| if frame == 4 { 0.4 } else { 0.8 };
        let cx = inputs(&peak, &[]);
        let c = Curve::music(20.0, 180.0, 50.0, false);
        assert!((c.value_in(at(100), &cx) - 116.0).abs() < 1e-4);
        assert_eq!(
            c.value_in(at(124), &cx),
            c.value_in(at(100), &cx),
            "the frame's peak"
        );
        assert_eq!(
            c.value_in(at(125), &cx),
            180.0,
            "boosted past the top, held there"
        );
        let inverted = Curve::music(20.0, 180.0, 0.0, true);
        assert!((inverted.value_in(at(100), &cx) - (20.0 + 160.0 * 0.6)).abs() < 1e-4);
        // Without the music: halfway, as before it's worked out.
        assert_eq!(c.value_in(at(100), &CurveInputs::default()), 100.0);
        assert_eq!(c.value_at(0.3), 100.0);
        assert!(c.is_driven() && !Curve::ramp(0.0, 1.0).is_driven());
    }

    #[test]
    fn music_trigger_fades_jump_and_fall_as_xlights_builds_them() {
        // Loud only in frame 5. Points come every frame (a 200th of the effect is 5 ms): xLights
        // reads frame (time + 25) / 25 for the point at `time`, so the point at 100 ms jumps:
        // (0.1, 0) then (0.1, 1), falls a quarter a point, and reaches 0 at (0.2, 0).
        let peak = |frame: u64| if frame == 5 { 0.9 } else { 0.1 };
        let cx = inputs(&peak, &[]);
        let c = Curve {
            trigger: 50.0,
            fade: 4.0,
            ..Curve::shaped(CurveShape::MusicTrigger, 10.0, 20.0, 1.0)
        };
        let level = |t: u64| c.level_in(at(t), &cx);
        assert_eq!(level(50), 0.0);
        assert_eq!(level(100), 0.0, "the first of the two points at the jump");
        assert!((level(110) - 0.9).abs() < 1e-5);
        assert!((level(150) - 0.5).abs() < 1e-5);
        assert_eq!(level(200), 0.0);
        assert_eq!(level(900), 0.0);
        assert!((c.value_in(at(150), &cx) - 15.0).abs() < 1e-4);
        // Loud throughout: up from the start.
        let loud = |_: u64| 1.0;
        assert_eq!(c.level_in(at(500), &inputs(&loud, &[])), 1.0);
    }

    #[test]
    fn timing_curves_follow_the_marks() {
        let track = TimingTrack::new(
            "Beats",
            crate::TimingKind::Beats,
            vec![Mark::new(100, 200, ""), Mark::new(300, 400, "")],
        );
        let tracks = [track];
        let quiet = |_: u64| 0.0;
        let cx = inputs(&quiet, &tracks);
        let on = |shape: CurveShape, fade: f32| Curve {
            fade,
            timing_track: Some(tracks[0].id),
            ..Curve::shaped(shape, 0.0, 1.0, 1.0)
        };
        // Toggles at each start and end of a mark.
        let toggle = on(CurveShape::TimingToggle, 0.0);
        let read = |c: &Curve, t: u64| c.level_in(at(t), &cx);
        assert_eq!(
            [50, 150, 200, 250, 350, 450].map(|t| read(&toggle, t)),
            [0.0, 1.0, 0.0, 0.0, 1.0, 0.0]
        );
        // Jumps at each mark and fades over 4 frames.
        let fade = on(CurveShape::TimingFade, 4.0);
        assert_eq!(
            [50, 100, 150, 200, 325].map(|t| read(&fade, t)),
            [0.0, 1.0, 0.5, 0.0, 0.75]
        );
        // Over half the time to the next edge (the mark's end): 100 ms is 4 frames, so 2.
        let span = on(CurveShape::TimingFadeSpan, 50.0);
        assert_eq!([100, 125, 150].map(|t| read(&span, t)), [1.0, 0.5, 0.0]);
        // A track that isn't there: halfway.
        let missing = Curve {
            timing_track: Some(TimingTrackId::new()),
            ..toggle.clone()
        };
        assert_eq!(read(&missing, 150), 0.5);
    }
}
