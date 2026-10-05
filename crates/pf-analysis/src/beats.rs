//! Tempo, the beat grid, and bars.

use crate::onset::{OnsetEnvelope, normalized};

const MIN_BPM: f64 = 60.0;
const MAX_BPM: f64 = 200.0;
/// Tempo the estimate leans toward when two tempos fit equally well (half or double time).
const PREFERRED_BPM: f64 = 120.0;
/// Width of that lean, in octaves.
const PREFERENCE_OCTAVES: f64 = 1.0;
/// How strongly the beat tracker keeps to the tempo versus chasing onsets.
const TIGHTNESS: f32 = 400.0;

/// The beat period in frames (fractional), or `None` without a steady pulse.
pub fn estimate_tempo(envelope: &OnsetEnvelope) -> Option<f64> {
    // Smoothed, so a beat period that falls between whole frames still lines peaks up (otherwise
    // twice the period, which may land closer to a whole number of frames, can look better).
    let e = smoothed(&normalized(envelope));
    let n = e.len();
    let frame_s = envelope.frame_seconds();
    let min_lag = (60.0 / MAX_BPM / frame_s).floor().max(1.0) as usize;
    let max_lag = (60.0 / MIN_BPM / frame_s).ceil() as usize;
    if n < max_lag * 2 + 2 {
        return None;
    }
    let mean = e.iter().map(|&x| f64::from(x)).sum::<f64>() / n as f64;
    let a: Vec<f64> = e.iter().map(|&x| f64::from(x) - mean).collect();
    let corr = |lag: usize| -> f64 {
        let sum: f64 = a[..n - lag].iter().zip(&a[lag..]).map(|(x, y)| x * y).sum();
        sum / (n - lag) as f64
    };
    // One past each end, for interpolation.
    let raw: Vec<f64> = (min_lag - 1..=max_lag + 1).map(corr).collect();
    let at = |lag: usize| raw[lag + 1 - min_lag];
    let mut best: Option<(usize, f64)> = None;
    for lag in min_lag..=max_lag {
        let r = at(lag);
        if r <= 0.0 {
            continue;
        }
        let bpm = 60.0 / (lag as f64 * frame_s);
        let lean = (-0.5 * ((bpm / PREFERRED_BPM).log2() / PREFERENCE_OCTAVES).powi(2)).exp();
        let score = r * lean;
        if best.is_none_or(|(_, s)| score > s) {
            best = Some((lag, score));
        }
    }
    let (lag, _) = best?;
    // Refine between frames with a parabola through the neighbors.
    let (l, c, r) = (at(lag - 1), at(lag), at(lag + 1));
    let denom = l - 2.0 * c + r;
    let shift = if denom.abs() > 1e-12 {
        (0.5 * (l - r) / denom).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    Some(lag as f64 + shift)
}

/// A light blur (a 5-frame triangle).
fn smoothed(e: &[f32]) -> Vec<f32> {
    const KERNEL: [f32; 5] = [1.0, 2.0, 3.0, 2.0, 1.0];
    (0..e.len())
        .map(|i| {
            let mut sum = 0.0;
            for (k, w) in KERNEL.iter().enumerate() {
                if let Some(&x) = (i + k).checked_sub(2).and_then(|j| e.get(j)) {
                    sum += w * x;
                }
            }
            sum / 9.0
        })
        .collect()
}

/// Beat frames: the path through the song that lands on strong onsets while keeping beats about
/// `period` frames apart (dynamic programming). Beats before the first onset or after the last
/// (more than half a beat away) are dropped.
pub fn beat_grid(envelope: &OnsetEnvelope, period: f64, onsets: &[usize]) -> Vec<usize> {
    let e = normalized(envelope);
    let n = e.len();
    if n == 0 || !period.is_finite() || period < 1.0 {
        return Vec::new();
    }
    let p = period as f32;
    let earliest = (2.0 * period).round() as usize;
    let latest = (period / 2.0).round().max(1.0) as usize;
    let mut score = vec![0.0f32; n];
    let mut previous: Vec<Option<usize>> = vec![None; n];
    for t in 0..n {
        let strength = e[t].max(0.0);
        let mut best: Option<(usize, f32)> = None;
        if t >= latest {
            let first = t.saturating_sub(earliest);
            for (tau, &earlier) in score.iter().enumerate().take(t - latest + 1).skip(first) {
                let gap = (t - tau) as f32 / p;
                let candidate = earlier - TIGHTNESS * gap.ln().powi(2);
                if best.is_none_or(|(_, s)| candidate > s) {
                    best = Some((tau, candidate));
                }
            }
        }
        match best {
            Some((tau, s)) if s > 0.0 => {
                score[t] = strength + s;
                previous[t] = Some(tau);
            }
            _ => score[t] = strength,
        }
    }
    // End on the best-scoring frame within the last beat.
    let tail = n.saturating_sub(period.ceil() as usize);
    let Some(mut t) = (tail..n).max_by(|&a, &b| score[a].total_cmp(&score[b])) else {
        return Vec::new();
    };
    let mut beats = vec![t];
    while let Some(tau) = previous[t] {
        beats.push(tau);
        t = tau;
    }
    beats.reverse();
    let (Some(&first), Some(&last)) = (onsets.first(), onsets.last()) else {
        return Vec::new();
    };
    let slack = latest;
    beats.retain(|&b| b + slack >= first && b <= last + slack);
    beats
}

/// The first beat of each bar, assuming 4/4: of the four ways to group the beats, the one whose
/// first beats are strongest overall.
pub fn bars(envelope: &OnsetEnvelope, beats: &[usize]) -> Vec<usize> {
    if beats.is_empty() {
        return Vec::new();
    }
    let e = normalized(envelope);
    let strength = |frame: usize| -> f32 {
        // The strongest envelope value near the beat (onsets can sit a frame off the grid).
        let (lo, hi) = (frame.saturating_sub(1), (frame + 2).min(e.len()));
        e.get(lo..hi)
            .map_or(0.0, |w| w.iter().copied().fold(0.0, f32::max))
    };
    let phase = (0..4)
        .max_by(|&a, &b| {
            let total = |k: usize| -> f32 { beats.iter().skip(k).step_by(4).map(|&f| strength(f)).sum() };
            total(a).total_cmp(&total(b)).then(b.cmp(&a))
        })
        .unwrap_or(0);
    beats.iter().skip(phase).step_by(4).copied().collect()
}
