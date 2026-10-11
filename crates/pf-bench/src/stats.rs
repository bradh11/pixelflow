//! Summaries of timings: percentiles over every frame of a run.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// How a set of timings is spread, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spread {
    pub count: usize,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub max: f64,
}

impl Spread {
    /// The spread of `values` (milliseconds). Percentiles are nearest-rank: p99 of 100 values is
    /// the 99th smallest, so one slow frame in a hundred shows.
    pub fn of(values: &[f64]) -> Self {
        if values.is_empty() {
            return Self::default();
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        let rank = |p: f64| {
            let n = (p / 100.0 * sorted.len() as f64).ceil() as usize;
            sorted[n.clamp(1, sorted.len()) - 1]
        };
        Self {
            count: sorted.len(),
            mean: sorted.iter().sum::<f64>() / sorted.len() as f64,
            p50: rank(50.0),
            p95: rank(95.0),
            p99: rank(99.0),
            max: sorted[sorted.len() - 1],
        }
    }

    pub fn of_durations(values: impl IntoIterator<Item = Duration>) -> Self {
        let ms: Vec<f64> = values.into_iter().map(ms).collect();
        Self::of(&ms)
    }
}

/// A duration in milliseconds.
pub fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// How far `values` stray from their mean (the population standard deviation).
pub fn deviation(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_are_nearest_rank() {
        let values: Vec<f64> = (1..=100).map(f64::from).collect();
        let spread = Spread::of(&values);
        assert_eq!(
            (spread.p50, spread.p95, spread.p99, spread.max),
            (50.0, 95.0, 99.0, 100.0)
        );
        assert_eq!((spread.count, spread.mean), (100, 50.5));
        // One slow frame in fifty is past the 95th percentile but shows in the 99th.
        let mut frames = vec![1.0; 49];
        frames.push(40.0);
        let spread = Spread::of(&frames);
        assert_eq!((spread.p95, spread.p99, spread.max), (1.0, 40.0, 40.0));
    }

    #[test]
    fn nothing_measured_is_all_zero() {
        assert_eq!(Spread::of(&[]), Spread::default());
        assert_eq!(deviation(&[]), 0.0);
        assert_eq!(deviation(&[25.0, 25.0, 25.0]), 0.0);
        assert_eq!(deviation(&[20.0, 30.0]), 5.0);
    }
}
