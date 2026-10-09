//! The lead vocal brought forward, every ~6 ms, to lock sung words onto: where it starts a
//! sound (onsets), where it stops (offsets), and how loud it is. Not source separation, just
//! what a lead vocal usually is:
//!
//! - **In the middle of the mix.** Each frequency bin of the mid signal ((L + R) / 2) is kept by
//!   how alike the two sides are there: a bin where |L − R| is small next to |L + R| is centred
//!   (the voice, the kick, the bass); one where they're as big is a wide or panned instrument,
//!   and is turned down. A mono file is all middle.
//! - **Pitched**, for the vowels: the centred spectrum split into harmonic and percussive parts
//!   by median filtering (Fitzgerald, 2010), the harmonic part kept in the voice band
//!   (200–4000 Hz). Its loudness is [`VocalTrack::energy`], and its rises (spectral flux) are
//!   [`VocalTrack::onset`].
//! - **Hissing or popping**, for the consonants: the centred spectrum's rises in 2–8 kHz
//!   ([`VocalTrack::consonant`]), where "s", "t", and "k" are louder than the vowels.
//!
//! The voice is taken to stop ([`VocalTrack::offsets`]) where its loudness falls into the
//! quiet around it (under its local floor plus a share of the local range) and stays there
//! for [`OFFSET_HOLD_MS`]. A pitched centre instrument (a lead synth, a sax solo) looks like a
//! voice too; the lyrics' own timing comes first and this only fine-tunes it.

use crate::AnalysisError;
use crate::vocal::{ActivityExtractor, VocalActivity};
use pf_audio::StereoFrames;
use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;

/// The version of [`VocalTrack::to_bytes`], raised when it or how the track is made changes.
pub const VOCAL_TRACK_FORMAT: u32 = 1;
const MAGIC: &[u8; 4] = b"PFVT";

/// Where the voice's pitched sound sits (Hz).
const VOICE_HZ: (f32, f32) = (200.0, 4_000.0);
/// Where consonants are louder than vowels (Hz).
const CONSONANT_HZ: (f32, f32) = (2_000.0, 8_000.0);
/// The median along time for the harmonic part (ms).
const HARMONIC_MS: f64 = 150.0;
/// The median across frequency for the percussive part (Hz either side).
const PERCUSSIVE_HZ: f32 = 350.0;
/// Frames between the two compared for a rise.
const FLUX_LAG: usize = 2;
/// Log compression of magnitudes before comparing them.
const COMPRESS: f32 = 100.0;
/// Power treated as silence.
const FLOOR: f32 = 1e-10;
/// Below the song's loud vocal moments by this much (dB) is silence.
const SILENCE_DB: f32 = 50.0;
/// How long the voice must stay down to have stopped (ms).
pub const OFFSET_HOLD_MS: f64 = 70.0;
/// The stretch either side over which the voice's local floor and peak are taken (ms).
const LOCAL_MS: f64 = 1_000.0;
/// A rise in the voice's loudness this big (dB) or more counts fully.
pub const RISE_DB: f32 = 15.0;
/// What's usual for the rises is taken over this either side (ms).
const USUAL_MS: f64 = 150.0;
/// Onsets closer than this are one (ms).
const ONSET_GAP_MS: f64 = 60.0;
/// The flux rises a little after the sound starts (the window has to take enough of it in);
/// onsets are put this much earlier (ms).
const ONSET_LAG_MS: f64 = 6.0;
/// Samples between checks for a stop.
const CHECK_EVERY: usize = 1 << 16;

/// The lead vocal, a frame every [`VocalTrack::hop_ms`]; frame `i` stands for
/// `offset_ms + i * hop_ms`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VocalTrack {
    pub hop_ms: f64,
    /// The time frame 0 stands for (the middle of its window).
    pub offset_ms: f64,
    /// Whether the two sides differ (a mono file is all middle).
    pub stereo: bool,
    /// How loud the voice is, 0–1 (1 its loud moments, 0 [`SILENCE_DB`] under).
    pub energy: Vec<f32>,
    /// How much new pitched sound starts in the voice band, 0–1.
    pub onset: Vec<f32>,
    /// How much new hiss or pop starts in the consonant band, 0–1.
    pub consonant: Vec<f32>,
    /// How much louder the voice gets just here than just before, 0–1 (1 is [`RISE_DB`] or
    /// more): a sung word starting after a breath or a held consonant.
    pub rise: Vec<f32>,
    /// Whether the voice is sounding (above its local floor).
    pub voiced: Vec<bool>,
    /// Where pitched sound starts (ms), in order.
    pub onsets: Vec<u64>,
    /// Where hiss or pop starts (ms), in order.
    pub consonant_onsets: Vec<u64>,
    /// Where the voice stops (ms), in order.
    pub offsets: Vec<u64>,
    /// The slower measure of when the voice is sounding at all, for sung stretches.
    pub activity: VocalActivity,
}

impl VocalTrack {
    pub fn len(&self) -> usize {
        self.energy.len()
    }

    pub fn is_empty(&self) -> bool {
        self.energy.is_empty()
    }

    /// The time (ms) frame `i` stands for.
    pub fn time_ms(&self, i: usize) -> f64 {
        self.offset_ms + i as f64 * self.hop_ms
    }

    /// The frame nearest `ms` (`None` for an empty track).
    pub fn frame_at(&self, ms: f64) -> Option<usize> {
        if self.is_empty() || self.hop_ms <= 0.0 {
            return None;
        }
        let i = ((ms - self.offset_ms) / self.hop_ms).round().max(0.0) as usize;
        Some(i.min(self.len() - 1))
    }

    /// Whether the voice is sounding at `ms` (not past the end).
    pub fn is_voiced(&self, ms: f64) -> bool {
        let inside = ms >= self.offset_ms - self.hop_ms && ms <= self.time_ms(self.len()) + self.hop_ms;
        inside && self.frame_at(ms).is_some_and(|i| self.voiced[i])
    }

    /// The track as bytes, for a cache file.
    pub fn to_bytes(&self) -> Vec<u8> {
        let n = self.len();
        let mut out = Vec::with_capacity(48 + n * 17);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VOCAL_TRACK_FORMAT.to_le_bytes());
        out.push(u8::from(self.stereo));
        out.extend_from_slice(&self.hop_ms.to_le_bytes());
        out.extend_from_slice(&self.offset_ms.to_le_bytes());
        out.extend_from_slice(&(n as u32).to_le_bytes());
        for list in [&self.energy, &self.onset, &self.consonant, &self.rise] {
            for v in list {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out.extend(self.voiced.iter().map(|&v| u8::from(v)));
        for list in [&self.onsets, &self.consonant_onsets, &self.offsets] {
            put_times(&mut out, list);
        }
        out.extend_from_slice(&self.activity.hop_ms.to_le_bytes());
        out.extend_from_slice(&(self.activity.level.len() as u32).to_le_bytes());
        for v in &self.activity.level {
            out.extend_from_slice(&v.to_le_bytes());
        }
        put_times(&mut out, &self.activity.onsets);
        out
    }

    /// A track read back from [`VocalTrack::to_bytes`]; `None` when the bytes aren't one (or are
    /// of another format).
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let mut r = Reader(bytes);
        if r.take(4)? != MAGIC || r.u32()? != VOCAL_TRACK_FORMAT {
            return None;
        }
        let stereo = r.take(1)?[0] != 0;
        let hop_ms = r.f64()?;
        let offset_ms = r.f64()?;
        let n = r.u32()? as usize;
        let energy = r.floats(n)?;
        let onset = r.floats(n)?;
        let consonant = r.floats(n)?;
        let rise = r.floats(n)?;
        let voiced = r.take(n)?.iter().map(|&b| b != 0).collect();
        let onsets = r.times()?;
        let consonant_onsets = r.times()?;
        let offsets = r.times()?;
        let activity_hop = r.f64()?;
        let levels = r.u32()? as usize;
        let level = r.floats(levels)?;
        let activity_onsets = r.times()?;
        if !r.0.is_empty() || hop_ms.is_nan() || hop_ms <= 0.0 {
            return None;
        }
        Some(Self {
            hop_ms,
            offset_ms,
            stereo,
            energy,
            onset,
            consonant,
            rise,
            voiced,
            onsets,
            consonant_onsets,
            offsets,
            activity: VocalActivity {
                hop_ms: activity_hop,
                level,
                onsets: activity_onsets,
            },
        })
    }
}

fn put_times(out: &mut Vec<u8>, times: &[u64]) {
    out.extend_from_slice(&(times.len() as u32).to_le_bytes());
    for t in times {
        out.extend_from_slice(&t.to_le_bytes());
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (a, b) = self.0.split_at_checked(n)?;
        self.0 = b;
        Some(a)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn floats(&mut self, n: usize) -> Option<Vec<f32>> {
        let bytes = self.take(n.checked_mul(4)?)?;
        Some(
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|&c| f32::from_le_bytes(c))
                .collect(),
        )
    }

    fn times(&mut self) -> Option<Vec<u64>> {
        let n = self.u32()? as usize;
        let bytes = self.take(n.checked_mul(8)?)?;
        Some(
            bytes
                .as_chunks::<8>()
                .0
                .iter()
                .map(|&c| u64::from_le_bytes(c))
                .collect(),
        )
    }
}

/// Measures (left, right) pairs at `rate` Hz (a mono song: the same sample on both sides).
pub fn vocal_track(frames: impl IntoIterator<Item = (f32, f32)>, rate: u32) -> VocalTrack {
    match vocal_track_cancellable(frames, rate, &|| false) {
        Ok(track) => track,
        Err(_) => unreachable!("a measure that is never stopped finishes"),
    }
}

/// Like [`vocal_track`], checking `stop` every so often.
pub fn vocal_track_cancellable(
    frames: impl IntoIterator<Item = (f32, f32)>,
    rate: u32,
    stop: &dyn Fn() -> bool,
) -> Result<VocalTrack, AnalysisError> {
    let rate = rate.max(1);
    let limit = crate::MAX_ANALYSIS_MS.saturating_mul(u64::from(rate)) / 1000;
    let mut extractor = Extractor::new(rate);
    for (i, (l, r)) in frames.into_iter().take(limit as usize).enumerate() {
        if i % CHECK_EVERY == 0 && stop() {
            return Err(AnalysisError::Cancelled);
        }
        extractor.push(l, r);
    }
    if stop() {
        return Err(AnalysisError::Cancelled);
    }
    Ok(extractor.finish())
}

/// Decodes a music file and measures it (see [`vocal_track`]).
pub fn vocal_track_file(path: &Path, stop: &dyn Fn() -> bool) -> Result<VocalTrack, AnalysisError> {
    let frames = StereoFrames::open(path)?;
    let rate = frames.sample_rate();
    vocal_track_cancellable(frames, rate, stop)
}

/// One frame's centred spectrum, waiting for the frames after it.
struct Pending {
    magnitude: Vec<f32>,
    /// The median across frequency of `magnitude`.
    across: Vec<f32>,
}

struct Extractor {
    rate: u32,
    window: usize,
    hop: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    hann: Vec<f32>,
    left_in: Vec<f32>,
    right_in: Vec<f32>,
    left_out: Vec<Complex<f32>>,
    right_out: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    left: Vec<f32>,
    right: Vec<f32>,
    filled: usize,
    fresh: usize,
    /// The highest bin used, and the voice and consonant bands' bins.
    top: usize,
    voice: (usize, usize),
    consonants: (usize, usize),
    freq_half: usize,
    time_half: usize,
    pending: VecDeque<Pending>,
    first: usize,
    next: usize,
    /// The last [`FLUX_LAG`] separated frames' compressed voice-band harmonic part and
    /// consonant-band magnitudes, oldest first.
    history: VecDeque<(Vec<f32>, Vec<f32>)>,
    mid_power: f64,
    side_power: f64,
    energy: Vec<f32>,
    flux: Vec<f32>,
    hiss: Vec<f32>,
    activity: ActivityExtractor,
}

impl Extractor {
    fn new(rate: u32) -> Self {
        // A ~23 ms window (a power of two), every quarter window (~6 ms).
        let window = ((0.02 * f64::from(rate)) as usize)
            .next_power_of_two()
            .clamp(64, 4096);
        let hop = window / 4;
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(window);
        let bin_hz = rate as f32 / window as f32;
        let bin = |hz: f32| ((hz / bin_hz).round() as usize).clamp(1, window / 2);
        let top = bin(CONSONANT_HZ.1.min(rate as f32 * 0.475));
        let hop_ms = hop as f64 * 1000.0 / f64::from(rate);
        Self {
            rate,
            window,
            hop,
            hann: (0..window)
                .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / window as f32).cos())
                .collect(),
            left_in: fft.make_input_vec(),
            right_in: fft.make_input_vec(),
            left_out: fft.make_output_vec(),
            right_out: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            left: vec![0.0; window],
            right: vec![0.0; window],
            filled: 0,
            fresh: 0,
            top,
            voice: (bin(VOICE_HZ.0), bin(VOICE_HZ.1).min(top)),
            consonants: (bin(CONSONANT_HZ.0).min(top), top),
            freq_half: ((PERCUSSIVE_HZ / bin_hz).round() as usize).max(2),
            time_half: ((HARMONIC_MS / hop_ms / 2.0).round() as usize).max(2),
            pending: VecDeque::new(),
            first: 0,
            next: 0,
            history: VecDeque::new(),
            mid_power: 0.0,
            side_power: 0.0,
            energy: Vec::new(),
            flux: Vec::new(),
            hiss: Vec::new(),
            activity: ActivityExtractor::new(rate),
        }
    }

    fn push(&mut self, l: f32, r: f32) {
        let clean = |s: f32| if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
        let (l, r) = (clean(l), clean(r));
        self.activity.push(l, r);
        self.left[self.filled] = l;
        self.right[self.filled] = r;
        self.filled += 1;
        self.fresh += 1;
        if self.filled == self.window {
            self.frame();
            self.left.copy_within(self.hop.., 0);
            self.right.copy_within(self.hop.., 0);
            self.filled = self.window - self.hop;
        }
    }

    fn frame(&mut self) {
        self.fresh = 0;
        for k in 0..self.window {
            self.left_in[k] = self.left[k] * self.hann[k];
            self.right_in[k] = self.right[k] * self.hann[k];
        }
        // A plan's own buffers are always the right length.
        let _ = self
            .fft
            .process_with_scratch(&mut self.left_in, &mut self.left_out, &mut self.scratch);
        let _ = self
            .fft
            .process_with_scratch(&mut self.right_in, &mut self.right_out, &mut self.scratch);
        // Scaled so a full-scale sine is about 1 in its bin.
        let scale = 4.0 / self.window as f32;
        let mut magnitude = Vec::with_capacity(self.top + 1);
        for k in 0..=self.top {
            let (l, r) = (self.left_out[k], self.right_out[k]);
            let mid = (l + r).norm() * 0.5 * scale;
            let side = (l - r).norm() * 0.5 * scale;
            self.mid_power += f64::from(mid * mid);
            self.side_power += f64::from(side * side);
            // Alike on both sides: kept; as much apart as together (or more): turned down.
            let centred = (1.0 - 1.5 * side / (mid + 1e-9)).clamp(0.0, 1.0);
            magnitude.push(mid * centred * centred);
        }
        let across = median_filter(&magnitude, self.freq_half);
        self.pending.push_back(Pending { magnitude, across });
        let newest = self.first + self.pending.len() - 1;
        while self.next + self.time_half <= newest {
            self.separate();
        }
        while self.first + self.time_half < self.next {
            self.pending.pop_front();
            self.first += 1;
        }
    }

    /// Splits frame `next` into its harmonic and percussive parts, with the frames around it.
    fn separate(&mut self) {
        let at = self.next - self.first;
        let lo = at.saturating_sub(self.time_half);
        let hi = (at + self.time_half + 1).min(self.pending.len());
        let mut along = vec![0.0f32; hi - lo];
        let frame = &self.pending[at];
        let (v0, v1) = self.voice;
        let (c0, c1) = self.consonants;
        let mut energy = 0.0f32;
        let mut voice = Vec::with_capacity(v1 + 1 - v0);
        for k in v0..=v1 {
            for (slot, p) in along.iter_mut().zip(self.pending.range(lo..hi)) {
                *slot = p.magnitude[k];
            }
            let h = median(&mut along);
            let p = frame.across[k];
            let (h2, p2) = (h * h, p * p);
            let harmonic = if h2 + p2 > 0.0 { h2 / (h2 + p2) } else { 0.5 };
            let x = frame.magnitude[k] * harmonic;
            energy += x * x;
            voice.push((1.0 + COMPRESS * x).ln());
        }
        let hiss: Vec<f32> = frame.magnitude[c0..=c1]
            .iter()
            .map(|&x| (1.0 + COMPRESS * x).ln())
            .collect();
        let rise = |now: &[f32], before: &[f32]| -> f32 {
            now.iter().zip(before).map(|(a, b)| (a - b).max(0.0)).sum::<f32>() / now.len().max(1) as f32
        };
        let (flux, hissing) = match self.history.front() {
            Some((v, c)) if self.history.len() == FLUX_LAG => (rise(&voice, v), rise(&hiss, c)),
            _ => (0.0, 0.0),
        };
        self.energy.push(energy);
        self.flux.push(flux);
        self.hiss.push(hissing);
        self.history.push_back((voice, hiss));
        if self.history.len() > FLUX_LAG {
            self.history.pop_front();
        }
        self.next += 1;
    }

    fn finish(mut self) -> VocalTrack {
        if self.fresh > 0 {
            self.left[self.filled..].fill(0.0);
            self.right[self.filled..].fill(0.0);
            self.frame();
        }
        while self.next < self.first + self.pending.len() {
            self.separate();
        }
        let hop_ms = self.hop as f64 * 1000.0 / f64::from(self.rate);
        let offset_ms = self.window as f64 / 2.0 * 1000.0 / f64::from(self.rate);
        let stereo = self.side_power > self.mid_power * 1e-6;
        let activity = self.activity.finish();
        finish(
            Raw {
                energy: self.energy,
                flux: self.flux,
                hiss: self.hiss,
            },
            hop_ms,
            offset_ms,
            stereo,
            activity,
        )
    }
}

struct Raw {
    energy: Vec<f32>,
    flux: Vec<f32>,
    hiss: Vec<f32>,
}

/// The value at `share` of the way up `values`, sorted (0 when empty).
fn percentile(values: &[f32], share: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted[((sorted.len() - 1) as f32 * share) as usize]
}

fn finish(raw: Raw, hop_ms: f64, offset_ms: f64, stereo: bool, activity: VocalActivity) -> VocalTrack {
    let n = raw.energy.len();
    let db: Vec<f32> = raw.energy.iter().map(|&e| 10.0 * e.max(FLOOR).log10()).collect();
    let audible: Vec<f32> = db.iter().copied().filter(|&d| d > -90.0).collect();
    let loud = if audible.is_empty() {
        -100.0
    } else {
        percentile(&audible, 0.98)
    };
    let quiet = loud - SILENCE_DB;
    let energy: Vec<f32> = db
        .iter()
        .map(|&d| ((d - quiet) / SILENCE_DB).clamp(0.0, 1.0))
        .collect();
    // Rises in silence are nothing; the rest is how far each rises above what's usual around
    // it (busy music rises all the time), scaled by the song's big rises.
    let usual_half = ((USUAL_MS / hop_ms).round() as usize).max(1);
    let scaled = |flux: &[f32]| -> Vec<f32> {
        let usual = median_filter(flux, usual_half);
        let gated: Vec<f32> = flux
            .iter()
            .zip(&usual)
            .zip(&db)
            .map(|((&f, &u), &d)| {
                if d < quiet || audible.is_empty() {
                    0.0
                } else {
                    (f - u).max(0.0)
                }
            })
            .collect();
        let big = percentile(
            &gated.iter().copied().filter(|&f| f > 0.0).collect::<Vec<_>>(),
            0.99,
        );
        if big <= 0.0 {
            return vec![0.0; gated.len()];
        }
        gated.iter().map(|&f| (f / big).min(1.0)).collect()
    };
    let onset = scaled(&raw.flux);
    let consonant = scaled(&raw.hiss);
    let frames = |ms: f64| ((ms / hop_ms).round() as usize).max(1);
    let to_ms = |i: f64| (offset_ms + i * hop_ms).max(0.0).round() as u64;

    // Sounding: above the local floor by a share of the local range (and at least 6 dB).
    let smooth = moving_mean(&db, 2);
    let floor = sliding(&smooth, frames(LOCAL_MS), false);
    let peak = sliding(&smooth, frames(LOCAL_MS), true);
    let mut voiced: Vec<bool> = (0..n)
        .map(|i| {
            let line = floor[i] + (0.4 * (peak[i] - floor[i])).max(6.0);
            smooth[i] >= line && db[i] >= quiet
        })
        .collect();
    // Dips shorter than the hold aren't stops.
    let hold = frames(OFFSET_HOLD_MS);
    let mut i = 0;
    while i < n {
        if voiced[i] {
            i += 1;
            continue;
        }
        let end = (i..n).find(|&j| voiced[j]).unwrap_or(n);
        if end - i < hold && i > 0 && end < n {
            voiced[i..end].fill(true);
        }
        i = end;
    }
    // Louder just after than just before, and sounding after.
    let level = moving_mean(&energy, 1);
    let (ahead, behind) = (frames(12.0), frames(12.0));
    let rise: Vec<f32> = (0..n)
        .map(|i| {
            let after = (i + ahead).min(n - 1);
            if !voiced[after] {
                return 0.0;
            }
            let before = level[i.saturating_sub(behind)];
            ((level[after] - before) * SILENCE_DB / RISE_DB).clamp(0.0, 1.0)
        })
        .collect();
    let offsets: Vec<u64> = (1..n)
        .filter(|&i| voiced[i - 1] && !voiced[i])
        .map(|i| to_ms(i as f64))
        .collect();
    let lag = ONSET_LAG_MS / hop_ms;
    let pick = |env: &[f32]| -> Vec<u64> {
        pick_peaks(env, frames(25.0), frames(120.0), frames(ONSET_GAP_MS))
            .into_iter()
            .map(|i| to_ms(i as f64 - lag))
            .collect()
    };
    VocalTrack {
        hop_ms,
        offset_ms,
        stereo,
        onsets: pick(&onset),
        consonant_onsets: pick(&consonant),
        energy,
        onset,
        consonant,
        rise,
        voiced,
        offsets,
        activity,
    }
}

/// Peaks of `env`: the highest within `near` frames, standing above the mean within `around`
/// frames, at least `gap` frames apart (the stronger kept).
fn pick_peaks(env: &[f32], near: usize, around: usize, gap: usize) -> Vec<usize> {
    let mean = moving_mean(env, around);
    let mut peaks: Vec<usize> = Vec::new();
    for i in 0..env.len() {
        let v = env[i];
        let (lo, hi) = (i.saturating_sub(near), (i + near + 1).min(env.len()));
        let highest = env[lo..hi].iter().all(|&x| x <= v) && (lo..i).all(|j| env[j] < v);
        if !highest || v < 0.2 || v < 1.5 * mean[i] + 0.05 {
            continue;
        }
        match peaks.last() {
            Some(&p) if i - p < gap => {
                if v > env[p] {
                    *peaks.last_mut().unwrap_or(&mut 0) = i;
                }
            }
            _ => peaks.push(i),
        }
    }
    peaks
}

/// Each value replaced by the mean of those within `half` of it.
fn moving_mean(values: &[f32], half: usize) -> Vec<f32> {
    let mut sums = Vec::with_capacity(values.len() + 1);
    sums.push(0.0f64);
    for &v in values {
        sums.push(sums[sums.len() - 1] + f64::from(v));
    }
    (0..values.len())
        .map(|i| {
            let (lo, hi) = (i.saturating_sub(half), (i + half + 1).min(values.len()));
            ((sums[hi] - sums[lo]) / (hi - lo) as f64) as f32
        })
        .collect()
}

/// Each value replaced by the highest (or lowest) within `half` of it.
fn sliding(values: &[f32], half: usize, highest: bool) -> Vec<f32> {
    let better = |a: f32, b: f32| if highest { a >= b } else { a <= b };
    let mut out = Vec::with_capacity(values.len());
    let mut window: VecDeque<usize> = VecDeque::new();
    let n = values.len();
    let mut added = 0;
    for i in 0..n {
        while added < n && added <= i + half {
            while window.back().is_some_and(|&b| better(values[added], values[b])) {
                window.pop_back();
            }
            window.push_back(added);
            added += 1;
        }
        while window.front().is_some_and(|&f| f + half < i) {
            window.pop_front();
        }
        out.push(window.front().map_or(0.0, |&f| values[f]));
    }
    out
}

/// The median of `values` (reordering them).
fn median(values: &mut [f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mid = values.len() / 2;
    *values.select_nth_unstable_by(mid, f32::total_cmp).1
}

/// Each value replaced by the median of those within `half` of it.
fn median_filter(values: &[f32], half: usize) -> Vec<f32> {
    let mut scratch = vec![0.0f32; 2 * half + 1];
    (0..values.len())
        .map(|i| {
            let (lo, hi) = (i.saturating_sub(half), (i + half + 1).min(values.len()));
            let part = &mut scratch[..hi - lo];
            part.copy_from_slice(&values[lo..hi]);
            median(part)
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const RATE: u32 = 16_000;

    /// Cheap noise.
    pub fn noise(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    }

    /// A made-up song: a centred "voice" singing notes at `notes` (start, end ms), each starting
    /// with a short hiss, over wide instruments: a chord panned hard left, a different one hard
    /// right, and noise different on each side. `mono` puts it all in the middle instead.
    pub fn song(notes: &[(u64, u64)], length_ms: u64, mono: bool) -> Vec<(f32, f32)> {
        let n = (length_ms * u64::from(RATE) / 1000) as usize;
        let mut seed = 11;
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let ms = (i as u64 * 1000) / u64::from(RATE);
                let tone = |hz: f32| (std::f32::consts::TAU * hz * t).sin();
                let left_band = 0.08 * (tone(330.0) + tone(415.0) + tone(495.0));
                let right_band = 0.08 * (tone(262.0) + tone(311.0) + tone(392.0));
                let (nl, nr) = (noise(&mut seed) * 0.05, noise(&mut seed) * 0.05);
                let mut voice = 0.0;
                if let Some(&(start, end)) = notes.iter().find(|&&(s, e)| ms >= s && ms < e) {
                    // A short fade in and out, so the note itself has no click.
                    let from = (ms - start) as f32 / 10.0;
                    let to = (end - ms) as f32 / 10.0;
                    let fade = from.min(to).min(1.0);
                    let hz = 220.0 + 20.0 * (start % 7) as f32;
                    voice = (1..=6).map(|h| tone(hz * h as f32) / h as f32).sum::<f32>() * 0.25 * fade;
                    if ms - start < 40 {
                        voice += noise(&mut seed) * 0.3;
                    }
                }
                if mono {
                    let m = voice + (left_band + right_band) / 2.0 + nl;
                    (m, m)
                } else {
                    (voice + left_band + nl, voice + right_band + nr)
                }
            })
            .collect()
    }

    pub const NOTES: [(u64, u64); 6] = [
        (1_000, 1_400),
        (1_600, 2_300),
        (2_900, 3_200),
        (3_300, 4_100),
        (4_700, 5_000),
        (5_600, 6_400),
    ];

    fn nearest(times: &[u64], t: u64) -> u64 {
        times.iter().map(|&o| o.abs_diff(t)).min().unwrap_or(u64::MAX)
    }

    #[test]
    fn a_centred_voice_is_found_among_wide_instruments() {
        let track = vocal_track(song(&NOTES, 7_000, false), RATE);
        assert!(track.stereo);
        assert!((track.hop_ms - 8.0).abs() < 0.5, "{}", track.hop_ms);
        for &(start, end) in &NOTES {
            assert!(nearest(&track.onsets, start) <= 25, "{start}: {:?}", track.onsets);
            assert!(
                nearest(&track.consonant_onsets, start) <= 25,
                "{start}: {:?}",
                track.consonant_onsets
            );
            assert!(nearest(&track.offsets, end) <= 40, "{end}: {:?}", track.offsets);
            assert!(track.is_voiced(((start + end) / 2) as f64));
        }
        // The notes' starts stand well above anything else, and the voice isn't taken to sound
        // where only the instruments play.
        let starts = NOTES.map(|n| n.0);
        let mut elsewhere = 0.0f32;
        for i in 0..track.len() {
            if nearest(&starts, track.time_ms(i).round() as u64) > 60 {
                elsewhere = elsewhere.max(track.onset[i]);
            }
        }
        let peaks: Vec<f32> = starts
            .iter()
            .map(|&s| {
                (0..track.len())
                    .filter(|&i| track.time_ms(i).round() as u64 >= s.saturating_sub(25))
                    .take_while(|&i| track.time_ms(i).round() as u64 <= s + 25)
                    .map(|i| track.onset[i])
                    .fold(0.0, f32::max)
            })
            .collect();
        let weakest = peaks.iter().copied().fold(f32::MAX, f32::min);
        assert!(weakest > 2.0 * elsewhere, "{peaks:?} vs {elsewhere}");
        assert!(!track.is_voiced(500.0) && !track.is_voiced(6_700.0));
    }

    #[test]
    fn a_mono_song_is_all_middle() {
        let track = vocal_track(song(&NOTES, 7_000, true), RATE);
        assert!(!track.stereo);
        for &(start, _) in &NOTES {
            assert!(nearest(&track.onsets, start) <= 30, "{start}: {:?}", track.onsets);
        }
    }

    #[test]
    fn the_track_round_trips_as_bytes_and_stop_is_heard() {
        let track = vocal_track(song(&NOTES[..2], 2_500, false), RATE);
        assert_eq!(VocalTrack::from_bytes(&track.to_bytes()), Some(track.clone()));
        let mut bytes = track.to_bytes();
        bytes.pop();
        assert_eq!(VocalTrack::from_bytes(&bytes), None);
        assert_eq!(VocalTrack::from_bytes(b"PFAT"), None);
        let silent = vocal_track(vec![(0.0, 0.0); RATE as usize], RATE);
        assert!(silent.onsets.is_empty() && silent.offsets.is_empty());
        assert!(silent.voiced.iter().all(|v| !v));
        assert!(matches!(
            vocal_track_cancellable(vec![(0.0, 0.0); RATE as usize], RATE, &|| true),
            Err(AnalysisError::Cancelled)
        ));
    }
}
