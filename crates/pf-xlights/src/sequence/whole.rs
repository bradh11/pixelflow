//! Settings xLights shows in whole numbers where PixelFlow's effect honours fractions.
//!
//! PixelFlow's Pinwheel draws the wheel its settings describe, fractions and all, so a setting
//! on a curve glides. In xLights' frames the same wheel moves a whole unit at a time. A still,
//! one-armed wheel whose start angle jumps to 31 degrees on each mark of a timing track and
//! falls back over ten frames, 3.1 degrees a frame, is nearest those frames, pixel for pixel,
//! with the fraction dropped (31, 27, 24, 21 … 3, 0): nearer than with it kept, rounded, or
//! raised to the next whole number. The center of a wheel that slides from one side of the
//! middle to the other is nearer them with its fraction dropped too, and below the middle, where
//! dropping it and rounding down part ways, it is dropping it (toward zero) that fits.
//!
//! An import should look the same, and a Pinwheel made in PixelFlow should go on gliding, so
//! the difference is made up here, in what the import hands the effect: a curve on such a
//! setting comes across as a curve through the whole numbers xLights shows, one for each frame
//! the effect plays on. A curve is read frame by frame, so nothing has to hold between them,
//! and frames in a straight line need only its two ends.
//!
//! That makes a curve of plain points of one that followed a timing track: it is read at the
//! marks the sequence came with, and no longer moves when they do. Two kinds of curve stay as
//! they are, and glide: one that follows the music, which has no values until the music is
//! read, and one whose whole numbers take more points than a curve has room for (it turns a
//! corner on most frames of a long effect).

use pf_sequence::{Curve, CurveInputs, CurveTime, Effect, EffectParams, MAX_CURVE_POINTS, TimingTrack};

/// The Pinwheel settings xLights' frames show in whole numbers, by their keys here: the start
/// angle and the center. A twist on a curve was measured too, and its frames don't say either
/// way; no measured sequence has a curve on thickness, speed, or arm length.
const PINWHEEL: [&str; 3] = ["offset", "centerX", "centerY"];

/// How much reading of curves one import does for this: a read for each frame of each curve,
/// times the marks on its track for one that follows a timing track (reading a frame of those
/// looks through the marks). A real sequence asks for a small part of it; past it, a file made
/// to ask for hours of frames on thousands of curves keeps the curves it came with.
pub(super) const MOST_READS: u64 = 50_000_000;

/// How near a whole number counts as that number, as a share of a curve's range (and in a
/// setting's own units, for one without a curve). A curve carried over keeps its values to
/// within half this of its range, and one that rests on 20 mustn't be shown at 19 for being
/// read back as 19.999.
const SLACK: f32 = 1e-4;

/// Makes the settings of `effect` that xLights shows in whole numbers whole: the setting itself
/// and, where the importer can work it out, each value its curve gives on a frame. `reads` is
/// what's left of the import's [`MOST_READS`].
pub(super) fn whole_settings(effect: &mut Effect, frame_ms: u32, tracks: &[TimingTrack], reads: &mut u64) {
    if !matches!(effect.params, EffectParams::Pinwheel(_)) {
        return;
    }
    for key in PINWHEEL {
        if let Some(value) = effect.params.number(key) {
            effect.params.set_number(key, whole(value, SLACK));
        }
        let Some(curve) = effect.curves.get(key) else {
            continue;
        };
        match stepped(curve, effect.start_ms, effect.end_ms, frame_ms, tracks, reads) {
            Stepped::AsItIs => {}
            Stepped::Still(value) => {
                effect.curves.remove(key);
                effect.params.set_number(key, value);
            }
            Stepped::Through(first, curve) => {
                effect.curves.insert(key.to_string(), curve);
                effect.params.set_number(key, first);
            }
        }
    }
}

/// `value` without its fraction (toward zero, as xLights' frames show it), or the whole number
/// it is within `slack` of.
fn whole(value: f32, slack: f32) -> f32 {
    let nearest = value.round();
    let whole = if (value - nearest).abs() <= slack {
        nearest
    } else {
        value.trunc()
    };
    // Adding zero keeps a small negative value from coming out as minus zero.
    whole + 0.0
}

/// A curve as xLights' frames show a whole-number setting on it.
enum Stepped {
    /// Left alone: it's whole already, or can't be worked out here.
    AsItIs,
    /// It never leaves one whole number.
    Still(f32),
    /// Its value on the effect's first frame, and the curve through its whole numbers.
    Through(f32, Curve),
}

/// `curve` on an effect from `start_ms` to `end_ms`, read on each frame the effect plays on with
/// the fraction dropped.
fn stepped(
    curve: &Curve,
    start_ms: u64,
    end_ms: u64,
    frame_ms: u32,
    tracks: &[TimingTrack],
    reads: &mut u64,
) -> Stepped {
    if curve.shape.follows_music() {
        return Stepped::AsItIs;
    }
    let frame = u64::from(frame_ms.max(1));
    let first = start_ms.div_ceil(frame) * frame;
    let frames = end_ms.saturating_sub(first).div_ceil(frame);
    let track = tracks.iter().find(|t| Some(t.id) == curve.timing_track);
    // Without its track a curve that follows one sits at its middle, and is left to.
    if curve.shape.follows_marks() && track.is_none() {
        return Stepped::AsItIs;
    }
    let marks = track.map_or(1, |t| t.marks.len().max(1) as u64);
    let Some(left) = reads.checked_sub(frames.saturating_mul(marks)) else {
        return Stepped::AsItIs;
    };
    *reads = left;
    let inputs = CurveInputs {
        peak: None,
        frame_ms,
        tracks: track.map_or(&[][..], std::slice::from_ref),
    };
    let at = |frame_number: u64| CurveTime {
        start_ms,
        end_ms,
        t_ms: first + frame_number * frame,
    };
    let slack = SLACK * (curve.to - curve.from).abs().max(1.0);
    let mut values = Vec::new();
    let mut fractions = false;
    for frame_number in 0..frames {
        let value = curve.value_in(at(frame_number), &inputs);
        if !value.is_finite() {
            return Stepped::AsItIs;
        }
        let whole = whole(value, slack);
        fractions |= (value - whole).abs() > slack;
        values.push(whole);
    }
    let Some(&start) = values.first() else {
        return Stepped::AsItIs;
    };
    if !fractions {
        return Stepped::AsItIs;
    }
    let (low, high) = values
        .iter()
        .fold((start, start), |(low, high), &v| (low.min(v), high.max(v)));
    if low == high {
        return Stepped::Still(start);
    }
    // A point on every frame where the line through the frames turns: a frame in a straight
    // line between its neighbors (level, or stepping evenly) is on the curve without one.
    // Whole numbers and frame counts, so "straight" is exact.
    let mut corners: Vec<(u64, f32)> = Vec::new();
    for (frame_number, &value) in (0u64..).zip(&values) {
        while let [.., a, b] = corners[..]
            && in_line(a, b, (frame_number, value))
        {
            corners.pop();
        }
        corners.push((frame_number, value));
    }
    if corners.len() > MAX_CURVE_POINTS {
        return Stepped::AsItIs;
    }
    let mut points: Vec<[f32; 2]> = Vec::with_capacity(corners.len());
    for (frame_number, value) in corners {
        let time = at(frame_number).progress();
        // Two frames that fall on one moment of a curve can't both be given a value.
        if points.last().is_some_and(|p| p[0] >= time) {
            return Stepped::AsItIs;
        }
        points.push([time, (value - low) / (high - low)]);
    }
    Stepped::Through(start, Curve::custom(low, high, points))
}

/// Whether frame `b` is on the straight line from frame `a` to frame `c` (each a frame's number
/// and its whole value).
fn in_line(a: (u64, f32), b: (u64, f32), c: (u64, f32)) -> bool {
    let (rise_b, rise_c) = (f64::from(b.1 - a.1), f64::from(c.1 - a.1));
    let (run_b, run_c) = ((b.0 - a.0) as f64, (c.0 - a.0) as f64);
    rise_b * run_c == rise_c * run_b
}

#[cfg(test)]
mod tests {
    use super::*;
    use pf_sequence::{CurveShape, EffectKind, Mark, TimingKind};

    const FRAME_MS: u32 = 50;

    fn pinwheel(start_ms: u64, end_ms: u64) -> Effect {
        Effect::new(EffectKind::Pinwheel, start_ms, end_ms)
    }

    /// The setting `key` on each frame the effect plays on.
    fn frames(effect: &Effect, key: &str, tracks: &[TimingTrack]) -> Vec<f32> {
        let inputs = CurveInputs {
            peak: None,
            frame_ms: FRAME_MS,
            tracks,
        };
        (effect.start_ms..effect.end_ms)
            .step_by(FRAME_MS as usize)
            .map(|t| effect.at_with(t, &inputs).params.number(key).unwrap())
            .collect()
    }

    /// Whether the effect's frames show `want`: each a whole number, to within what a number
    /// that size can hold (far less than a wheel can show).
    #[track_caller]
    fn assert_frames(got: &[f32], want: &[f32]) {
        assert_eq!(got.len(), want.len());
        for (i, (got, want)) in got.iter().zip(want).enumerate() {
            assert_eq!(want.fract(), 0.0, "frame {i}");
            assert!((got - want).abs() <= 1e-4, "frame {i}: {got}, not {want}");
        }
    }

    #[test]
    fn a_curve_comes_across_through_the_whole_numbers_it_passes_frame_by_frame() {
        let mut effect = pinwheel(1000, 3000);
        effect.params.set_number("centerX", -2.5);
        effect.curves.insert("centerX".into(), Curve::ramp(-2.5, 7.3));
        let glided = frames(&effect, "centerX", &[]);
        whole_settings(&mut effect, FRAME_MS, &[], &mut { MOST_READS });
        let stepped = frames(&effect, "centerX", &[]);
        // Fractions dropped toward zero, on either side of it.
        let want: Vec<f32> = glided.iter().map(|v| v.trunc()).collect();
        assert_frames(&stepped, &want);
        assert_frames(&stepped[..8], &[-2.0, -2.0, -2.0, -1.0, -1.0, -1.0, -1.0, 0.0]);
        assert_frames(&stepped[36..], &[6.0, 6.0, 6.0, 7.0]);
        assert_eq!(
            effect.params.number("centerX"),
            Some(-2.0),
            "the setting is where it starts"
        );
        let curve = &effect.curves["centerX"];
        assert_eq!(
            (curve.shape, curve.from, curve.to),
            (CurveShape::Custom, -2.0, 7.0)
        );
        // A point where the value changes and one where it last held, none in between.
        assert_eq!(curve.points.len(), 19);
        assert_eq!(curve.problem(-100.0, 100.0), None);
    }

    #[test]
    fn a_curve_on_a_timing_track_is_read_at_its_marks_and_no_longer_follows_them() {
        let track = TimingTrack::new(
            "Beats",
            TimingKind::Custom,
            vec![
                Mark::new(0, 100, ""),
                Mark::new(750, 850, ""),
                Mark::new(1500, 1600, ""),
            ],
        );
        let tracks = [track];
        let mut effect = pinwheel(0, 2250);
        let mut fade = Curve::shaped(CurveShape::TimingFade, 0.0, 31.0, 1.0);
        (fade.fade, fade.timing_track) = (10.0, Some(tracks[0].id));
        effect.curves.insert("offset".into(), fade.clone());
        whole_settings(&mut effect, FRAME_MS, &tracks, &mut { MOST_READS });
        // xLights' frames: 31 on the mark, then down by 3.1 a frame with the fraction dropped.
        let beat = [
            31.0, 27.0, 24.0, 21.0, 18.0, 15.0, 12.0, 9.0, 6.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        ];
        assert_frames(&frames(&effect, "offset", &tracks), &beat.repeat(3));
        let curve = &effect.curves["offset"];
        assert_eq!((curve.shape, curve.timing_track), (CurveShape::Custom, None));
        assert_eq!(effect.params.number("offset"), Some(31.0));
        assert_eq!(curve.problem(0.0, 360.0), None);
        // Four points a mark: on it, a frame on, the foot of the fall, and the frame before the
        // next (the frames down the fall are in a straight line, 3 apart).
        assert_eq!(curve.points.len(), 3 * 4);

        // Its track isn't in the sequence: it sits at its middle, whole, and is left to follow.
        let mut lost = pinwheel(0, 2250);
        lost.params.set_number("offset", 15.5);
        lost.curves.insert("offset".into(), fade.clone());
        whole_settings(&mut lost, FRAME_MS, &[], &mut { MOST_READS });
        assert_eq!(lost.curves["offset"], fade);
        assert_eq!(lost.params.number("offset"), Some(15.0));
    }

    #[test]
    fn curves_that_are_whole_already_stay_put_or_follow_the_music_are_left_or_dropped() {
        // Whole on every frame: nothing to do.
        let mut effect = pinwheel(0, 2000);
        let ramp = Curve::ramp(0.0, 40.0);
        effect.curves.insert("offset".into(), ramp.clone());
        whole_settings(&mut effect, FRAME_MS, &[], &mut { MOST_READS });
        assert_eq!(effect.curves["offset"], ramp);

        // Never leaves one whole number: no curve, and the setting is that number.
        let mut effect = pinwheel(0, 2000);
        effect.curves.insert("centerY".into(), Curve::ramp(-12.8, -12.2));
        whole_settings(&mut effect, FRAME_MS, &[], &mut { MOST_READS });
        assert!(effect.curves.is_empty());
        assert_eq!(effect.params.number("centerY"), Some(-12.0));

        // Follows the music: left to glide with it, from where it sits without it.
        let mut effect = pinwheel(0, 2000);
        let music = Curve::music(0.5, 31.0, 0.0, false);
        effect.params.set_number("offset", 15.75);
        effect.curves.insert("offset".into(), music.clone());
        whole_settings(&mut effect, FRAME_MS, &[], &mut { MOST_READS });
        assert_eq!(effect.curves["offset"], music);
        assert_eq!(effect.params.number("offset"), Some(15.0));
    }

    #[test]
    fn a_curve_with_more_whole_numbers_than_a_curve_has_points_keeps_gliding() {
        // A turn in fifty seconds: 0.36 degrees a frame, the whole number changing on a third
        // of 1000 frames and holding for uneven runs between.
        let mut effect = pinwheel(0, 50_000);
        let ramp = Curve::ramp(0.0, 360.0);
        effect.curves.insert("offset".into(), ramp.clone());
        whole_settings(&mut effect, FRAME_MS, &[], &mut { MOST_READS });
        assert_eq!(effect.curves["offset"], ramp);
        // The same turn in fifteen seconds fits: 1.2 degrees a frame.
        let mut effect = pinwheel(0, 15_000);
        effect.curves.insert("offset".into(), ramp);
        whole_settings(&mut effect, FRAME_MS, &[], &mut { MOST_READS });
        let values = frames(&effect, "offset", &[]);
        let want: Vec<f32> = (0..300).map(|frame| (frame * 6 / 5) as f32).collect();
        assert_frames(&values, &want);
        let points = effect.curves["offset"].points.len();
        assert!(points > 100 && points <= MAX_CURVE_POINTS, "{points}");
    }

    #[test]
    fn an_import_reads_only_so_many_frames_of_curves() {
        // Forty frames of a ramp, and a timing track's three marks looked through on each of
        // forty-five: what's asked for is taken off what's left, and a curve there isn't enough
        // left for stays as it came.
        let ramp = Curve::ramp(-2.5, 7.3);
        let mut effect = pinwheel(0, 2000);
        effect.curves.insert("centerX".into(), ramp.clone());
        let mut reads = 100;
        whole_settings(&mut effect, FRAME_MS, &[], &mut reads);
        assert_ne!(effect.curves["centerX"], ramp);
        assert_eq!(reads, 60);
        let mut next = pinwheel(0, 4000);
        next.curves.insert("centerX".into(), ramp.clone());
        whole_settings(&mut next, FRAME_MS, &[], &mut reads);
        assert_eq!(next.curves["centerX"], ramp);
        assert_eq!(reads, 60);

        let marks = vec![
            Mark::new(0, 100, ""),
            Mark::new(750, 850, ""),
            Mark::new(1500, 1600, ""),
        ];
        let tracks = [TimingTrack::new("Beats", TimingKind::Custom, marks)];
        let mut fade = Curve::shaped(CurveShape::TimingFade, 0.0, 31.0, 1.0);
        (fade.fade, fade.timing_track) = (10.0, Some(tracks[0].id));
        let mut effect = pinwheel(0, 2250);
        effect.curves.insert("offset".into(), fade.clone());
        let mut reads = 134;
        whole_settings(&mut effect, FRAME_MS, &tracks, &mut reads);
        assert_eq!((effect.curves["offset"].clone(), reads), (fade, 134));
        reads = 135;
        whole_settings(&mut effect, FRAME_MS, &tracks, &mut reads);
        assert_eq!((effect.curves["offset"].shape, reads), (CurveShape::Custom, 0));
    }

    #[test]
    fn only_a_pinwheels_start_angle_and_center_are_made_whole() {
        let mut effect = pinwheel(0, 2000);
        let ramp = Curve::ramp(0.25, 90.5);
        for key in ["twist", "thickness", "speed", "armSize"] {
            effect.curves.insert(key.into(), ramp.clone());
        }
        whole_settings(&mut effect, FRAME_MS, &[], &mut { MOST_READS });
        assert!(effect.curves.values().all(|c| *c == ramp));
        // Another effect's settings are its own.
        let mut fan = Effect::new(EffectKind::Fan, 0, 2000);
        fan.curves.insert("centerX".into(), ramp.clone());
        let before = fan.clone();
        whole_settings(&mut fan, FRAME_MS, &[], &mut { MOST_READS });
        assert_eq!(fan, before);
    }
}
