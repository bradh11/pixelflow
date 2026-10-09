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
//! 9. **Drums and the rest**: alongside, every ~12 ms, the song split into its percussive and
//!    harmonic parts by median filtering ([`Layers`]), and the drum hits in the percussive part
//!    told apart: kick, snare, hi-hat, crash ([`Analysis::drums`], [`Analysis::bar_drums`]).
//! 10. **Moments**: from all of that, what makes a show dramatic, ranked by importance:
//!     impacts, stops and restarts, breakdowns, builds, fills, peaks, holds, key changes,
//!     shouts, drops, crashes, and section changes ([`Analysis::moments`]); shouts again from
//!     the sung words when there are lyrics ([`Analysis::moments_with_words`]).
//! 11. **Voice**: separately, a cheap guess at when the voice is sounding ([`vocal_activity`]),
//!     to fine-tune lyric timing.
//! 12. **Audio track**: separately, what the music is doing at every frame of a sequence (levels,
//!     bands, spectrum, onsets, beats) for effects that follow it ([`audio_track`]).

mod beats;
mod drums;
mod energy;
mod events;
mod features;
mod grid;
mod key;
mod layers;
mod meter;
mod moments;
mod onset;
mod rises;
mod sections;
mod stops;
mod structure;
mod track;
mod vocal;

pub use beats::{bars, beat_grid, estimate_tempo};
pub use drums::{BarDrums, Drum, DrumHit};
pub use energy::BarEnergy;
pub use events::{Event, EventKind};
pub use layers::Layers;
pub use moments::{Moment, MomentKind, ShoutCues, Suggest, moments_track};
pub use onset::{FRAME, HOP, OnsetEnvelope, onset_envelope, pick_onsets};
pub use sections::{Level, Section};
pub use track::{
    AudioTrack, NOTES, RANGE_DB, SPECTRUM_BANDS, TILT_DB, TRACK_FORMAT, audio_track, audio_track_cancellable,
    audio_track_file,
};
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
    /// What makes the song dramatic, ranked ([`Moment::importance`]), in time order.
    pub moments: Vec<Moment>,
    /// The notable drum hits (crashes, and kicks and snares harder than those around), in time
    /// order; every bar's counts are in `bar_drums`.
    pub drums: Vec<DrumHit>,
    /// Each bar's drum hits (one per entry in `bars`).
    pub bar_drums: Vec<BarDrums>,
    pub confidence: Confidence,
    /// What shouts are found from, for lyrics that come later.
    #[serde(skip)]
    pub shout_cues: ShoutCues,
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
    let mut separator = layers::LayerExtractor::new(rate);
    let envelope = onset_envelope(
        checked.inspect(|&s| {
            let s = onset::clean(s);
            extractor.push(s);
            separator.push(s);
        }),
        rate,
    );
    if stopped || stop() {
        return Err(AnalysisError::Cancelled);
    }
    let features = extractor.finish();
    let layers = separator.finish();
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
    if stop() {
        return Err(AnalysisError::Cancelled);
    }
    let drum_onsets = drums::onsets(&layers);
    let bar_drums = drums::bar_drums(&drum_onsets, &seconds(&bars), end);
    let mut analysis = Analysis {
        duration_ms,
        tempo_bpm: tempo.map(|period| (60.0 / (period * envelope.frame_seconds())) as f32),
        beats,
        bars,
        onsets: ms(&onsets),
        energy: sections::energy_per_second(&envelope),
        sections,
        events,
        bar_energy,
        drums: drums::notable(&drum_onsets, end),
        bar_drums,
        confidence,
        ..Analysis::default()
    };
    let (found, cues) = find_moments(&analysis, &layers, &features, &drum_onsets);
    analysis.moments = moments::rank(found, moments::from_events(&analysis.events), &analysis);
    analysis.shout_cues = cues;
    Ok(analysis)
}

/// Every detector's moments in `analysis` (all but its moments filled in), and what shouts are
/// found from.
fn find_moments(
    analysis: &Analysis,
    layers: &Layers,
    features: &features::Features,
    onsets: &[drums::Onset],
) -> (Vec<moments::Found>, ShoutCues) {
    let seconds = |times: &[u64]| times.iter().map(|&t| t as f64 / 1000.0).collect::<Vec<_>>();
    let (beats, bars) = (seconds(&analysis.beats), seconds(&analysis.bars));
    let song = moments::Song {
        layers,
        features,
        onsets,
        beats: &beats,
        bars: &bars,
        sections: &analysis.sections,
        bar_drums: &analysis.bar_drums,
        bar_energy: &analysis.bar_energy,
        end: analysis.duration_ms as f64 / 1000.0,
        beat: analysis
            .tempo_bpm
            .filter(|t| *t > 0.0)
            .map_or(0.5, |t| 60.0 / f64::from(t)),
        levels: moments::Levels::new(layers),
    };
    let (mut found, gaps) = stops::stops(&song);
    let impacts = rises::impacts(&song);
    found.extend(rises::builds(&song, &impacts));
    found.extend(impacts);
    let breakdowns = stops::breakdowns(&song);
    // A chord held through a breakdown is the breakdown; one at the end stays a hold.
    let within = |h: &moments::Found| {
        breakdowns.iter().any(|b| {
            let (a, z) = (h.at.max(b.at), h.end.unwrap_or(h.at).min(b.end.unwrap_or(b.at)));
            h.label.is_none() && z - a > 0.5 * (h.end.unwrap_or(h.at) - h.at)
        })
    };
    found.extend(stops::holds(&song).into_iter().filter(|h| !within(h)));
    found.extend(breakdowns);
    found.extend(rises::fills(&song));
    found.extend(rises::peaks(&song));
    found.extend(key::key_changes(&song));
    found.extend(moments::section_changes(&analysis.sections));
    let cues = ShoutCues {
        frame_s: layers.frame_seconds(),
        offset_s: layers.time_s(0),
        voice_db: layers.voice_db.clone(),
        hits: song.audible().map(|o| (o.time_s, o.strength())).collect(),
        gaps,
    };
    found.extend(moments::vocal_bursts(&cues, song.end));
    (found, cues)
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
    /// and "Onsets". See also [`Analysis::sections_track`], [`Analysis::accents_track`],
    /// [`Analysis::moments_track`], and [`Analysis::drums_track`].
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
