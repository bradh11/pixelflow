//! Music analysis for sequencing: where the notes start (onsets), the tempo, the beats, the bars,
//! the song's sections, and the moments to land on, ready to use as timing tracks.
//!
//! 1. **Onset envelope**: the song (mono) is cut into 1024-sample frames every 512 samples; each
//!    frame's log-magnitude spectrum is compared with the previous one, and the total rise
//!    (spectral flux) says how much new sound started.
//! 2. **Onsets**: peaks of the envelope above a moving average (adaptive threshold).
//! 3. **Tempo**: the envelope's autocorrelation over 60–200 BPM, gently weighted toward 120 BPM
//!    to avoid half- and double-time mistakes.
//! 4. **Beats**: dynamic programming picks the beat times that land on strong onsets while
//!    staying close to the tempo (Ellis, 2007), so the grid follows small tempo drifts.
//! 5. **Features**: alongside, every ~46 ms, timbre coefficients (from a log-mel spectrum),
//!    chroma, loudness, flux, and band energies, averaged per beat.
//! 6. **Bars**: 4/4 is assumed; the downbeat is the one of every 4 beats where strong onsets,
//!    kicks, chord changes, and section changes fall most.
//! 7. **Sections**: where the timbre and harmony change (novelty in self-similarity matrices), on
//!    bar lines, grouped by what repeats and named (Verse, Chorus …); see [`Analysis::sections()`].
//! 8. **Energy and accents**: each bar's loudness overall and in the bass, mids, and treble, 0–1
//!    ([`Analysis::bar_energy`]); and moments to land on: hits, drops, breaks, and builds
//!    ([`Analysis::events`]).
//! 9. **Voice**: separately, a cheap guess at when the voice is sounding ([`vocal_activity`]),
//!    to fine-tune lyric timing.

mod beats;
mod energy;
mod events;
mod features;
mod grid;
mod meter;
mod onset;
mod sections;
mod structure;
mod vocal;

pub use beats::{bars, beat_grid, estimate_tempo};
pub use energy::BarEnergy;
pub use events::{Event, EventKind};
pub use onset::{FRAME, HOP, OnsetEnvelope, onset_envelope, pick_onsets};
pub use sections::{Level, Section};
pub use vocal::{VOCAL_THRESHOLD, VocalActivity, vocal_activity, vocal_activity_file};

use pf_audio::{AudioError, MonoSamples};
use pf_sequence::{Mark, TimingKind, TimingTrack};
use serde::Serialize;
use std::path::Path;

/// Longest song analyzed: 4 hours (the longest sequence). Anything after that is ignored.
pub const MAX_ANALYSIS_MS: u64 = 4 * 60 * 60 * 1000;

#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("{0}")]
    Audio(#[from] AudioError),
    #[error("The music file has no sound to analyze.")]
    Empty,
    #[error("The analysis was stopped.")]
    Cancelled,
}

/// What analysis found in a song. Times are in milliseconds from the start.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub duration_ms: u64,
    /// Beats per minute, if the song has a steady pulse.
    pub tempo_bpm: Option<f32>,
    pub beats: Vec<u64>,
    /// The first beat of each bar (assuming 4/4): the downbeats.
    pub bars: Vec<u64>,
    pub onsets: Vec<u64>,
    /// How loud each second is, 0–1 (1 = as loud as the song's loud parts).
    #[serde(skip)]
    pub energy: Vec<f32>,
    /// The song's sections from its structure, back to back (empty for an analysis made without
    /// it: [`Analysis::sections()`] then works them out from `energy`).
    pub sections: Vec<Section>,
    /// Moments to land on (hits, drops, breaks, builds), in time order.
    pub events: Vec<Event>,
    /// Each bar's energy (one per entry in `bars`).
    pub bar_energy: Vec<BarEnergy>,
    pub confidence: Confidence,
}

/// How sure the analysis is of each part, 0–1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Confidence {
    /// The tempo and beats (how much stronger the sound is on the beats than between them).
    pub tempo: f32,
    /// Which beat is beat 1.
    pub downbeat: f32,
    /// The sections' grouping and names (their mean).
    pub sections: f32,
}

/// Samples between checks for a stop.
const CHECK_EVERY: usize = 1 << 16;

/// Analyzes mono samples at `sample_rate`.
pub fn analyze(samples: impl IntoIterator<Item = f32>, sample_rate: u32) -> Analysis {
    match analyze_cancellable(samples, sample_rate, &|| false) {
        Ok(analysis) => analysis,
        Err(_) => unreachable!("an analysis that is never stopped finishes"),
    }
}

/// Like [`analyze`], checking `stop` every so often (about every 1.5 s of 44.1 kHz audio) and
/// giving up with [`AnalysisError::Cancelled`] once it says so.
pub fn analyze_cancellable(
    samples: impl IntoIterator<Item = f32>,
    sample_rate: u32,
    stop: &dyn Fn() -> bool,
) -> Result<Analysis, AnalysisError> {
    let rate = sample_rate.max(1);
    let limit = MAX_ANALYSIS_MS.saturating_mul(u64::from(rate)) / 1000;
    let mut stopped = false;
    let checked = samples
        .into_iter()
        .take(limit as usize)
        .enumerate()
        .take_while(|(i, _)| {
            if i % CHECK_EVERY == 0 && stop() {
                stopped = true;
            }
            !stopped
        })
        .map(|(_, s)| s);
    // The features are taken from the same samples as the envelope, as they go by.
    let mut extractor = features::FeatureExtractor::new(rate);
    let envelope = onset_envelope(checked.inspect(|&s| extractor.push(onset::clean(s))), rate);
    if stopped || stop() {
        return Err(AnalysisError::Cancelled);
    }
    let features = extractor.finish();
    let onsets = pick_onsets(&envelope);
    let tempo = estimate_tempo(&envelope);
    let beat_frames = tempo
        .map(|period| beat_grid(&envelope, period, &onsets))
        .unwrap_or_default();
    let ms = |frames: &[usize]| frames.iter().map(|&f| envelope.time_ms(f)).collect::<Vec<_>>();
    let beats = ms(&beat_frames);
    let duration_ms = envelope.duration_ms();
    let seconds = |times: &[u64]| times.iter().map(|&t| t as f64 / 1000.0).collect::<Vec<_>>();
    let end = duration_ms as f64 / 1000.0;

    // Beat-long units with the features averaged over each, compared every one with every other
    // (unless there are too many to: a very long recording's sections then come from its
    // loudness, see `Analysis::sections()`).
    let period = tempo.map(|p| p * envelope.frame_seconds());
    let grid = grid::Grid::new(&seconds(&beats), period, end);
    let synced = grid::sync(&features, &grid);
    let similarity = (grid.len() <= structure::MAX_UNITS)
        .then(|| (structure::Ssm::timbre(&synced), structure::Ssm::chroma(&synced)));
    let novelty = match &similarity {
        Some((timbre, chroma)) => structure::novelty(timbre, chroma),
        None => vec![0.0; grid.len()],
    };
    if stop() {
        return Err(AnalysisError::Cancelled);
    }

    let (phase, downbeat) =
        meter::downbeat_phase(&envelope, &features, &grid, &synced, &novelty, &beat_frames);
    let bars: Vec<u64> = beats
        .iter()
        .copied()
        .skip(phase)
        .step_by(structure::BEATS_PER_BAR)
        .collect();
    let bar_phase = if beats.is_empty() { 0 } else { grid.lead + phase };
    let sections = match &similarity {
        Some((timbre, chroma)) => {
            let cuts = structure::boundaries(&novelty, &grid, bar_phase, structure::min_section_units());
            sections::from_structure(&grid, &synced, &structure::Ssm::mean(timbre, chroma), &cuts)
        }
        None => Vec::new(),
    };
    let bar_energy = energy::bar_energy(&features, &seconds(&bars), end);
    let events = events::events(&envelope, &onsets, &seconds(&beats), &seconds(&bars), &bar_energy);
    let confidence = Confidence {
        tempo: round2(meter::tempo_confidence(&envelope, &beat_frames)),
        downbeat: round2(downbeat),
        sections: round2(sections.iter().map(|s| s.confidence).sum::<f32>() / sections.len().max(1) as f32),
    };
    Ok(Analysis {
        duration_ms,
        tempo_bpm: tempo.map(|period| (60.0 / (period * envelope.frame_seconds())) as f32),
        beats,
        bars,
        onsets: ms(&onsets),
        energy: sections::energy_per_second(&envelope),
        sections,
        events,
        bar_energy,
        confidence,
    })
}

fn round2(x: f32) -> f32 {
    (x * 100.0).round() / 100.0
}

/// Decodes and analyzes a music file.
pub fn analyze_file(path: &Path) -> Result<Analysis, AnalysisError> {
    analyze_file_cancellable(path, &|| false)
}

/// Like [`analyze_file`], stopping when `stop` says so (see [`analyze_cancellable`]).
pub fn analyze_file_cancellable(path: &Path, stop: &dyn Fn() -> bool) -> Result<Analysis, AnalysisError> {
    let samples = MonoSamples::open(path)?;
    let rate = samples.sample_rate();
    let analysis = analyze_cancellable(samples, rate, stop)?;
    if analysis.duration_ms == 0 {
        return Err(AnalysisError::Empty);
    }
    Ok(analysis)
}

/// Marks spanning from each time to the next (the last runs to `end_ms`, or as long as the one
/// before it), labeled by `label`.
fn spans(times: &[u64], end_ms: u64, label: impl Fn(usize) -> String) -> Vec<Mark> {
    times
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            let end = match times.get(i + 1) {
                Some(&next) => next,
                None => {
                    let previous = i.checked_sub(1).map_or(0, |p| start - times[p]);
                    end_ms.min(start + previous).max(start)
                }
            };
            Mark::new(start, end, label(i))
        })
        .collect()
}

impl Analysis {
    /// Timing tracks for the sequence: "Beats" (labeled 1–4 within each bar), "Bars" (numbered),
    /// and "Onsets". See also [`Analysis::sections_track`] and [`Analysis::accents_track`].
    pub fn timing_tracks(&self) -> Vec<TimingTrack> {
        let first_bar = self.bars.first().copied();
        // How far into a bar the first beat is, so beat labels count from each downbeat.
        let lead = first_bar.map_or(0, |b| self.beats.iter().take_while(|&&t| t < b).count());
        let beats = spans(&self.beats, self.duration_ms, |i| {
            (((i + 4 - lead % 4) % 4) + 1).to_string()
        });
        let bars = spans(&self.bars, self.duration_ms, |i| (i + 1).to_string());
        let onsets = spans(&self.onsets, self.duration_ms, |_| String::new());
        vec![
            TimingTrack::new("Beats", TimingKind::Beats, beats),
            TimingTrack::new("Bars", TimingKind::Bars, bars),
            TimingTrack::new("Onsets", TimingKind::Custom, onsets),
        ]
    }
}
