//! A song's sections, and its loudness per second (what sections came from before the song's
//! structure was analyzed, and still do for an analysis made without it).

use crate::energy::scale_db;
use crate::grid::{Grid, Synced};
use crate::structure::{self, MAX_SECTIONS, Ssm};
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
    pub(crate) fn of(energy: f32) -> Self {
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

/// A section of the song.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub start_ms: u64,
    pub end_ms: u64,
    /// Mean energy, 0–1 (relative to the song's loud parts).
    pub energy: f32,
    pub level: Level,
    /// What it probably is: "Intro", "Verse", "Pre-Chorus", "Chorus", "Bridge", "Break",
    /// "Interlude", "Outro", or "Part" (repeated, but not one of those); "Whole song" when the song
    /// is one section. From an analysis without structure: the level and its count ("High 2").
    pub label: String,
    /// Sections of the same material share a letter (A, B, A, C …).
    pub group: String,
    /// How sure the grouping and label are, 0–1.
    pub confidence: f32,
}

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

/// Sections from the structure: units of `grid` cut at `cuts`, grouped by what repeats in
/// `ssm`, and named.
pub(crate) fn from_structure(grid: &Grid, synced: &Synced, ssm: &Ssm, cuts: &[usize]) -> Vec<Section> {
    let n = grid.len().min(synced.loudness_db.len());
    if n == 0 || grid.end <= 0.0 {
        return Vec::new();
    }
    let mut edges = vec![0];
    edges.extend(cuts.iter().copied().filter(|&c| c > 0 && c < n));
    edges.push(n);
    edges.dedup();
    let cut: Vec<(usize, usize)> = edges.windows(2).map(|w| (w[0], w[1])).collect();
    let (cut_group, cut_sure) = structure::groups(ssm, &cut);
    // Back-to-back sections of the same material are one section; groups are then numbered by
    // first appearance again.
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut group: Vec<usize> = Vec::new();
    let mut sure: Vec<(f32, f32)> = Vec::new();
    for (i, &(a, b)) in cut.iter().enumerate() {
        if group.last() == Some(&cut_group[i]) {
            if let (Some(span), Some(s)) = (spans.last_mut(), sure.last_mut()) {
                span.1 = b;
                *s = (s.0 + cut_sure[i], s.1 + 1.0);
            }
        } else {
            spans.push((a, b));
            group.push(cut_group[i]);
            sure.push((cut_sure[i], 1.0));
        }
    }
    let mut order: Vec<usize> = Vec::new();
    for g in &mut group {
        let at = order.iter().position(|o| o == g).unwrap_or_else(|| {
            order.push(*g);
            order.len() - 1
        });
        *g = at;
    }
    let sure: Vec<f32> = sure.iter().map(|&(sum, n)| sum / n).collect();
    let unit_energy = scale_db(&synced.loudness_db[..n]);
    let energy: Vec<f32> = spans
        .iter()
        .map(|&(a, b)| {
            let mean = unit_energy[a..b].iter().sum::<f32>() / (b - a) as f32;
            (mean * 100.0).round() / 100.0
        })
        .collect();
    let starts: Vec<f64> = spans.iter().map(|&(a, _)| grid.start_of(a)).collect();
    let names = structure::names(&group, &energy, &starts, grid.end);
    let ms = |s: f64| (s * 1000.0).round() as u64;
    spans
        .iter()
        .enumerate()
        .map(|(i, &(a, b))| Section {
            start_ms: if a == 0 { 0 } else { ms(grid.start_of(a)) },
            end_ms: ms(grid.start_of(b)),
            energy: energy[i],
            level: Level::of(energy[i]),
            label: names[i].0.clone(),
            group: structure::letter(group[i]),
            confidence: ((sure[i] + names[i].1) / 2.0 * 100.0).round() / 100.0,
        })
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

    /// The song's sections, back to back from the start to the end: those found from its
    /// structure (the `sections` field) or, for an analysis without them, runs of phrases at the
    /// same energy level (the first and last "Intro" and "Outro" unless they're high).
    pub fn sections(&self) -> Vec<Section> {
        if !self.sections.is_empty() {
            return self.sections.clone();
        }
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
                    group: crate::structure::letter(level as usize),
                    confidence: 0.3,
                }
            })
            .collect()
    }

    /// The sections as a "Sections" timing track, labeled, and numbered where a label comes up
    /// more than once ("Chorus 2").
    pub fn sections_track(&self) -> TimingTrack {
        let sections = self.sections();
        let mut seen: Vec<(&str, usize)> = Vec::new();
        let marks = sections
            .iter()
            .map(|s| {
                let repeats = sections.iter().filter(|o| o.label == s.label).count() > 1;
                let label = if repeats && !s.label.ends_with(|c: char| c.is_ascii_digit()) {
                    let n = match seen.iter_mut().find(|(l, _)| *l == s.label) {
                        Some((_, n)) => {
                            *n += 1;
                            *n
                        }
                        None => {
                            seen.push((&s.label, 1));
                            1
                        }
                    };
                    format!("{} {n}", s.label)
                } else {
                    s.label.clone()
                };
                Mark::new(s.start_ms, s.end_ms, label)
            })
            .collect();
        TimingTrack::new("Sections", TimingKind::Sections, marks)
    }

    /// The moments to land on ([`Analysis::events`]) as an "Accents" timing track: breaks and
    /// builds for as long as they last, hits and drops for a beat (cut short by the next one).
    /// A hit or drop inside a break or build is left out.
    pub fn accents_track(&self) -> TimingTrack {
        let beat = self
            .tempo_bpm
            .filter(|t| *t > 0.0)
            .map_or(250, |t| (60_000.0 / t).round() as u64)
            .clamp(100, 1000);
        let spans: Vec<(u64, u64)> = self
            .events
            .iter()
            .filter_map(|e| e.duration_ms.map(|d| (e.time_ms, e.time_ms + d)))
            .collect();
        let mut marks: Vec<Mark> = self
            .events
            .iter()
            .filter(|e| {
                e.duration_ms.is_some() || !spans.iter().any(|&(a, b)| e.time_ms > a && e.time_ms < b)
            })
            .map(|e| {
                let end = e.time_ms + e.duration_ms.unwrap_or(beat).max(1);
                Mark::new(
                    e.time_ms,
                    end.min(self.duration_ms.max(e.time_ms + 1)),
                    e.kind.word(),
                )
            })
            .collect();
        marks.sort_by_key(|m| (m.start_ms, m.end_ms));
        let mut kept: Vec<Mark> = Vec::with_capacity(marks.len());
        for mark in marks {
            if let Some(last) = kept.last_mut()
                && last.end_ms > mark.start_ms
            {
                if mark.start_ms > last.start_ms {
                    last.end_ms = mark.start_ms;
                } else {
                    // Two at once: the longer is kept.
                    if mark.end_ms > last.end_ms {
                        *last = mark;
                    }
                    continue;
                }
            }
            if mark.end_ms > mark.start_ms {
                kept.push(mark);
            }
        }
        TimingTrack::new("Accents", TimingKind::Custom, kept)
    }
}
