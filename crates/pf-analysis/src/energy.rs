//! How much is going on, bar by bar: overall, in the bass, the mids, and the treble, each 0–1.

use crate::features::Features;
use serde::Serialize;

/// The smallest loudness range (dB) scaled to 0–1, so a song that hardly changes stays near the
/// top instead of having its small changes blown up.
const MIN_RANGE_DB: f32 = 12.0;
/// Below this (dB) a song is silent.
const SILENT_DB: f32 = -70.0;

/// One bar's energy, each 0–1 relative to the song (1 = as loud as its loud parts).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BarEnergy {
    pub overall: f32,
    /// Bass (below 200 Hz): kick and bass line.
    pub low: f32,
    /// 200 Hz–2 kHz: vocals, guitars, keys.
    pub mid: f32,
    /// Above 2 kHz: cymbals, hats, brightness.
    pub high: f32,
}

fn percentile(sorted: &[f32], p: usize) -> f32 {
    sorted[(sorted.len() * p / 100).min(sorted.len() - 1)]
}

/// dB values scaled to 0–1: the song's 95th percentile is 1, and 0 is that less the larger of
/// the song's range (95th less 5th percentile) and 12 dB. All 0 for silence.
pub(crate) fn scale_db(values: &[f32]) -> Vec<f32> {
    if values.is_empty() {
        return Vec::new();
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    let (low, high) = (percentile(&sorted, 5), percentile(&sorted, 95));
    if high < SILENT_DB {
        return vec![0.0; values.len()];
    }
    let range = (high - low).max(MIN_RANGE_DB);
    values
        .iter()
        .map(|&v| (((v - (high - range)) / range).clamp(0.0, 1.0) * 100.0).round() / 100.0)
        .collect()
}

/// dB of the mean power over frames `from..to` of `db` (at least one frame).
fn mean_db(db: impl Fn(usize) -> f32, from: usize, to: usize) -> f32 {
    let to = to.max(from + 1);
    let sum: f64 = (from..to).map(|f| 10f64.powf(f64::from(db(f)) / 10.0)).sum();
    (10.0 * (sum / (to - from) as f64).max(1e-10).log10()) as f32
}

/// The energy of each bar starting at `bars` (s), the last running to `end` (s).
pub(crate) fn bar_energy(features: &Features, bars: &[f64], end: f64) -> Vec<BarEnergy> {
    if features.is_empty() || bars.is_empty() {
        return Vec::new();
    }
    let spans: Vec<(usize, usize)> = bars
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            let stop = bars.get(i + 1).copied().unwrap_or(end).max(start);
            let from = features.frame_at(start);
            (from, features.frame_at(stop).max(from + 1).min(features.len()))
        })
        .collect();
    let series = |db: &dyn Fn(usize) -> f32| -> Vec<f32> {
        scale_db(&spans.iter().map(|&(a, b)| mean_db(db, a, b)).collect::<Vec<_>>())
    };
    let overall = series(&|f| features.loudness_db[f]);
    let [low, mid, high] = [0, 1, 2].map(|band| series(&|f| features.bands_db[f][band]));
    (0..bars.len())
        .map(|i| BarEnergy {
            overall: overall[i],
            low: low[i],
            mid: mid[i],
            high: high[i],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_keeps_small_changes_small_and_silence_zero() {
        assert_eq!(scale_db(&[-80.0; 4]), [0.0; 4]);
        let scaled = scale_db(&[-20.0, -20.0, -18.0, -10.0, -10.0]);
        assert_eq!(scaled[4], 1.0);
        assert!((scaled[0] - (1.0 - 10.0 / 12.0)).abs() < 0.01, "{scaled:?}");
        let flat = scale_db(&[-10.0, -11.0, -10.0]);
        assert!(flat.iter().all(|&x| x > 0.9), "{flat:?}");
    }
}
