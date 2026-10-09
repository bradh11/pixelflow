//! What a song sounds like over time, for finding its structure: every ~46 ms, timbre
//! coefficients (MFCC-like, from a 40-band log-mel spectrum), chroma (how much of each of the 12
//! pitch classes is sounding), loudness, spectral flux (of the mel spectrum), and the energy in
//! the bass, mid, and treble bands.
//!
//! The features are computed while the onset envelope reads the song, from the same samples, so
//! the song is still only decoded once and never held in memory.

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;

/// Mel bands in the spectrum.
pub const MEL_BANDS: usize = 40;
/// Timbre coefficients: the mel spectrum's cosine transform, leaving out the first (loudness).
pub const TIMBRE: usize = 13;
/// Lowest and highest mel band edges, in Hz.
const MEL_LOW_HZ: f32 = 30.0;
const MEL_HIGH_HZ: f32 = 11_000.0;
/// Chroma is taken from this range: below it the spectrum can't tell neighboring semitones apart.
const CHROMA_LOW_HZ: f32 = 180.0;
const CHROMA_HIGH_HZ: f32 = 5_000.0;
/// Where the bass band ends and the treble band starts, in Hz.
const BASS_HZ: f32 = 200.0;
const TREBLE_HZ: f32 = 2_000.0;
/// Power treated as silence (about -100 dB).
const FLOOR: f32 = 1e-10;

/// The features of one song, a frame every [`Features::frame_seconds`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Features {
    pub sample_rate: u32,
    /// Samples per frame, and between frames.
    pub window: usize,
    pub hop: usize,
    /// Timbre: coefficients 1–13 of the mel spectrum's cosine transform.
    pub timbre: Vec<[f32; TIMBRE]>,
    /// How strongly each pitch class (C, C♯, … B) sounds, scaled so the strongest is 1 (all 0 in
    /// silence).
    pub chroma: Vec<[f32; 12]>,
    /// Loudness (dB of the mean power; about -100 for silence).
    pub loudness_db: Vec<f32>,
    /// How much the mel spectrum rose since the frame before (dB per band, falls counting 0).
    pub flux: Vec<f32>,
    /// Power (dB) in the bass, mid, and treble bands.
    pub bands_db: Vec<[f32; 3]>,
}

impl Features {
    /// Seconds between frames.
    pub fn frame_seconds(&self) -> f64 {
        self.hop as f64 / f64::from(self.sample_rate.max(1))
    }

    /// The frame whose middle is nearest `seconds`.
    pub fn frame_at(&self, seconds: f64) -> usize {
        let half = self.window as f64 / 2.0 / f64::from(self.sample_rate.max(1));
        let frame = ((seconds - half) / self.frame_seconds()).round().max(0.0) as usize;
        frame.min(self.len().saturating_sub(1))
    }

    pub fn len(&self) -> usize {
        self.loudness_db.len()
    }

    pub fn is_empty(&self) -> bool {
        self.loudness_db.is_empty()
    }
}

/// Reads samples one at a time and works out the features a frame at a time.
pub(crate) struct FeatureExtractor {
    window: usize,
    hop: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    hann: Vec<f32>,
    input: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    power: Vec<f32>,
    buffer: Vec<f32>,
    filled: usize,
    /// Samples since the last frame was taken.
    fresh: usize,
    /// Per mel band: its first bin and the weight of each bin from there.
    mel_filters: Vec<(usize, Vec<f32>)>,
    /// Per bin: its pitch class, if it's in the chroma range.
    pitch_class: Vec<Option<usize>>,
    /// Per bin: its band (0 bass, 1 mid, 2 treble).
    band: Vec<usize>,
    /// The cosine transform (timbre coefficient × mel band).
    dct: Vec<[f32; MEL_BANDS]>,
    /// The last frame's log-mel spectrum (dB per band), for the flux.
    previous_mel: Option<[f32; MEL_BANDS]>,
    features: Features,
}

impl FeatureExtractor {
    pub(crate) fn new(sample_rate: u32) -> Self {
        let rate = sample_rate.max(1);
        // About 46 ms between frames, a power of two (2048 at 44.1 kHz); windows twice as long.
        let hop = (0.0464 * f64::from(rate)).log2().round().clamp(6.0, 14.0).exp2() as usize;
        let window = 2 * hop;
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(window);
        let bins = window / 2 + 1;
        let hz = |bin: usize| bin as f32 * rate as f32 / window as f32;
        let nyquist = rate as f32 / 2.0;

        let mel = |f: f32| 2595.0 * (1.0 + f / 700.0).log10();
        let unmel = |m: f32| 700.0 * (10f32.powf(m / 2595.0) - 1.0);
        let (lo, hi) = (mel(MEL_LOW_HZ), mel(MEL_HIGH_HZ.min(nyquist * 0.95)));
        let edges: Vec<f32> = (0..MEL_BANDS + 2)
            .map(|i| unmel(lo + (hi - lo) * i as f32 / (MEL_BANDS + 1) as f32))
            .collect();
        let mel_filters = (0..MEL_BANDS)
            .map(|b| {
                let (left, center, right) = (edges[b], edges[b + 1], edges[b + 2]);
                let weight = |f: f32| {
                    if f <= left || f >= right {
                        0.0
                    } else if f <= center {
                        (f - left) / (center - left)
                    } else {
                        (right - f) / (right - center)
                    }
                };
                let first = (left * window as f32 / rate as f32).floor() as usize;
                let last = ((right * window as f32 / rate as f32).ceil() as usize).min(bins - 1);
                let mut weights: Vec<f32> = (first..=last).map(|k| weight(hz(k))).collect();
                // A band narrower than a bin still takes the bin nearest its middle.
                if weights.iter().all(|&w| w == 0.0) {
                    let nearest = (center * window as f32 / rate as f32).round() as usize;
                    weights = (first..=last)
                        .map(|k| if k == nearest { 1.0 } else { 0.0 })
                        .collect();
                }
                (first, weights)
            })
            .collect();
        let pitch_class = (0..bins)
            .map(|k| {
                let f = hz(k);
                (CHROMA_LOW_HZ..CHROMA_HIGH_HZ).contains(&f).then(|| {
                    let midi = 69.0 + 12.0 * (f / 440.0).log2();
                    (midi.round() as i64).rem_euclid(12) as usize
                })
            })
            .collect();
        let band = (0..bins)
            .map(|k| match hz(k) {
                f if f < BASS_HZ => 0,
                f if f < TREBLE_HZ => 1,
                _ => 2,
            })
            .collect();
        let dct = (1..=TIMBRE)
            .map(|c| {
                let mut row = [0.0f32; MEL_BANDS];
                for (b, w) in row.iter_mut().enumerate() {
                    *w = (std::f32::consts::PI * c as f32 * (b as f32 + 0.5) / MEL_BANDS as f32).cos()
                        * (2.0 / MEL_BANDS as f32).sqrt();
                }
                row
            })
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
            power: vec![0.0; bins],
            fft,
            buffer: vec![0.0; window],
            filled: 0,
            fresh: 0,
            mel_filters,
            pitch_class,
            band,
            dct,
            previous_mel: None,
            features: Features {
                sample_rate: rate,
                window,
                hop,
                ..Features::default()
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

    /// The features, after a last partial frame (padded with silence) if it holds new samples.
    pub(crate) fn finish(mut self) -> Features {
        if self.fresh > 0 {
            self.buffer[self.filled..].fill(0.0);
            self.frame();
        }
        self.features
    }

    fn frame(&mut self) {
        self.fresh = 0;
        let mean_square = self.buffer.iter().map(|&s| s * s).sum::<f32>() / self.window as f32;
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
        for (p, c) in self.power.iter_mut().zip(&self.spectrum) {
            *p = c.norm_sqr() * scale;
        }
        let db = |power: f32| 10.0 * power.max(FLOOR).log10();

        let mut mel = [0.0f32; MEL_BANDS];
        for (m, (first, weights)) in mel.iter_mut().zip(&self.mel_filters) {
            let sum: f32 = weights
                .iter()
                .zip(&self.power[*first..])
                .map(|(w, p)| w * p)
                .sum();
            *m = db(sum);
        }
        let mut timbre = [0.0f32; TIMBRE];
        for (t, row) in timbre.iter_mut().zip(&self.dct) {
            *t = row.iter().zip(&mel).map(|(w, m)| w * m).sum();
        }
        let mut chroma = [0.0f32; 12];
        let mut bands = [0.0f32; 3];
        for ((p, pc), &band) in self.power.iter().zip(&self.pitch_class).zip(&self.band) {
            if let Some(pc) = pc {
                chroma[*pc] += p.sqrt();
            }
            bands[band] += p;
        }
        let strongest = chroma.iter().copied().fold(0.0, f32::max);
        if strongest > 1e-4 {
            chroma.iter_mut().for_each(|c| *c /= strongest);
        } else {
            chroma = [0.0; 12];
        }
        let flux = match &self.previous_mel {
            Some(previous) => {
                mel.iter()
                    .zip(previous)
                    .map(|(m, p)| (m - p).max(0.0))
                    .sum::<f32>()
                    / MEL_BANDS as f32
            }
            None => 0.0,
        };
        self.previous_mel = Some(mel);
        let f = &mut self.features;
        f.timbre.push(timbre);
        f.chroma.push(chroma);
        f.loudness_db.push(db(mean_square));
        f.flux.push(flux);
        f.bands_db.push(bands.map(db));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn features_of(samples: impl Iterator<Item = f32>, rate: u32) -> Features {
        let mut extractor = FeatureExtractor::new(rate);
        samples.for_each(|s| extractor.push(s));
        extractor.finish()
    }

    fn tone(hz: f32, seconds: f32, rate: u32) -> impl Iterator<Item = f32> {
        (0..(seconds * rate as f32) as usize)
            .map(move |i| 0.5 * (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin())
    }

    #[test]
    fn frames_are_about_46_ms_apart() {
        for rate in [22_050, 44_100, 48_000] {
            let f = features_of(tone(440.0, 2.0, rate), rate);
            assert!(
                (f.frame_seconds() - 0.046).abs() < 0.005,
                "{rate}: {}",
                f.frame_seconds()
            );
            assert!(
                (f.len() as f64 * f.frame_seconds() - 2.0).abs() < 0.1,
                "{rate}: {}",
                f.len()
            );
            let first = f.window as f64 / 2.0 / f64::from(rate);
            assert_eq!(
                f.frame_at(1.0),
                ((1.0 - first) / f.frame_seconds()).round() as usize
            );
            assert_eq!(f.frame_at(1e9), f.len() - 1);
        }
    }

    #[test]
    fn chroma_names_the_note_and_bands_split_the_spectrum() {
        // A4 (440 Hz) is pitch class 9; E5 (659 Hz) is 4.
        let a = features_of(tone(440.0, 1.0, 44_100), 44_100);
        let e = features_of(tone(659.25, 1.0, 44_100), 44_100);
        let strongest = |c: &[f32; 12]| (0..12).max_by(|&x, &y| c[x].total_cmp(&c[y])).unwrap();
        assert_eq!(strongest(&a.chroma[5]), 9);
        assert_eq!(strongest(&e.chroma[5]), 4);
        assert!(a.timbre[5] != e.timbre[5]);
        let bass = features_of(tone(80.0, 1.0, 44_100), 44_100);
        let [low, mid, high] = bass.bands_db[5];
        assert!(low > mid + 20.0 && low > high + 20.0, "{:?}", bass.bands_db[5]);
        assert!((a.loudness_db[5] - 20.0 * (0.5f32 / 2f32.sqrt()).log10()).abs() < 0.5);
    }

    #[test]
    fn silence_and_short_clips() {
        let f = features_of(std::iter::repeat_n(0.0, 10_000), 44_100);
        assert!(f.chroma.iter().all(|c| c.iter().all(|&x| x == 0.0)));
        assert!(f.loudness_db.iter().all(|&l| l <= -99.0));
        assert_eq!(features_of(std::iter::repeat_n(0.1, 100), 44_100).len(), 1);
        assert!(features_of(std::iter::empty(), 44_100).is_empty());
    }
}
