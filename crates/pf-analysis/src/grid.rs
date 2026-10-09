//! The song cut into beat-long units (or half seconds without a beat), with the features averaged
//! over each: what structure is found from.

use crate::features::{Features, TIMBRE};

/// Seconds per unit when the song has no beat.
const UNIT_WITHOUT_BEAT_S: f64 = 0.5;

/// Beat-long units covering the whole song.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Grid {
    /// Where each unit starts (s); the last runs to `end`.
    pub starts: Vec<f64>,
    pub end: f64,
    /// Units added before the first detected beat (the beats carried back to the start).
    pub lead: usize,
}

impl Grid {
    /// Units from `beats` (s, `period` apart on average), carried back to the start and on to
    /// `end` at the beat period; half seconds without beats.
    pub fn new(beats: &[f64], period: Option<f64>, end: f64) -> Self {
        let step = period.filter(|p| *p > 0.05 && !beats.is_empty());
        let Some(step) = step else {
            let starts = (0..)
                .map(|i| i as f64 * UNIT_WITHOUT_BEAT_S)
                .take_while(|&t| t < end)
                .collect();
            return Self { starts, end, lead: 0 };
        };
        let mut starts = vec![0.0];
        let mut t = beats[0] - step;
        while t > step * 0.25 {
            starts.push(t);
            t -= step;
        }
        starts[1..].reverse();
        // A first beat right at the start stands for the unit at the start.
        let lead = if beats[0] <= step * 0.25 { 0 } else { starts.len() };
        let first = usize::from(lead == 0);
        starts.extend(beats[first..].iter().copied().filter(|&b| b > 0.0 && b < end));
        let mut t = beats[beats.len() - 1] + step;
        while t < end - step * 0.25 {
            starts.push(t);
            t += step;
        }
        Self { starts, end, lead }
    }

    pub fn len(&self) -> usize {
        self.starts.len()
    }

    /// Where unit `i` ends.
    pub fn end_of(&self, i: usize) -> f64 {
        self.starts.get(i + 1).copied().unwrap_or(self.end)
    }

    /// Where unit `i` starts (the end for `i == len`).
    pub fn start_of(&self, i: usize) -> f64 {
        self.starts.get(i).copied().unwrap_or(self.end)
    }
}

/// The features averaged over each unit.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Synced {
    /// Timbre coefficients and loudness, each scaled to zero mean and unit spread over the song.
    pub timbre: Vec<[f32; TIMBRE + 1]>,
    /// Mean chroma.
    pub chroma: Vec<[f32; 12]>,
    /// Mean power, in dB, overall and in the bass, mid, and treble bands.
    pub loudness_db: Vec<f32>,
    pub bands_db: Vec<[f32; 3]>,
    /// Mean spectral flux.
    pub flux: Vec<f32>,
}

/// dB of the mean power of dB values.
fn mean_db(values: impl Iterator<Item = f32>) -> f32 {
    let (sum, n) = values.fold((0.0f64, 0usize), |(s, n), db| {
        (s + 10f64.powf(f64::from(db) / 10.0), n + 1)
    });
    if n == 0 {
        -100.0
    } else {
        (10.0 * (sum / n as f64).max(1e-10).log10()) as f32
    }
}

/// Averages the features over each unit of `grid`.
pub(crate) fn sync(features: &Features, grid: &Grid) -> Synced {
    let mut out = Synced::default();
    if features.is_empty() {
        return out;
    }
    let mut raw_timbre: Vec<[f32; TIMBRE + 1]> = Vec::with_capacity(grid.len());
    for i in 0..grid.len() {
        let first = features.frame_at(grid.start_of(i));
        let last = features
            .frame_at(grid.end_of(i))
            .max(first + 1)
            .min(features.len());
        let frames = first.min(last - 1)..last;
        let n = frames.len() as f32;
        let mut timbre = [0.0f32; TIMBRE + 1];
        let mut chroma = [0.0f32; 12];
        let mut flux = 0.0;
        for f in frames.clone() {
            for (t, x) in timbre.iter_mut().zip(&features.timbre[f]) {
                *t += x / n;
            }
            for (c, x) in chroma.iter_mut().zip(&features.chroma[f]) {
                *c += x / n;
            }
            flux += features.flux[f] / n;
        }
        let loudness = mean_db(frames.clone().map(|f| features.loudness_db[f]));
        timbre[TIMBRE] = loudness;
        raw_timbre.push(timbre);
        out.chroma.push(chroma);
        out.loudness_db.push(loudness);
        out.bands_db
            .push([0, 1, 2].map(|b| mean_db(frames.clone().map(|f| features.bands_db[f][b]))));
        out.flux.push(flux);
    }
    // Each dimension to zero mean and unit spread, so none outweighs the rest.
    let n = raw_timbre.len().max(1) as f32;
    for d in 0..=TIMBRE {
        let mean = raw_timbre.iter().map(|t| t[d]).sum::<f32>() / n;
        let sd = (raw_timbre.iter().map(|t| (t[d] - mean).powi(2)).sum::<f32>() / n).sqrt();
        for t in &mut raw_timbre {
            t[d] = if sd > 1e-6 { (t[d] - mean) / sd } else { 0.0 };
        }
    }
    out.timbre = raw_timbre;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beats_are_carried_to_both_ends() {
        let beats: Vec<f64> = (0..10).map(|i| 2.0 + i as f64 * 0.5).collect();
        let g = Grid::new(&beats, Some(0.5), 10.0);
        assert_eq!(g.starts[0], 0.0);
        assert_eq!(g.lead, 4, "{:?}", g.starts);
        assert_eq!(g.starts[g.lead], 2.0);
        assert!((g.starts.last().unwrap() - 9.5).abs() < 1e-9, "{:?}", g.starts);
        assert!(g.starts.windows(2).all(|w| w[1] > w[0]));
        let g = Grid::new(&[0.0, 0.5, 1.0], Some(0.5), 1.5);
        assert_eq!((g.lead, g.len()), (0, 3), "{:?}", g.starts);
        let g = Grid::new(&[], None, 2.2);
        assert_eq!(g.starts, [0.0, 0.5, 1.0, 1.5, 2.0]);
        assert_eq!(g.end_of(4), 2.2);
    }
}
