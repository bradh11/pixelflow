//! Music analysis for sequencing: where the notes start (onsets), the tempo, the beats, and the
//! bars, ready to use as timing tracks.
//!
//! 1. **Onset envelope**: the song (mono) is cut into 1024-sample frames every 512 samples; each
//!    frame's log-magnitude spectrum is compared with the previous one, and the total rise
//!    (spectral flux) says how much new sound started.
//! 2. **Onsets**: peaks of the envelope above a moving average (adaptive threshold).
//! 3. **Tempo**: the envelope's autocorrelation over 60–200 BPM, gently weighted toward 120 BPM
//!    to avoid half- and double-time mistakes.
//! 4. **Beats**: dynamic programming picks the beat times that land on strong onsets while
//!    staying close to the tempo (Ellis, 2007), so the grid follows small tempo drifts.
//! 5. **Bars**: 4/4 is assumed; the downbeat is whichever of every 4 beats is strongest overall.
//! 6. **Energy and sections**: each frame's loudness (RMS), averaged per second and scaled to the
//!    song's loud parts (its 95th percentile), so 1 is as loud as the song gets. Sections are runs
//!    of 4-bar phrases (8 s without a beat) at the same energy level, merged; see
//!    [`Analysis::sections`].

mod beats;
mod onset;
mod sections;

pub use beats::{bars, beat_grid, estimate_tempo};
pub use onset::{FRAME, HOP, OnsetEnvelope, onset_envelope, pick_onsets};
pub use sections::{Level, Section};

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
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub duration_ms: u64,
    /// Beats per minute, if the song has a steady pulse.
    pub tempo_bpm: Option<f32>,
    pub beats: Vec<u64>,
    /// The first beat of each bar (assuming 4/4).
    pub bars: Vec<u64>,
    pub onsets: Vec<u64>,
    /// How loud each second is, 0–1 (1 = as loud as the song's loud parts).
    #[serde(skip)]
    pub energy: Vec<f32>,
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
    let envelope = onset_envelope(checked, rate);
    if stopped {
        return Err(AnalysisError::Cancelled);
    }
    let onsets = pick_onsets(&envelope);
    let tempo = estimate_tempo(&envelope);
    let beat_frames = tempo
        .map(|period| beat_grid(&envelope, period, &onsets))
        .unwrap_or_default();
    let bar_frames = bars(&envelope, &beat_frames);
    let ms = |frames: &[usize]| frames.iter().map(|&f| envelope.time_ms(f)).collect::<Vec<_>>();
    Ok(Analysis {
        duration_ms: envelope.duration_ms(),
        tempo_bpm: tempo.map(|period| (60.0 / (period * envelope.frame_seconds())) as f32),
        beats: ms(&beat_frames),
        bars: ms(&bar_frames),
        onsets: ms(&onsets),
        energy: sections::energy_per_second(&envelope),
    })
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
    /// and "Onsets".
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
