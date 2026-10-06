//! A song's energy over time, and sections from it.

use crate::{Analysis, OnsetEnvelope};
use pf_sequence::{Mark, TimingKind, TimingTrack};
use serde::Serialize;

/// How much is going on in a section, relative to the song's loud parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Low,
    Medium,
    High,
}

impl Level {
    fn of(energy: f32) -> Self {
        if energy < 0.4 {
            Level::Low
        } else if energy < 0.7 {
            Level::Medium
        } else {
            Level::High
        }
    }

    fn word(self) -> &'static str {
        match self {
            Level::Low => "Low",
            Level::Medium => "Mid",
            Level::High => "High",
        }
    }
}

/// A stretch of the song at one energy level.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub start_ms: u64,
    pub end_ms: u64,
    /// Mean energy, 0–1.
    pub energy: f32,
    pub level: Level,
    /// "Intro", "Outro", or the level and its count ("High 2").
    pub label: String,
}

/// Sections at most (a longer song's are merged into fewer).
const MAX_SECTIONS: usize = 32;
/// Bars per phrase, the unit sections are made of.
const PHRASE_BARS: usize = 4;
/// The phrase length without a beat to go by.
const PHRASE_MS: u64 = 8_000;

/// Loudness per second, scaled so the song's 95th percentile is 1 (all 0 for silence).
pub(crate) fn energy_per_second(envelope: &OnsetEnvelope) -> Vec<f32> {
    let seconds = envelope.duration_ms().div_ceil(1000) as usize;
    let mut sums = vec![(0.0f64, 0u32); seconds];
    for (frame, &loud) in envelope.loudness.iter().enumerate() {
        let second = (envelope.time_ms(frame) / 1000) as usize;
        if let Some(slot) = sums.get_mut(second.min(seconds.saturating_sub(1))) {
            slot.0 += f64::from(loud);
            slot.1 += 1;
        }
    }
    let raw: Vec<f32> = sums
        .iter()
        .map(|&(sum, n)| if n == 0 { 0.0 } else { (sum / f64::from(n)) as f32 })
        .collect();
    let mut sorted = raw.clone();
    sorted.sort_by(f32::total_cmp);
    let reference = sorted
        .get((sorted.len() * 95 / 100).min(sorted.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0.0);
    if reference < 1e-4 {
        return vec![0.0; raw.len()];
    }
    raw.iter()
        .map(|&e| ((e / reference).min(1.0) * 100.0).round() / 100.0)
        .collect()
}

impl Analysis {
    /// Where phrases start: every 4th bar (the first from 0), or every 8 s without bars.
    fn phrase_starts(&self) -> Vec<u64> {
        let mut starts = vec![0];
        if self.bars.len() >= 2 {
            starts.extend(
                self.bars
                    .iter()
                    .copied()
                    .step_by(PHRASE_BARS)
                    .filter(|&b| b > 0 && b < self.duration_ms),
            );
        } else {
            starts.extend((1..).map(|i| i * PHRASE_MS).take_while(|&t| t < self.duration_ms));
        }
        starts.dedup();
        starts
    }

    /// Mean energy from `start_ms` to `end_ms`.
    fn energy_between(&self, start_ms: u64, end_ms: u64) -> f32 {
        let from = (start_ms / 1000) as usize;
        let to = (end_ms.div_ceil(1000) as usize).min(self.energy.len());
        let part = self.energy.get(from..to).unwrap_or_default();
        if part.is_empty() {
            0.0
        } else {
            part.iter().sum::<f32>() / part.len() as f32
        }
    }

    /// Moves each change of level to the bar near it where the energy jumps most (the phrase
    /// grid can be a bar or two off where the song changes).
    fn refine_boundaries(&self, runs: &mut [(u64, u64, Level)]) {
        if self.bars.len() < 2 {
            return;
        }
        let window = 2 * PHRASE_BARS;
        for i in 1..runs.len() {
            let (lo, hi) = (runs[i - 1].0, runs[i].1);
            let at = runs[i].0;
            let Some(here) = self.bars.iter().position(|&b| b >= at) else {
                continue;
            };
            let best = self
                .bars
                .iter()
                .enumerate()
                .skip(here.saturating_sub(PHRASE_BARS))
                .take(window + 1)
                .filter(|&(_, &b)| b > lo && b < hi)
                .map(|(k, &b)| {
                    let before = self.bars.get(k.saturating_sub(2)).copied().unwrap_or(lo).max(lo);
                    let after = self.bars.get(k + 2).copied().unwrap_or(hi).min(hi);
                    let jump = (self.energy_between(b, after) - self.energy_between(before, b)).abs();
                    (jump, b)
                })
                .max_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, bar)) = best {
                runs[i - 1].1 = bar;
                runs[i].0 = bar;
            }
        }
    }

    /// The song's sections: runs of phrases at the same energy level, back to back from the
    /// start to the end. The first and last are "Intro" and "Outro" unless they're high.
    pub fn sections(&self) -> Vec<Section> {
        if self.duration_ms == 0 {
            return Vec::new();
        }
        let starts = self.phrase_starts();
        let mut runs: Vec<(u64, u64, Level)> = Vec::new();
        for (i, &start) in starts.iter().enumerate() {
            let end = starts.get(i + 1).copied().unwrap_or(self.duration_ms);
            let level = Level::of(self.energy_between(start, end));
            match runs.last_mut() {
                Some(last) if last.2 == level => last.1 = end,
                _ => runs.push((start, end, level)),
            }
        }
        self.refine_boundaries(&mut runs);
        // A very long song: fold the shortest section into its neighbour until few enough.
        while runs.len() > MAX_SECTIONS {
            let shortest = (0..runs.len())
                .min_by_key(|&i| runs[i].1 - runs[i].0)
                .unwrap_or(0);
            let into = if shortest == 0 { 1 } else { shortest - 1 };
            let (lo, hi) = (shortest.min(into), shortest.max(into));
            runs[lo].1 = runs[hi].1;
            runs.remove(hi);
        }
        let last = runs.len() - 1;
        let mut counts = [0usize; 3];
        runs.iter()
            .enumerate()
            .map(|(i, &(start_ms, end_ms, level))| {
                let energy = (self.energy_between(start_ms, end_ms) * 100.0).round() / 100.0;
                let label = if last == 0 {
                    "Whole song".to_string()
                } else if i == 0 && level != Level::High {
                    "Intro".to_string()
                } else if i == last && level != Level::High {
                    "Outro".to_string()
                } else {
                    let n = &mut counts[level as usize];
                    *n += 1;
                    format!("{} {n}", level.word())
                };
                Section {
                    start_ms,
                    end_ms,
                    energy,
                    level,
                    label,
                }
            })
            .collect()
    }

    /// The sections as a "Sections" timing track, labeled.
    pub fn sections_track(&self) -> TimingTrack {
        let marks = self
            .sections()
            .into_iter()
            .map(|s| Mark::new(s.start_ms, s.end_ms, s.label))
            .collect();
        TimingTrack::new("Sections", TimingKind::Sections, marks)
    }
}
