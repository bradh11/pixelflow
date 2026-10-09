//! The song split into its drums and the rest: harmonic/percussive separation by median
//! filtering (Fitzgerald, 2010).
//!
//! Every ~12 ms, a 23 ms window's spectrum is taken (bins up to 4 kHz one by one, above that
//! about 170 Hz at a time, up to 16 kHz). Sustained sound (notes, chords, the voice) is steady
//! along time and peaky across frequency; a drum hit is the opposite: a short, broadband
//! burst. So a median along time (over ~200 ms) keeps the harmonic part of each bin and a
//! median across frequency (over 17 bins) the percussive part, and each bin is shared between
//! the two in proportion (soft masks). The medians along time need ~100 ms of the song ahead,
//! so frames are separated a little behind the samples as they go by: the song is still read
//! once, and only the summaries below are kept.

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use std::collections::VecDeque;
use std::sync::Arc;

/// Frames either side in the median along time (17 frames, ~200 ms).
const TIME_HALF: usize = 8;
/// Bins either side in the median across frequency.
const FREQ_HALF: usize = 8;
/// Bins are kept one by one up to here (Hz), then grouped.
const FINE_HZ: f32 = 4_000.0;
/// The width of a group above `FINE_HZ` (Hz), and the highest frequency used.
const GROUP_HZ: f32 = 172.0;
const TOP_HZ: f32 = 16_000.0;
/// Power treated as silence (about -100 dB).
const FLOOR: f32 = 1e-10;
/// The drum bands (Hz): kick, snare body (and toms), snare rattle, and hi-hats and cymbals.
pub(crate) const DRUM_BANDS: [(f32, f32); 4] = [
    (35.0, 130.0),
    (140.0, 420.0),
    (2_000.0, 6_000.0),
    (6_000.0, TOP_HZ),
];
pub(crate) const KICK: usize = 0;
pub(crate) const BODY: usize = 1;
pub(crate) const RATTLE: usize = 2;
pub(crate) const CYMBAL: usize = 3;
/// Where the voice (and lead instruments) sits, for shouts (Hz).
const VOICE_HZ: (f32, f32) = (300.0, 3_000.0);

/// The song's drums and the rest, a frame every [`Layers::frame_seconds`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layers {
    pub sample_rate: u32,
    pub window: usize,
    pub hop: usize,
    /// Power (dB) of the percussive and the harmonic parts, and of both.
    pub percussive_db: Vec<f32>,
    pub harmonic_db: Vec<f32>,
    pub total_db: Vec<f32>,
    /// The percussive part's power (dB) in each of the drum bands.
    pub drums_db: Vec<[f32; 4]>,
    /// How much new percussive sound starts (spectral flux of the percussive part).
    pub percussive_flux: Vec<f32>,
    /// The harmonic part's power (dB) where the voice sits.
    pub voice_db: Vec<f32>,
    /// The whole sound's power (dB) in the cymbal band, for how long a cymbal rings (its
    /// steady tail goes to the harmonic part).
    pub cymbal_db: Vec<f32>,
    /// The spectral centroid (Hz) of the whole sound: how bright it is.
    pub centroid_hz: Vec<f32>,
}

impl Layers {
    /// Seconds between frames.
    pub fn frame_seconds(&self) -> f64 {
        self.hop as f64 / f64::from(self.sample_rate.max(1))
    }

    /// The time (s) frame `i` stands for: the middle of its window.
    pub fn time_s(&self, i: usize) -> f64 {
        (i * self.hop + self.window / 2) as f64 / f64::from(self.sample_rate.max(1))
    }

    /// The frame whose middle is nearest `seconds`.
    pub fn frame_at(&self, seconds: f64) -> usize {
        let half = self.window as f64 / 2.0 / f64::from(self.sample_rate.max(1));
        let frame = ((seconds - half) / self.frame_seconds()).round().max(0.0) as usize;
        frame.min(self.len().saturating_sub(1))
    }

    /// Frames per `seconds` (at least 1).
    pub fn frames_for(&self, seconds: f64) -> usize {
        (seconds / self.frame_seconds()).round().max(1.0) as usize
    }

    pub fn len(&self) -> usize {
        self.percussive_db.len()
    }

    pub fn is_empty(&self) -> bool {
        self.percussive_db.is_empty()
    }
}

/// One frame's spectrum, waiting for the frames after it.
struct Pending {
    /// Magnitude per group (root of the mean power of its bins).
    magnitude: Vec<f32>,
    /// The median across frequency of `magnitude`.
    across: Vec<f32>,
    total: f32,
    cymbal: f32,
    centroid: f32,
}

/// Reads samples one at a time and separates the frames as soon as the frames after them are in.
pub(crate) struct LayerExtractor {
    window: usize,
    hop: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    hann: Vec<f32>,
    input: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    buffer: Vec<f32>,
    filled: usize,
    fresh: usize,
    /// Per group: its first bin, how many bins, and its middle (Hz).
    groups: Vec<(usize, usize, f32)>,
    /// Per group: the drum band it's in, and whether it's in the voice band.
    drum_band: Vec<Option<usize>>,
    voice: Vec<bool>,
    /// The frames not yet separated (and the `TIME_HALF` before them), oldest first.
    pending: VecDeque<Pending>,
    /// The index of `pending`'s first frame, and of the next frame to separate.
    first: usize,
    next: usize,
    /// The last separated frame's percussive part (log-compressed), for its flux.
    previous: Vec<f32>,
    layers: Layers,
}

impl LayerExtractor {
    pub(crate) fn new(sample_rate: u32) -> Self {
        let rate = sample_rate.max(1);
        // About 12 ms between frames, a power of two (512 at 44.1 kHz); windows twice as long.
        let hop = (0.0116 * f64::from(rate)).log2().round().clamp(4.0, 12.0).exp2() as usize;
        let window = 2 * hop;
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(window);
        let bin_hz = rate as f32 / window as f32;
        let last = ((TOP_HZ.min(rate as f32 * 0.475) / bin_hz) as usize).clamp(1, window / 2);
        let per_group = ((GROUP_HZ / bin_hz).round() as usize).max(1);
        let mut groups = Vec::new();
        let mut bin = 1;
        while bin <= last {
            let n = if (bin as f32) * bin_hz < FINE_HZ {
                1
            } else {
                per_group.min(last + 1 - bin)
            };
            groups.push((bin, n, (bin as f32 + (n as f32 - 1.0) / 2.0) * bin_hz));
            bin += n;
        }
        let drum_band = groups
            .iter()
            .map(|&(_, _, hz)| DRUM_BANDS.iter().position(|&(lo, hi)| hz >= lo && hz < hi))
            .collect();
        let voice = groups
            .iter()
            .map(|&(_, _, hz)| hz >= VOICE_HZ.0 && hz < VOICE_HZ.1)
            .collect();
        Self {
            window,
            hop,
            hann: (0..window)
                .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / window as f32).cos())
                .collect(),
            input: fft.make_input_vec(),
            spectrum: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            buffer: vec![0.0; window],
            filled: 0,
            fresh: 0,
            previous: vec![0.0; groups.len()],
            groups,
            drum_band,
            voice,
            pending: VecDeque::new(),
            first: 0,
            next: 0,
            layers: Layers {
                sample_rate: rate,
                window,
                hop,
                ..Layers::default()
            },
        }
    }

    /// Takes the next sample (already cleaned up: finite and within -1..1).
    pub(crate) fn push(&mut self, sample: f32) {
        self.buffer[self.filled] = sample;
        self.filled += 1;
        self.fresh += 1;
        if self.filled == self.window {
            self.frame();
            self.buffer.copy_within(self.hop.., 0);
            self.filled = self.window - self.hop;
        }
    }

    /// The layers, after a last partial frame (padded with silence) and the frames still waiting.
    pub(crate) fn finish(mut self) -> Layers {
        if self.fresh > 0 {
            self.buffer[self.filled..].fill(0.0);
            self.frame();
        }
        while self.next < self.first + self.pending.len() {
            self.separate();
        }
        self.layers
    }

    fn frame(&mut self) {
        self.fresh = 0;
        for ((x, &s), &w) in self.input.iter_mut().zip(&self.buffer).zip(&self.hann) {
            *x = s * w;
        }
        if self
            .fft
            .process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch)
            .is_err()
        {
            return;
        }
        // Scaled so a full-scale sine is about 0 dB in its bin.
        let scale = 4.0 / (self.window as f32 * self.window as f32 / 4.0);
        let mut total = 0.0;
        let mut weighted = 0.0;
        let mut cymbal = 0.0;
        let magnitude: Vec<f32> = self
            .groups
            .iter()
            .zip(&self.drum_band)
            .map(|(&(first, n, hz), &band)| {
                let power: f32 = self.spectrum[first..first + n]
                    .iter()
                    .map(|c| c.norm_sqr() * scale)
                    .sum();
                total += power;
                weighted += power * hz;
                if band == Some(CYMBAL) {
                    cymbal += power;
                }
                (power / n as f32).sqrt()
            })
            .collect();
        let across = median_filter(&magnitude, FREQ_HALF);
        self.pending.push_back(Pending {
            magnitude,
            across,
            total,
            cymbal,
            centroid: if total > FLOOR { weighted / total } else { 0.0 },
        });
        // Keep the frames the next one to separate needs, and separate those that have theirs.
        let newest = self.first + self.pending.len() - 1;
        while self.next + TIME_HALF <= newest {
            self.separate();
        }
        while self.first + TIME_HALF < self.next {
            self.pending.pop_front();
            self.first += 1;
        }
    }

    /// Separates frame `next` with the frames around it that are in.
    fn separate(&mut self) {
        let at = self.next - self.first;
        let lo = at.saturating_sub(TIME_HALF);
        let hi = (at + TIME_HALF + 1).min(self.pending.len());
        let mut along = [0.0f32; 2 * TIME_HALF + 1];
        let frame = &self.pending[at];
        let (mut percussive, mut harmonic, mut voice, mut flux) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut drums = [0.0f32; 4];
        for (g, &(_, n, _)) in self.groups.iter().enumerate() {
            let k = hi - lo;
            for (slot, p) in along.iter_mut().zip(self.pending.range(lo..hi)) {
                *slot = p.magnitude[g];
            }
            let h = median(&mut along[..k]);
            let p = frame.across[g];
            let x = frame.magnitude[g];
            let (h2, p2) = (h * h, p * p);
            let share = if h2 + p2 > 0.0 { p2 / (h2 + p2) } else { 0.5 };
            let (xp, xh) = (x * share, x * (1.0 - share));
            let n = n as f32;
            percussive += xp * xp * n;
            harmonic += xh * xh * n;
            if let Some(band) = self.drum_band[g] {
                drums[band] += xp * xp * n;
            }
            if self.voice[g] {
                voice += xh * xh * n;
            }
            let compressed = (1.0 + 1000.0 * xp).ln();
            flux += (compressed - self.previous[g]).max(0.0);
            self.previous[g] = compressed;
        }
        let db = |power: f32| 10.0 * power.max(FLOOR).log10();
        let l = &mut self.layers;
        l.percussive_db.push(db(percussive));
        l.harmonic_db.push(db(harmonic));
        l.total_db.push(db(frame.total));
        l.drums_db.push(drums.map(db));
        l.percussive_flux.push(if self.next == 0 { 0.0 } else { flux });
        l.voice_db.push(db(voice));
        l.cymbal_db.push(db(frame.cymbal));
        l.centroid_hz.push(frame.centroid);
        self.next += 1;
    }
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
mod tests {
    use super::*;

    fn layers_of(samples: &[f32], rate: u32) -> Layers {
        let mut extractor = LayerExtractor::new(rate);
        samples.iter().for_each(|&s| extractor.push(s));
        extractor.finish()
    }

    #[test]
    fn a_tone_is_harmonic_and_clicks_are_percussive() {
        let rate = 22_050;
        let n = 4 * rate as usize;
        let tone: Vec<f32> = (0..n)
            .map(|i| 0.3 * (std::f32::consts::TAU * 440.0 * i as f32 / rate as f32).sin())
            .collect();
        // A click every half second.
        let clicks: Vec<f32> = (0..n)
            .map(|i| if i % (rate as usize / 2) < 3 { 0.9 } else { 0.0 })
            .collect();
        let both: Vec<f32> = tone.iter().zip(&clicks).map(|(a, b)| a + b).collect();
        let (t, c, m) = (
            layers_of(&tone, rate),
            layers_of(&clicks, rate),
            layers_of(&both, rate),
        );
        assert_eq!(t.len(), m.len());
        assert!((t.frame_seconds() - 0.0116).abs() < 0.001);
        let mean = |v: &[f32]| v[20..v.len() - 20].iter().sum::<f32>() / (v.len() - 40) as f32;
        // The tone is almost all harmonic; the clicks almost all percussive.
        assert!(mean(&t.harmonic_db) > mean(&t.percussive_db) + 15.0);
        let at_click = c.frame_at(1.0);
        let hit = (at_click - 2..=at_click + 2)
            .map(|f| c.percussive_db[f] - c.harmonic_db[f])
            .fold(f32::MIN, f32::max);
        assert!(hit > 10.0, "{hit}");
        // Mixed: the percussive part follows the clicks, the harmonic part the tone.
        let on = (at_click - 2..=at_click + 2)
            .map(|f| m.percussive_db[f])
            .fold(f32::MIN, f32::max);
        let off = m.percussive_db[m.frame_at(1.25)];
        assert!(on > off + 15.0, "{on} {off}");
        let steady = |f: usize| m.harmonic_db[f];
        assert!((steady(m.frame_at(1.0)) - steady(m.frame_at(1.25))).abs() < 4.0);
        let flux_on = m.percussive_flux[at_click - 2..=at_click + 2]
            .iter()
            .copied()
            .fold(0.0, f32::max);
        assert!(flux_on > 5.0 * m.percussive_flux[m.frame_at(1.25)].max(0.01));
        // The voice band hears the 440 Hz tone; the centroid sits near it.
        assert!(mean(&t.voice_db) > -20.0);
        assert!(
            (t.centroid_hz[100] - 440.0).abs() < 60.0,
            "{}",
            t.centroid_hz[100]
        );
    }

    #[test]
    fn silence_and_short_clips() {
        let l = layers_of(&[0.0; 5000], 44_100);
        assert!(l.percussive_db.iter().all(|&d| d <= -99.0));
        assert_eq!(layers_of(&[0.1; 100], 44_100).len(), 1);
        assert!(layers_of(&[], 44_100).is_empty());
    }
}
