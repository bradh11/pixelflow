//! Which beat is beat 1, and how sure the tempo and that are.
//!
//! Each of the four ways to group the beats into bars is scored on what tends to happen on a
//! downbeat: a strong onset, a kick (a rise in the bass), a change of chord, and a change of
//! section. Each cue is measured at every beat and scaled to the song's spread, so a cue the song
//! doesn't have (no bass, no harmony) adds nothing either way.

use crate::features::Features;
use crate::grid::{Grid, Synced};
use crate::onset::{OnsetEnvelope, normalized};
use crate::structure::BEATS_PER_BAR;

/// How much each cue counts: onset strength, bass rise, chord change, section change.
const WEIGHTS: [f32; 4] = [1.0, 1.0, 1.0, 0.5];

/// The strongest normalized envelope value near `frame` (onsets can sit a frame off the grid).
fn strength(e: &[f32], frame: usize) -> f32 {
    let (lo, hi) = (frame.saturating_sub(1), (frame + 2).min(e.len()));
    e.get(lo..hi)
        .map_or(0.0, |w| w.iter().copied().fold(0.0, f32::max))
}

/// Values scaled to zero mean and unit spread (all 0 when they're all alike).
fn standardized(values: &[f32]) -> Vec<f32> {
    let n = values.len().max(1) as f32;
    let mean = values.iter().sum::<f32>() / n;
    let sd = (values.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n).sqrt();
    if sd < 1e-6 {
        return vec![0.0; values.len()];
    }
    values.iter().map(|x| (x - mean) / sd).collect()
}

/// Cosine distance between the mean chroma of two runs of units.
fn chord_change(chroma: &[[f32; 12]], before: std::ops::Range<usize>, after: std::ops::Range<usize>) -> f32 {
    let mean = |r: std::ops::Range<usize>| {
        let mut m = [0.0f32; 12];
        for c in &chroma[r] {
            for (x, y) in m.iter_mut().zip(c) {
                *x += y;
            }
        }
        m
    };
    let (a, b) = (mean(before), mean(after));
    let norm = |v: &[f32; 12]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
    let (na, nb) = (norm(&a), norm(&b));
    if na < 1e-6 || nb < 1e-6 {
        return 0.0;
    }
    1.0 - a.iter().zip(&b).map(|(x, y)| x * y).sum::<f32>() / (na * nb)
}

/// The beat (0–3, an index into `beats`) the first full bar starts on, and how sure that is
/// (0–1). `beats` are envelope frames; they're units `grid.lead ..` of `grid`.
pub(crate) fn downbeat_phase(
    envelope: &OnsetEnvelope,
    features: &Features,
    grid: &Grid,
    synced: &Synced,
    novelty: &[f32],
    beats: &[usize],
) -> (usize, f32) {
    if beats.len() < 2 * BEATS_PER_BAR {
        return (0, 0.0);
    }
    let e = normalized(envelope);
    let onset: Vec<f32> = beats.iter().map(|&b| strength(&e, b)).collect();
    let bass: Vec<f32> = beats
        .iter()
        .map(|&b| {
            let f = features.frame_at(envelope.time_ms(b) as f64 / 1000.0);
            (f.saturating_sub(1)..(f + 2).min(features.len()))
                .filter(|&g| g > 0)
                .map(|g| (features.bands_db[g][0] - features.bands_db[g - 1][0]).max(0.0))
                .fold(0.0, f32::max)
        })
        .collect();
    let units = synced.chroma.len();
    let unit = |j: usize| (grid.lead + j).min(units.saturating_sub(1));
    let chord: Vec<f32> = (0..beats.len())
        .map(|j| {
            let u = unit(j);
            if u < 2 || u + 2 > units {
                return 0.0;
            }
            chord_change(&synced.chroma, u - 2..u, u..u + 2)
        })
        .collect();
    let change: Vec<f32> = (0..beats.len())
        .map(|j| novelty.get(unit(j)).copied().unwrap_or(0.0))
        .collect();
    let cues = [onset, bass, chord, change].map(|c| standardized(&c));
    let scores: Vec<f32> = (0..BEATS_PER_BAR)
        .map(|phase| {
            cues.iter()
                .zip(WEIGHTS)
                .map(|(cue, w)| {
                    let mine: Vec<f32> = cue.iter().skip(phase).step_by(BEATS_PER_BAR).copied().collect();
                    w * mine.iter().sum::<f32>() / mine.len().max(1) as f32
                })
                .sum()
        })
        .collect();
    let mut order: Vec<usize> = (0..BEATS_PER_BAR).collect();
    order.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]).then(a.cmp(&b)));
    let margin = scores[order[0]] - scores[order[1]];
    (order[0], (1.0 - (-margin / 0.25).exp()).clamp(0.0, 1.0))
}

/// How sure the beat grid is (0–1): how much stronger the onsets on the beats are than halfway
/// between them (1 at 60% stronger or more).
pub(crate) fn tempo_confidence(envelope: &OnsetEnvelope, beats: &[usize]) -> f32 {
    if beats.len() < 4 {
        return 0.0;
    }
    let e = normalized(envelope);
    let on = beats.iter().map(|&b| strength(&e, b)).sum::<f32>() / beats.len() as f32;
    let between: Vec<f32> = beats
        .windows(2)
        .map(|w| strength(&e, (w[0] + w[1]) / 2))
        .collect();
    let off = between.iter().sum::<f32>() / between.len() as f32;
    if on <= 1e-6 {
        return 0.0;
    }
    if off <= 1e-6 {
        return 1.0;
    }
    ((on / off - 1.0) / 0.6).clamp(0.0, 1.0)
}
