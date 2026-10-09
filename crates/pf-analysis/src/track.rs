//! What the music is doing at each frame of a sequence, for effects that follow it (the render's
//! audio track, see `pf_render`'s `audio`).
//!
//! One pass over the song works out, for every frame (the sequence's frame time, 25 ms say):
//! - **peak** and **trough**: the frame's highest and lowest sample, as a share of the song's
//!   highest and lowest (xLights' `FrameData` `max` and `min`, which its Music value curves, music
//!   sparkles, and VU Meter read). xLights reads the left channel; here both are mixed to mono;
//! - **level**: how loud the frame is (its root mean square), 1 at the song's loudest frame and 0
//!   at [`RANGE_DB`] below it, so it follows loudness as it's heard (in dB);
//! - **bands**: bass (below 250 Hz), low mids (to 1 kHz), high mids (to 4 kHz), and treble, and a
//!   **spectrum** of [`SPECTRUM_BANDS`] bands log-spaced from 40 Hz to 16 kHz, each 0–1 the same
//!   way against its own loudest (or [`TILT_DB`] below the song's loudest band, when it never
//!   gets that loud), from a 2048-sample window around the frame's middle;
//! - **notes**: xLights' spectrogram (`AudioManager::CalculateSpectrumAnalysis`): for each MIDI
//!   note the strongest FFT bin in its range, as `log10`, over 2048-sample chunks taken one after
//!   another (a frame keeps the last chunk's when none starts in it), scaled to the song's
//!   strongest;
//! - **onset**: how much new sound starts in the frame (spectral flux), 1 at the song's strong
//!   onsets;
//! - **percussive**: the drums' loudness (the percussive part of the song, see [`crate::Layers`]),
//!   0–1 like the level;
//! - **note on** and **beat**: whether an onset or a beat falls in the frame.
//!
//! Frames follow the music's own clock: frame `i` starts at sample `i × frame_ms × rate / 1000`
//! (xLights rounds the samples per frame down, which drifts on long songs at 44.1 kHz).

use crate::layers::{LayerExtractor, Layers};
use crate::onset::{OnsetEnvelope, clean, onset_envelope, pick_onsets};
use crate::{AnalysisError, MAX_ANALYSIS_MS, beat_grid, estimate_tempo};
use pf_audio::MonoSamples;
use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use std::path::Path;
use std::sync::Arc;

/// Bands in [`AudioTrack::spectrum`].
pub const SPECTRUM_BANDS: usize = 32;
/// MIDI notes in [`AudioTrack::notes`] (0–126, as xLights' spectrogram).
pub const NOTES: usize = 127;
/// How far below the loudest a level of 0 is.
pub const RANGE_DB: f32 = 40.0;
/// How far below the song's loudest band a quiet band's own top is counted from (so a band that
/// only ever has a little leakage in it stays dark).
pub const TILT_DB: f32 = 30.0;
/// The window the bands and spectrum are taken from, and xLights' spectrogram chunks.
const WINDOW: usize = 2048;
/// Samples kept for the window around a frame's middle.
const RING: usize = 2 * WINDOW;
/// Where the named bands end (Hz): bass, low mids, high mids; treble is the rest.
const BAND_EDGES_HZ: [f32; 3] = [250.0, 1_000.0, 4_000.0];
/// The spectrum's range (Hz).
const SPECTRUM_LOW_HZ: f32 = 40.0;
const SPECTRUM_HIGH_HZ: f32 = 16_000.0;
/// Power treated as silence (-100 dB).
const FLOOR: f32 = 1e-10;
/// The format of [`AudioTrack::to_bytes`] (change it when the features change, so cached tracks
/// are worked out again).
pub const TRACK_FORMAT: u32 = 1;
const MAGIC: &[u8; 4] = b"PFAT";

const NOTE_ON: u8 = 1;
const BEAT: u8 = 2;

/// The music's features, a frame at a time. Frames past the end read as silence.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioTrack {
    frame_ms: u32,
    sample_rate: u32,
    peak: Vec<f32>,
    trough: Vec<f32>,
    level: Vec<f32>,
    bands: Vec<[f32; 4]>,
    onset: Vec<f32>,
    percussive: Vec<f32>,
    flags: Vec<u8>,
    spectrum: Vec<[u8; SPECTRUM_BANDS]>,
    notes: Vec<[u8; NOTES]>,
}

/// A value in 0–1 kept as a byte.
fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn unbyte(v: u8) -> f32 {
    f32::from(v) / 255.0
}

impl AudioTrack {
    /// The frame time the track was worked out for.
    pub fn frame_ms(&self) -> u32 {
        self.frame_ms
    }

    /// The music file's sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Frames with music.
    pub fn len(&self) -> usize {
        self.peak.len()
    }

    pub fn is_empty(&self) -> bool {
        self.peak.is_empty()
    }

    /// The frame playing at `t_ms`.
    pub fn frame_at(&self, t_ms: u64) -> u64 {
        t_ms / u64::from(self.frame_ms.max(1))
    }

    fn get<T: Copy>(values: &[T], frame: u64) -> Option<T> {
        usize::try_from(frame).ok().and_then(|i| values.get(i)).copied()
    }

    /// The frame's highest sample as a share of the song's (xLights' `max`), 0–1.
    pub fn peak(&self, frame: u64) -> f32 {
        Self::get(&self.peak, frame).unwrap_or(0.0)
    }

    /// The frame's lowest sample as a share of the song's lowest (xLights' `min`), 0–1.
    pub fn trough(&self, frame: u64) -> f32 {
        Self::get(&self.trough, frame).unwrap_or(0.0)
    }

    /// How loud the frame is, 0–1 (in dB: 1 the song's loudest, 0 [`RANGE_DB`] below).
    pub fn level(&self, frame: u64) -> f32 {
        Self::get(&self.level, frame).unwrap_or(0.0)
    }

    /// Bass, low mids, high mids, and treble, each 0–1.
    pub fn bands(&self, frame: u64) -> [f32; 4] {
        Self::get(&self.bands, frame).unwrap_or([0.0; 4])
    }

    pub fn bass(&self, frame: u64) -> f32 {
        self.bands(frame)[0]
    }

    pub fn low_mid(&self, frame: u64) -> f32 {
        self.bands(frame)[1]
    }

    pub fn high_mid(&self, frame: u64) -> f32 {
        self.bands(frame)[2]
    }

    pub fn treble(&self, frame: u64) -> f32 {
        self.bands(frame)[3]
    }

    /// The spectrum, lowest band first, each 0–1.
    pub fn spectrum(&self, frame: u64) -> [f32; SPECTRUM_BANDS] {
        Self::get(&self.spectrum, frame).map_or([0.0; SPECTRUM_BANDS], |s| s.map(unbyte))
    }

    /// Band `band` of the spectrum (0 the lowest), 0–1.
    pub fn band(&self, frame: u64, band: usize) -> f32 {
        Self::get(&self.spectrum, frame)
            .and_then(|s| s.get(band).copied())
            .map_or(0.0, unbyte)
    }

    /// The edges (Hz) of the spectrum's bands: band `i` runs from edge `i` to edge `i + 1`.
    pub fn spectrum_edges() -> [f32; SPECTRUM_BANDS + 1] {
        std::array::from_fn(|i| {
            SPECTRUM_LOW_HZ * (SPECTRUM_HIGH_HZ / SPECTRUM_LOW_HZ).powf(i as f32 / SPECTRUM_BANDS as f32)
        })
    }

    /// xLights' spectrogram for the frame: each MIDI note's strength, 0–1. `None` past the end.
    pub fn notes(&self, frame: u64) -> Option<[f32; NOTES]> {
        Self::get(&self.notes, frame).map(|n| n.map(unbyte))
    }

    /// One note of [`AudioTrack::notes`].
    pub fn note(&self, frame: u64, note: usize) -> f32 {
        Self::get(&self.notes, frame)
            .and_then(|n| n.get(note).copied())
            .map_or(0.0, unbyte)
    }

    /// How much new sound starts in the frame, 0–1.
    pub fn onset(&self, frame: u64) -> f32 {
        Self::get(&self.onset, frame).unwrap_or(0.0)
    }

    /// How loud the drums are, 0–1.
    pub fn percussive(&self, frame: u64) -> f32 {
        Self::get(&self.percussive, frame).unwrap_or(0.0)
    }

    /// Whether a new note (an onset) starts in the frame.
    pub fn is_note_on(&self, frame: u64) -> bool {
        Self::get(&self.flags, frame).is_some_and(|f| f & NOTE_ON != 0)
    }

    /// Whether a beat falls in the frame.
    pub fn is_beat(&self, frame: u64) -> bool {
        Self::get(&self.flags, frame).is_some_and(|f| f & BEAT != 0)
    }

    /// The track as bytes, for a cache file.
    pub fn to_bytes(&self) -> Vec<u8> {
        let n = self.len();
        let mut out = Vec::with_capacity(24 + n * (5 * 4 + 16 + 1 + SPECTRUM_BANDS + NOTES));
        out.extend_from_slice(MAGIC);
        for v in [TRACK_FORMAT, self.frame_ms, self.sample_rate, n as u32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for list in [
            &self.peak,
            &self.trough,
            &self.level,
            &self.onset,
            &self.percussive,
        ] {
            for v in list {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        for b in &self.bands {
            for v in b {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out.extend_from_slice(&self.flags);
        for s in &self.spectrum {
            out.extend_from_slice(s);
        }
        for s in &self.notes {
            out.extend_from_slice(s);
        }
        out
    }

    /// A track read back from [`AudioTrack::to_bytes`]; `None` when the bytes aren't one (or are
    /// of another format).
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let (head, mut rest) = bytes.split_at_checked(20)?;
        if &head[..4] != MAGIC {
            return None;
        }
        let word = |i: usize| u32::from_le_bytes([head[i], head[i + 1], head[i + 2], head[i + 3]]);
        let (format, frame_ms, sample_rate, n) = (word(4), word(8), word(12), word(16) as usize);
        if format != TRACK_FORMAT || frame_ms == 0 {
            return None;
        }
        let per_frame = 5 * 4 + 16 + 1 + SPECTRUM_BANDS + NOTES;
        if rest.len() != n.checked_mul(per_frame)? {
            return None;
        }
        let mut take = |len: usize| {
            let (a, b) = rest.split_at(len);
            rest = b;
            a
        };
        let floats = |b: &[u8]| -> Vec<f32> {
            b.as_chunks::<4>()
                .0
                .iter()
                .map(|&c| f32::from_le_bytes(c))
                .collect()
        };
        let peak = floats(take(4 * n));
        let trough = floats(take(4 * n));
        let level = floats(take(4 * n));
        let onset = floats(take(4 * n));
        let percussive = floats(take(4 * n));
        let bands = floats(take(16 * n)).as_chunks::<4>().0.to_vec();
        let flags = take(n).to_vec();
        let spectrum = take(SPECTRUM_BANDS * n).as_chunks::<SPECTRUM_BANDS>().0.to_vec();
        let notes = take(NOTES * n).as_chunks::<NOTES>().0.to_vec();
        Some(Self {
            frame_ms,
            sample_rate,
            peak,
            trough,
            level,
            bands,
            onset,
            percussive,
            flags,
            spectrum,
            notes,
        })
    }
}

/// The first sample of frame `i`.
fn frame_start(i: u64, frame_ms: u32, rate: u32) -> u64 {
    (u128::from(i) * u128::from(frame_ms) * u128::from(rate) / 1000) as u64
}

/// The frame sample `n` is in.
fn frame_of(n: u64, frame_ms: u32, rate: u32) -> u64 {
    let per = u128::from(frame_ms) * u128::from(rate);
    if per == 0 {
        return 0;
    }
    let mut i = (u128::from(n) * 1000 / per) as u64;
    while frame_start(i + 1, frame_ms, rate) <= n {
        i += 1;
    }
    while i > 0 && frame_start(i, frame_ms, rate) > n {
        i -= 1;
    }
    i
}

/// Reads samples one at a time and works out each frame's raw features.
struct Extractor {
    rate: u32,
    frame_ms: u32,
    /// Samples seen.
    n: u64,
    /// The frame being read, and the sample it ends before.
    frame: u64,
    frame_end: u64,
    sum_sq: f64,
    count: u32,
    max: f32,
    min: f32,
    peak: Vec<f32>,
    trough: Vec<f32>,
    rms: Vec<f32>,
    /// The last [`RING`] samples.
    ring: Vec<f32>,
    fft: Arc<dyn RealToComplex<f32>>,
    hann: Vec<f32>,
    input: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    /// The next frame to take the bands and spectrum of.
    next: u64,
    /// Per bin: its named band, and its spectrum band.
    band_of: Vec<Option<usize>>,
    spectrum_of: Vec<Option<usize>>,
    bands_db: Vec<[f32; 4]>,
    spectrum_db: Vec<[f32; SPECTRUM_BANDS]>,
    /// xLights' spectrogram: the chunk being filled, where it starts, and each finished chunk's
    /// notes with the frame it starts in.
    chunk: Vec<f32>,
    chunk_start: u64,
    chunks: Vec<(u64, [f32; NOTES])>,
    /// Per note: the bins xLights takes its strongest from (`None` when they run off the top).
    note_bins: Vec<Option<(usize, usize)>>,
}

impl Extractor {
    fn new(sample_rate: u32, frame_ms: u32) -> Self {
        let rate = sample_rate.max(1);
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(WINDOW);
        let bins = WINDOW / 2 + 1;
        let hz = |bin: usize| bin as f32 * rate as f32 / WINDOW as f32;
        let edges = AudioTrack::spectrum_edges();
        let band_of = (0..bins)
            .map(|k| {
                let f = hz(k);
                (k > 0).then(|| BAND_EDGES_HZ.iter().position(|&e| f < e).unwrap_or(3))
            })
            .collect();
        let spectrum_of = (0..bins)
            .map(|k| {
                let f = hz(k);
                (f >= edges[0] && f < edges[SPECTRUM_BANDS])
                    .then(|| edges.partition_point(|&e| e <= f).saturating_sub(1))
            })
            .collect();
        // `CalculateSpectrumAnalysis`: note j takes bins from its frequency to the next note's.
        let note_bins = (0..NOTES)
            .map(|j| {
                let at = |note: f64| 440.0 * ((note - 69.0) / 12.0).exp2();
                let start = (at(j as f64) * WINDOW as f64 / f64::from(rate)) as usize;
                let end = (at(j as f64 + 1.0) * WINDOW as f64 / f64::from(rate)) as usize;
                (end < bins - 1).then_some((start, end))
            })
            .collect();
        Self {
            rate,
            frame_ms: frame_ms.max(1),
            n: 0,
            frame: 0,
            frame_end: frame_start(1, frame_ms.max(1), rate),
            sum_sq: 0.0,
            count: 0,
            max: f32::MIN,
            min: f32::MAX,
            peak: Vec::new(),
            trough: Vec::new(),
            rms: Vec::new(),
            ring: vec![0.0; RING],
            hann: (0..WINDOW)
                .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / WINDOW as f32).cos())
                .collect(),
            input: fft.make_input_vec(),
            spectrum: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            next: 0,
            band_of,
            spectrum_of,
            bands_db: Vec::new(),
            spectrum_db: Vec::new(),
            chunk: Vec::with_capacity(WINDOW),
            chunk_start: 0,
            chunks: Vec::new(),
            note_bins,
        }
    }

    /// Takes the next sample (already cleaned up: finite and within -1..1).
    fn push(&mut self, s: f32) {
        while self.n >= self.frame_end {
            self.close_frame();
        }
        self.ring[(self.n % RING as u64) as usize] = s;
        self.n += 1;
        self.sum_sq += f64::from(s) * f64::from(s);
        self.count += 1;
        self.max = self.max.max(s);
        self.min = self.min.min(s);
        while self.middle(self.next) + (WINDOW / 2) as u64 <= self.n {
            self.window(self.next);
            self.next += 1;
        }
        self.chunk.push(s);
        if self.chunk.len() == WINDOW {
            self.take_chunk();
        }
    }

    fn close_frame(&mut self) {
        let count = self.count.max(1);
        self.peak.push(if self.count > 0 { self.max } else { 0.0 });
        self.trough.push(if self.count > 0 { self.min } else { 0.0 });
        self.rms.push((self.sum_sq / f64::from(count)).sqrt() as f32);
        self.frame += 1;
        self.frame_end = frame_start(self.frame + 1, self.frame_ms, self.rate);
        (self.sum_sq, self.count, self.max, self.min) = (0.0, 0, f32::MIN, f32::MAX);
    }

    /// The sample in the middle of frame `i`.
    fn middle(&self, i: u64) -> u64 {
        (frame_start(i, self.frame_ms, self.rate) + frame_start(i + 1, self.frame_ms, self.rate)) / 2
    }

    /// Frame `i`'s bands and spectrum, from the window around its middle (silence outside the
    /// song).
    fn window(&mut self, i: u64) {
        let first = self.middle(i) as i64 - (WINDOW / 2) as i64;
        for (k, (x, &w)) in self.input.iter_mut().zip(&self.hann).enumerate() {
            let at = first + k as i64;
            let s = if at < 0 || at as u64 >= self.n {
                0.0
            } else {
                self.ring[(at as u64 % RING as u64) as usize]
            };
            *x = s * w;
        }
        let mut bands = [0.0f32; 4];
        let mut spectrum = [0.0f32; SPECTRUM_BANDS];
        if self
            .fft
            .process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch)
            .is_ok()
        {
            // Scaled so a full-scale sine is about 0 dB in its bin.
            let scale = 4.0 / (WINDOW as f32 * WINDOW as f32 / 4.0);
            for ((c, band), spec) in self.spectrum.iter().zip(&self.band_of).zip(&self.spectrum_of) {
                let p = c.norm_sqr() * scale;
                if let Some(b) = band {
                    bands[*b] += p;
                }
                if let Some(b) = spec {
                    spectrum[*b] += p;
                }
            }
        }
        let db = |p: f32| 10.0 * p.max(FLOOR).log10();
        self.bands_db.push(bands.map(db));
        self.spectrum_db.push(spectrum.map(db));
    }

    /// xLights' spectrogram of the chunk just filled (`CalculateSpectrumAnalysis`: no window,
    /// unscaled magnitudes).
    fn take_chunk(&mut self) {
        self.input.copy_from_slice(&self.chunk);
        let mut notes = [0.0f32; NOTES];
        if self
            .fft
            .process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch)
            .is_ok()
        {
            for (note, bins) in notes.iter_mut().zip(&self.note_bins) {
                let Some((start, end)) = *bins else {
                    continue;
                };
                let strongest = self.spectrum[start..=end]
                    .iter()
                    .map(|c| c.norm())
                    .fold(0.0f32, f32::max);
                *note = strongest.log10().max(0.0);
            }
        }
        let frame = frame_of(self.chunk_start, self.frame_ms, self.rate);
        self.chunks.push((frame, notes));
        self.chunk_start += WINDOW as u64;
        self.chunk.clear();
    }

    /// Finishes the last frame, and the windows of the frames whose window runs past the end.
    fn finish(mut self) -> Self {
        if self.count > 0 {
            self.close_frame();
        }
        let frames = self.peak.len() as u64;
        while self.next < frames {
            self.window(self.next);
            self.next += 1;
        }
        self.bands_db.truncate(self.peak.len());
        self.spectrum_db.truncate(self.peak.len());
        self
    }
}

/// `db` as 0–1: 1 at `top`, 0 at [`RANGE_DB`] below it (and in silence).
fn scaled(db: f32, top: f32) -> f32 {
    if db <= -99.0 {
        0.0
    } else {
        ((db - top) / RANGE_DB + 1.0).clamp(0.0, 1.0)
    }
}

/// Each band's own top, or [`TILT_DB`] below the loudest band's, whichever is louder.
fn band_tops<const N: usize>(frames: &[[f32; N]]) -> [f32; N] {
    let mut tops = [-100.0f32; N];
    for f in frames {
        for (t, &v) in tops.iter_mut().zip(f) {
            *t = t.max(v);
        }
    }
    let loudest = tops.iter().copied().fold(-100.0f32, f32::max);
    tops.map(|t| t.max(loudest - TILT_DB))
}

/// Puts the raw features together into the track.
fn assemble(x: Extractor, envelope: &OnsetEnvelope, layers: &Layers) -> AudioTrack {
    let n = x.peak.len();
    let frame_ms = x.frame_ms;
    let frame_for_ms = |t: u64| (t / u64::from(frame_ms)) as usize;

    let highest = x.peak.iter().copied().fold(0.0f32, f32::max);
    let lowest = x.trough.iter().copied().fold(0.0f32, f32::min);
    let share = |v: f32, of: f32| if of != 0.0 { (v / of).clamp(0.0, 1.0) } else { 0.0 };
    let peak = x.peak.iter().map(|&v| share(v, highest)).collect();
    let trough = x.trough.iter().map(|&v| share(v, lowest)).collect();

    let rms_db: Vec<f32> = x.rms.iter().map(|&r| 20.0 * r.max(1e-5).log10()).collect();
    let top = rms_db.iter().copied().fold(-100.0f32, f32::max);
    let level = rms_db.iter().map(|&db| scaled(db, top)).collect();

    let tops = band_tops(&x.bands_db);
    let bands = x
        .bands_db
        .iter()
        .map(|b| std::array::from_fn(|i| scaled(b[i], tops[i])))
        .collect();
    let tops = band_tops(&x.spectrum_db);
    let spectrum = x
        .spectrum_db
        .iter()
        .map(|b| std::array::from_fn(|i| byte(scaled(b[i], tops[i]))))
        .collect();

    // A frame takes the strongest of the chunks starting in it, else the frame before's.
    let strongest = x
        .chunks
        .iter()
        .flat_map(|(_, notes)| notes.iter().copied())
        .fold(0.0f32, f32::max);
    let mut notes = vec![[0u8; NOTES]; n];
    let mut chunks = x.chunks.iter().peekable();
    let mut last = [0.0f32; NOTES];
    for (i, out) in notes.iter_mut().enumerate() {
        let mut fresh: Option<[f32; NOTES]> = None;
        while let Some((_, values)) = chunks.next_if(|(f, _)| *f as usize <= i) {
            let merged = fresh.get_or_insert([0.0; NOTES]);
            for (m, &v) in merged.iter_mut().zip(values) {
                *m = m.max(v);
            }
        }
        if let Some(fresh) = fresh {
            last = fresh;
        }
        *out = last.map(|v| byte(if strongest > 0.0 { v / strongest } else { 0.0 }));
    }

    // Onsets: the envelope's strongest in each frame, against its strong onsets (the 99th
    // percentile, so one freak spike doesn't flatten the rest).
    let mut onset = vec![0.0f32; n];
    for (f, &v) in envelope.values.iter().enumerate() {
        if let Some(slot) = onset.get_mut(frame_for_ms(envelope.time_ms(f))) {
            *slot = slot.max(v);
        }
    }
    let mut sorted: Vec<f32> = envelope.values.iter().copied().filter(|&v| v > 0.0).collect();
    sorted.sort_by(f32::total_cmp);
    let strong = sorted
        .get(((sorted.len() as f64 * 0.99) as usize).min(sorted.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0.0);
    for v in &mut onset {
        *v = if strong > 0.0 { (*v / strong).min(1.0) } else { 0.0 };
    }

    let mut drums = vec![-100.0f32; n];
    for (f, &db) in layers.percussive_db.iter().enumerate() {
        let t = (layers.time_s(f) * 1000.0).max(0.0) as u64;
        if let Some(slot) = drums.get_mut(frame_for_ms(t)) {
            *slot = slot.max(db);
        }
    }
    let top = drums.iter().copied().fold(-100.0f32, f32::max);
    let percussive = drums.iter().map(|&db| scaled(db, top)).collect();

    let mut flags = vec![0u8; n];
    let onsets = pick_onsets(envelope);
    for &f in &onsets {
        if let Some(flag) = flags.get_mut(frame_for_ms(envelope.time_ms(f))) {
            *flag |= NOTE_ON;
        }
    }
    if let Some(period) = estimate_tempo(envelope) {
        for f in beat_grid(envelope, period, &onsets) {
            if let Some(flag) = flags.get_mut(frame_for_ms(envelope.time_ms(f))) {
                *flag |= BEAT;
            }
        }
    }

    AudioTrack {
        frame_ms,
        sample_rate: x.rate,
        peak,
        trough,
        level,
        bands,
        onset,
        percussive,
        flags,
        spectrum,
        notes,
    }
}

/// Samples between checks for a stop.
const CHECK_EVERY: usize = 1 << 16;

/// The audio track of mono samples at `sample_rate`, a frame every `frame_ms`.
pub fn audio_track(samples: impl IntoIterator<Item = f32>, sample_rate: u32, frame_ms: u32) -> AudioTrack {
    match audio_track_cancellable(samples, sample_rate, frame_ms, &|| false) {
        Ok(track) => track,
        Err(_) => unreachable!("a track that is never stopped finishes"),
    }
}

/// Like [`audio_track`], checking `stop` every so often and giving up with
/// [`AnalysisError::Cancelled`] once it says so.
pub fn audio_track_cancellable(
    samples: impl IntoIterator<Item = f32>,
    sample_rate: u32,
    frame_ms: u32,
    stop: &dyn Fn() -> bool,
) -> Result<AudioTrack, AnalysisError> {
    let rate = sample_rate.max(1);
    let limit = MAX_ANALYSIS_MS.saturating_mul(u64::from(rate)) / 1000;
    let mut stopped = false;
    let mut extractor = Extractor::new(rate, frame_ms);
    let mut separator = LayerExtractor::new(rate);
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
    let envelope = onset_envelope(
        checked.inspect(|&s| {
            let s = clean(s);
            extractor.push(s);
            separator.push(s);
        }),
        rate,
    );
    if stopped || stop() {
        return Err(AnalysisError::Cancelled);
    }
    Ok(assemble(extractor.finish(), &envelope, &separator.finish()))
}

/// Decodes a music file and works out its audio track (see [`audio_track_cancellable`]).
pub fn audio_track_file(
    path: &Path,
    frame_ms: u32,
    stop: &dyn Fn() -> bool,
) -> Result<AudioTrack, AnalysisError> {
    let samples = MonoSamples::open(path)?;
    let rate = samples.sample_rate();
    let track = audio_track_cancellable(samples, rate, frame_ms, stop)?;
    if track.is_empty() {
        return Err(AnalysisError::Empty);
    }
    Ok(track)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const RATE: u32 = 44_100;

    fn tone(hz: f32, amplitude: f32, seconds: f32) -> Vec<f32> {
        (0..(seconds * RATE as f32) as usize)
            .map(|i| amplitude * (TAU * hz * i as f32 / RATE as f32).sin())
            .collect()
    }

    #[test]
    fn frames_follow_the_music_clock() {
        // 25 ms at 44.1 kHz is 1102.5 samples: frames alternate 1102 and 1103 samples.
        assert_eq!(frame_start(1, 25, RATE), 1102);
        assert_eq!(frame_start(2, 25, RATE), 2205);
        assert_eq!(frame_start(40 * 360, 25, RATE), 360 * RATE as u64);
        for n in [0, 1101, 1102, 2204, 2205, 999_999] {
            let f = frame_of(n, 25, RATE);
            assert!(
                frame_start(f, 25, RATE) <= n && n < frame_start(f + 1, 25, RATE),
                "{n}"
            );
        }
        let track = audio_track(tone(440.0, 0.5, 2.0), RATE, 25);
        assert_eq!(track.len(), 80);
        assert_eq!(track.frame_at(1999), 79);
        assert_eq!(track.peak(80), 0.0, "past the end is silence");
    }

    #[test]
    fn peaks_are_shares_of_the_loudest_sample_and_levels_follow_loudness_in_db() {
        // Two seconds loud, then two at a tenth (20 dB down), then silence.
        let mut samples = tone(440.0, 0.8, 2.0);
        samples.extend(tone(440.0, 0.08, 2.0));
        samples.extend(vec![0.0; RATE as usize]);
        let t = audio_track(samples, RATE, 50);
        assert!((t.peak(10) - 1.0).abs() < 0.01, "{}", t.peak(10));
        assert!((t.peak(60) - 0.1).abs() < 0.01, "{}", t.peak(60));
        assert!((t.trough(10) - 1.0).abs() < 0.01);
        assert!((t.level(10) - 1.0).abs() < 0.01);
        assert!(
            (t.level(60) - (1.0 - 20.0 / RANGE_DB)).abs() < 0.02,
            "{}",
            t.level(60)
        );
        assert_eq!((t.peak(90), t.level(90)), (0.0, 0.0));
    }

    #[test]
    fn a_tone_lights_its_band_its_spectrum_band_and_its_note() {
        // A 60 Hz bass tone, then a 6 kHz treble one.
        let mut samples = tone(60.0, 0.5, 1.0);
        samples.extend(tone(6_000.0, 0.5, 1.0));
        let t = audio_track(samples, RATE, 25);
        let (bass, treble) = (20, 60);
        assert!(
            t.bass(bass) > 0.95 && t.treble(bass) < 0.05,
            "{:?}",
            t.bands(bass)
        );
        assert!(
            t.treble(treble) > 0.95 && t.bass(treble) < 0.05,
            "{:?}",
            t.bands(treble)
        );
        assert!(t.low_mid(bass) < 0.3 && t.high_mid(treble) < 0.5);
        let edges = AudioTrack::spectrum_edges();
        let band_of = |hz: f32| edges.partition_point(|&e| e <= hz) - 1;
        let s = t.spectrum(bass);
        assert!(s[band_of(60.0)] > 0.95, "{s:?}");
        assert!(s[band_of(1_000.0)] < 0.05 && s[band_of(6_000.0)] < 0.05, "{s:?}");
        assert!(t.band(treble, band_of(6_000.0)) > 0.95 && t.band(treble, band_of(60.0)) < 0.05);
        // xLights' spectrogram: 6 kHz is MIDI note 114, 60 Hz between 34 and 35 (low notes share
        // bins, so the first of the strongest).
        let strongest = |frame: u64| {
            let notes = t.notes(frame).unwrap();
            let top = notes.iter().copied().fold(0.0, f32::max);
            notes.iter().position(|&v| v == top).unwrap()
        };
        assert!((113..=115).contains(&strongest(treble)), "{}", strongest(treble));
        assert!((33..=37).contains(&strongest(bass)), "{}", strongest(bass));
        assert!(
            t.note(bass, 35) > 0.9 && t.note(bass, 90) < 0.6,
            "{:?}",
            t.notes(bass)
        );
    }

    #[test]
    fn clicks_are_onsets_beats_and_drums() {
        // A click every half second (120 BPM) over a quiet hum.
        let n = 8 * RATE as usize;
        let samples: Vec<f32> = (0..n)
            .map(|i| {
                let click = if i % (RATE as usize / 2) < 40 { 0.9 } else { 0.0 };
                click + 0.02 * (TAU * 220.0 * i as f32 / RATE as f32).sin()
            })
            .collect();
        let t = audio_track(samples, RATE, 25);
        let frames = t.len() as u64;
        let on: Vec<u64> = (0..frames).filter(|&f| t.is_note_on(f)).collect();
        assert!(on.len() >= 14, "{on:?}");
        // Each onset falls on a click (every 20 frames), within a frame.
        assert!(on.iter().all(|f| f % 20 <= 1 || f % 20 == 19), "{on:?}");
        let beats: Vec<u64> = (0..frames).filter(|&f| t.is_beat(f)).collect();
        assert!(beats.len() >= 12, "{beats:?}");
        assert!(
            beats.windows(2).all(|w| (19..=21).contains(&(w[1] - w[0]))),
            "{beats:?}"
        );
        // The onset strength and the drums peak at the clicks.
        assert!(t.onset(on[3]) > 0.5 && t.onset(on[3] + 10) < 0.2);
        assert!(t.percussive(on[3]) > t.percussive(on[3] + 10) + 0.3);
    }

    #[test]
    fn tracks_survive_the_round_trip_through_bytes() {
        let t = audio_track(tone(300.0, 0.4, 1.5), RATE, 25);
        let bytes = t.to_bytes();
        assert_eq!(AudioTrack::from_bytes(&bytes), Some(t));
        assert_eq!(AudioTrack::from_bytes(&bytes[..bytes.len() - 1]), None);
        let mut other = bytes.clone();
        other[4] = 99;
        assert_eq!(AudioTrack::from_bytes(&other), None, "another format");
        assert_eq!(AudioTrack::from_bytes(b"nope"), None);
    }

    #[test]
    fn silence_and_nothing() {
        let t = audio_track(vec![0.0; 10_000], RATE, 25);
        assert!(!t.is_empty());
        assert!((0..t.len() as u64).all(|f| t.peak(f) == 0.0 && t.level(f) == 0.0 && t.bass(f) == 0.0));
        assert!(audio_track(std::iter::empty(), RATE, 25).is_empty());
        let stopped = audio_track_cancellable(tone(440.0, 0.5, 1.0), RATE, 25, &|| true);
        assert!(matches!(stopped, Err(AnalysisError::Cancelled)));
    }
}
