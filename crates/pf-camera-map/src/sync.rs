//! Finding where the sequence starts in a video, from each frame's overall brightness.

use crate::code::{CodeSpec, PREAMBLE};
use serde::{Deserialize, Serialize};

/// One video frame's overall brightness (any scale) at `t` seconds into the video.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub t: f64,
    pub v: f32,
}

/// Where the sequence starts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncFound {
    /// Seconds into the video the first slot begins.
    pub start: f64,
    /// How well the preamble matched, 0–1 (a correlation).
    pub score: f32,
}

/// The fraction of each slot skipped at either end when sampling it (transitions, latency).
const EDGE: f64 = 0.2;
/// The weakest preamble match taken as the sequence.
const MIN_SCORE: f32 = 0.85;

/// Finds the first complete pass of the sequence in `samples` (sorted by time): where the
/// preamble's dark and white slots line up best with the brightness. `None` when no complete
/// pass is in the video or nothing matches well enough.
pub fn find_sync(samples: &[Sample], spec: &CodeSpec) -> Option<SyncFound> {
    let (first, last) = (samples.first()?.t, samples.last()?.t);
    let slot = f64::from(spec.slot_seconds);
    let total = f64::from(spec.duration());
    if slot <= 0.0 || last - first < total {
        return None;
    }
    let step = (slot / 25.0).min(1.0 / 60.0);
    let template: Vec<f32> = PREAMBLE.iter().map(|&lit| if lit { 1.0 } else { 0.0 }).collect();
    let mut scores: Vec<(f64, f32)> = Vec::new();
    // Start a little before the first frame: the first slot is dark, so a video that begins
    // during it still has the rest of that slot to measure.
    let mut start = first - slot * EDGE;
    while start + total <= last + slot * EDGE {
        if let Some(means) = slot_means(samples, start, slot, PREAMBLE.len())
            && let Some(score) = correlation(&means, &template)
        {
            scores.push((start, score));
        }
        start += step;
    }
    let best = scores.iter().map(|s| s.1).fold(f32::MIN, f32::max);
    if best < MIN_SCORE {
        return None;
    }
    // The first pass that matches about as well as the best, centred on its plateau of equally
    // good starts (any start within the slots' middles scores the same).
    let near = |s: f32| s >= best - 0.02;
    let at = scores.iter().position(|s| near(s.1))?;
    let end = scores[at..]
        .iter()
        .position(|s| !near(s.1))
        .map_or(scores.len(), |n| at + n);
    let peak = scores[at..end].iter().map(|s| s.1).fold(f32::MIN, f32::max);
    Some(SyncFound {
        start: (scores[at].0 + scores[end - 1].0) / 2.0,
        score: peak,
    })
}

/// The time span to average for each slot of a pass starting at `start`: the middle of each slot.
pub fn slot_windows(start: f64, spec: &CodeSpec) -> Vec<(f64, f64)> {
    let slot = f64::from(spec.slot_seconds);
    (0..spec.slots().len())
        .map(|k| {
            let at = start + k as f64 * slot;
            (at + slot * EDGE, at + slot * (1.0 - EDGE))
        })
        .collect()
}

/// The mean brightness in the middle of each of `count` slots from `start`; `None` if a slot has
/// no frames in its middle.
fn slot_means(samples: &[Sample], start: f64, slot: f64, count: usize) -> Option<Vec<f32>> {
    (0..count)
        .map(|k| {
            let (a, b) = (
                start + (k as f64 + EDGE) * slot,
                start + (k as f64 + 1.0 - EDGE) * slot,
            );
            let from = samples.partition_point(|s| s.t < a);
            let to = samples.partition_point(|s| s.t <= b);
            let inside = &samples[from..to.max(from)];
            (!inside.is_empty()).then(|| inside.iter().map(|s| s.v).sum::<f32>() / inside.len() as f32)
        })
        .collect()
}

/// Pearson correlation of two equal-length series; `None` when either is flat.
fn correlation(a: &[f32], b: &[f32]) -> Option<f32> {
    let n = a.len() as f32;
    let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        sab += (x - ma) * (y - mb);
        saa += (x - ma) * (x - ma);
        sbb += (y - mb) * (y - mb);
    }
    (saa > 1e-9 && sbb > 1e-9).then(|| sab / (saa * sbb).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::{Base, Slot};

    /// Brightness of a video of the sequence starting at `start`, sampled at `fps`, lasting
    /// `length` seconds: dark is 20, white 60, references and digits in between, with a gentle
    /// exposure drift and deterministic jitter.
    fn video(spec: &CodeSpec, start: f64, fps: f64, length: f64) -> Vec<Sample> {
        let slots = spec.slots();
        let slot = f64::from(spec.slot_seconds);
        (0..(length * fps) as usize)
            .map(|i| {
                let t = i as f64 / fps;
                let since = t - start;
                let lit = if since < 0.0 {
                    0.0
                } else {
                    match slots[(since / slot) as usize % slots.len()] {
                        Slot::Dark => 0.0,
                        Slot::White => 40.0,
                        Slot::Reference(_) => 14.0,
                        Slot::Digit(d) | Slot::Check(d) => 8.0 + f32::from(d) * 3.0,
                    }
                };
                let drift = 1.0 + 0.1 * (t as f32 * 0.3).sin();
                let jitter = ((i * 7919) % 13) as f32 / 13.0 - 0.5;
                Sample {
                    t,
                    v: (20.0 + lit) * drift + jitter,
                }
            })
            .collect()
    }

    #[test]
    fn finds_the_start_of_the_first_full_pass() {
        let spec = CodeSpec::new(300, Base::Four);
        for start in [0.0, 1.37, 4.02] {
            let found = find_sync(&video(&spec, start, 30.0, start + 20.0), &spec).unwrap();
            assert!(
                (found.start - start).abs() < 0.06,
                "start {start}: found {}",
                found.start
            );
            assert!(found.score > 0.95);
        }
    }

    #[test]
    fn a_video_that_starts_mid_pass_syncs_on_the_next_one() {
        let spec = CodeSpec::new(300, Base::Four);
        // The first pass started 3 s before the video; the next starts one pass later.
        let samples: Vec<Sample> = video(&spec, 0.0, 30.0, 40.0)
            .into_iter()
            .filter(|s| s.t >= 3.0)
            .map(|s| Sample { t: s.t - 3.0, ..s })
            .collect();
        let next = f64::from(spec.duration()) - 3.0;
        let found = find_sync(&samples, &spec).unwrap();
        assert!((found.start - next).abs() < 0.06, "found {}", found.start);
    }

    #[test]
    fn no_sync_without_the_sequence_or_a_complete_pass() {
        let spec = CodeSpec::new(300, Base::Four);
        let flat: Vec<Sample> = (0..600)
            .map(|i| Sample {
                t: f64::from(i) / 30.0,
                v: 20.0,
            })
            .collect();
        assert_eq!(find_sync(&flat, &spec), None);
        // Shorter than one pass.
        assert_eq!(find_sync(&video(&spec, 0.5, 30.0, 6.0), &spec), None);
        assert_eq!(find_sync(&[], &spec), None);
    }

    #[test]
    fn windows_cover_the_middle_of_each_slot() {
        let spec = CodeSpec::new(10, Base::Four);
        let windows = slot_windows(2.0, &spec);
        assert_eq!(windows.len(), spec.slots().len());
        assert!((windows[0].0 - 2.1).abs() < 1e-9 && (windows[0].1 - 2.4).abs() < 1e-9);
        assert!((windows[3].0 - 3.6).abs() < 1e-9);
    }
}
