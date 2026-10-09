//! A cheap guess at when the voice is sounding: not source separation, just three things a lead
//! vocal usually is, measured every ~23 ms.
//!
//! - **In the middle of the mix**: its energy in the voice band (200–4000 Hz) is in the mid
//!   signal (left + right), not the side (left − right). A mono file is all middle.
//! - **In the voice band**: a good share of the mid signal's energy is there.
//! - **Pitched**: the band's spectrum is peaky (low spectral flatness), unlike drums and noise.
//!
//! Their product, smoothed over ~250 ms and scaled by the song's own loud vocal moments, is the
//! [`VocalActivity::level`] (0–1). Its sharp rises are [`VocalActivity::onsets`]: where sung
//! words are likely to start. Pitched centre instruments (a lead synth, a sax solo) look like a
//! voice too; the lyrics' own timing comes first and this only fine-tunes it.

use crate::AnalysisError;
use pf_audio::StereoFrames;
use realfft::RealFftPlanner;
use std::path::Path;

const LOW_HZ: f32 = 200.0;
const HIGH_HZ: f32 = 4_000.0;
/// Power treated as silence, per bin.
const FLOOR: f32 = 1e-10;
/// Voice-band power treated as silence, whatever the rest of the song is like (far below -90 dB).
const SILENT_POWER: f32 = 1e-6;
/// Frames averaged for the level (about 250 ms).
const SMOOTH_FRAMES: usize = 11;
/// The level from which the voice is taken to be sounding.
pub const VOCAL_THRESHOLD: f32 = 0.35;
/// Samples between checks for a stop.
const CHECK_EVERY: usize = 1 << 16;

/// How likely the voice is sounding over a song.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VocalActivity {
    /// Milliseconds between values.
    pub hop_ms: f64,
    /// 0–1 per frame, the first centred at `hop_ms / 2`.
    pub level: Vec<f32>,
    /// Where the voice likely starts a note (ms), in order.
    pub onsets: Vec<u64>,
}

impl VocalActivity {
    /// The level at `ms` (0 past the end).
    pub fn at(&self, ms: u64) -> f32 {
        if self.hop_ms <= 0.0 {
            return 0.0;
        }
        let frame = (ms as f64 / self.hop_ms) as usize;
        self.level.get(frame).copied().unwrap_or(0.0)
    }

    pub fn is_vocal(&self, ms: u64) -> bool {
        self.at(ms) >= VOCAL_THRESHOLD
    }
}

/// Measures (left, right) pairs at `rate` Hz, checking `stop` every so often.
pub fn vocal_activity(
    frames: impl IntoIterator<Item = (f32, f32)>,
    rate: u32,
    stop: &dyn Fn() -> bool,
) -> Result<VocalActivity, AnalysisError> {
    let mut extractor = ActivityExtractor::new(rate);
    for (i, (l, r)) in frames.into_iter().enumerate() {
        if i % CHECK_EVERY == 0 && stop() {
            return Err(AnalysisError::Cancelled);
        }
        extractor.push(l, r);
    }
    Ok(extractor.finish())
}

/// Takes (left, right) pairs one at a time (see [`vocal_activity`]), so other measures can be
/// taken from the same pass over the song.
pub(crate) struct ActivityExtractor {
    hop: usize,
    window: usize,
    rate: u32,
    fft: std::sync::Arc<dyn realfft::RealToComplex<f32>>,
    first: usize,
    last: usize,
    hann: Vec<f32>,
    mid_in: Vec<f32>,
    side_in: Vec<f32>,
    mid_out: Vec<realfft::num_complex::Complex<f32>>,
    side_out: Vec<realfft::num_complex::Complex<f32>>,
    scratch: Vec<realfft::num_complex::Complex<f32>>,
    mid: Vec<f32>,
    side: Vec<f32>,
    filled: usize,
    raw: Vec<f32>,
    band_power: Vec<f32>,
}

impl ActivityExtractor {
    pub(crate) fn new(rate: u32) -> Self {
        let rate = rate.max(1);
        // About 23 ms between frames, a power of two; windows twice as long.
        let hop = (0.0232 * f64::from(rate)).log2().round().clamp(5.0, 13.0).exp2() as usize;
        let window = 2 * hop;
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(window);
        let bin_hz = rate as f32 / window as f32;
        let first = ((LOW_HZ / bin_hz).ceil() as usize).max(1);
        let last = ((HIGH_HZ / bin_hz).floor() as usize).min(window / 2);
        Self {
            hop,
            window,
            rate,
            first,
            last,
            hann: (0..window)
                .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / window as f32).cos())
                .collect(),
            mid_in: fft.make_input_vec(),
            side_in: fft.make_input_vec(),
            mid_out: fft.make_output_vec(),
            side_out: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            mid: vec![0.0; window],
            side: vec![0.0; window],
            filled: 0,
            raw: Vec::new(),
            band_power: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, l: f32, r: f32) {
        let clean = |s: f32| if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
        let (l, r) = (clean(l), clean(r));
        self.mid[self.filled] = (l + r) / 2.0;
        self.side[self.filled] = (l - r) / 2.0;
        self.filled += 1;
        if self.filled < self.window {
            return;
        }
        for k in 0..self.window {
            self.mid_in[k] = self.mid[k] * self.hann[k];
            self.side_in[k] = self.side[k] * self.hann[k];
        }
        // A plan's own buffers are always the right length.
        let _ = self
            .fft
            .process_with_scratch(&mut self.mid_in, &mut self.mid_out, &mut self.scratch);
        let _ = self
            .fft
            .process_with_scratch(&mut self.side_in, &mut self.side_out, &mut self.scratch);
        let power = |c: &realfft::num_complex::Complex<f32>| c.norm_sqr();
        let (first, last) = (self.first, self.last);
        let mid_total: f32 = self.mid_out.iter().map(power).sum::<f32>() + FLOOR;
        let band: Vec<f32> = self.mid_out[first..=last]
            .iter()
            .map(|c| power(c) + FLOOR)
            .collect();
        let mid_band: f32 = band.iter().sum();
        let side_band: f32 = self.side_out[first..=last].iter().map(power).sum::<f32>() + FLOOR;
        let centre = mid_band / (mid_band + side_band);
        let share = (mid_band / mid_total).min(1.0);
        let mean = mid_band / band.len() as f32;
        let geometric = (band.iter().map(|p| p.ln()).sum::<f32>() / band.len() as f32).exp();
        let pitched = 1.0 - (geometric / mean).clamp(0.0, 1.0);
        // Twice as much in the middle as the sides counts fully (0.5 is an even spread).
        let centred = ((centre - 0.5) * 2.0).clamp(0.0, 1.0);
        self.raw.push(centred * share.sqrt() * pitched);
        self.band_power.push(mid_band);
        self.mid.copy_within(self.hop.., 0);
        self.side.copy_within(self.hop.., 0);
        self.filled = self.window - self.hop;
    }

    pub(crate) fn finish(self) -> VocalActivity {
        let hop_ms = self.hop as f64 * 1000.0 / f64::from(self.rate);
        finish(self.raw, self.band_power, hop_ms)
    }
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

/// Quiet frames count as silent, then the level is smoothed and scaled to the song's loud vocal
/// moments, and its sharp rises picked out.
fn finish(mut raw: Vec<f32>, band_power: Vec<f32>, hop_ms: f64) -> VocalActivity {
    // Anything 40 dB under the song's loud parts is silence, whatever its shape.
    let loud = percentile(&band_power, 0.95);
    for (value, power) in raw.iter_mut().zip(&band_power) {
        if *power < loud * 1e-4 || *power < SILENT_POWER {
            *value = 0.0;
        }
    }
    let half = SMOOTH_FRAMES / 2;
    let smooth: Vec<f32> = (0..raw.len())
        .map(|i| {
            let span = &raw[i.saturating_sub(half)..(i + half + 1).min(raw.len())];
            span.iter().sum::<f32>() / span.len() as f32
        })
        .collect();
    // Scaled between the song's typical background (its lower quarter) and its loud vocal moments.
    let base = percentile(&smooth, 0.25);
    let top = percentile(&smooth, 0.95).max(base + 1e-6);
    let level: Vec<f32> = smooth
        .iter()
        .map(|v| ((v - base) / (top - base)).clamp(0.0, 1.0))
        .collect();
    // A rise of the unsmoothed value well above its neighbourhood, at most one per 150 ms.
    let gap = (150.0 / hop_ms).ceil().max(1.0) as usize;
    let mut onsets = Vec::new();
    let mut last: Option<usize> = None;
    for i in 1..raw.len() {
        let rise = raw[i] - raw[i - 1];
        let near = &raw[i.saturating_sub(gap)..(i + gap).min(raw.len())];
        let mean = near.iter().sum::<f32>() / near.len() as f32;
        let peak = rise > 0.0 && raw[i] > mean * 1.5 && raw[i] >= top * 0.5;
        if peak && last.is_none_or(|l| i - l >= gap) && level[i] >= VOCAL_THRESHOLD * 0.5 {
            onsets.push((i as f64 * hop_ms) as u64);
            last = Some(i);
        }
    }
    VocalActivity {
        hop_ms,
        level,
        onsets,
    }
}

/// Decodes a music file and measures it (see [`vocal_activity`]).
pub fn vocal_activity_file(path: &Path, stop: &dyn Fn() -> bool) -> Result<VocalActivity, AnalysisError> {
    let frames = StereoFrames::open(path)?;
    let rate = frames.sample_rate();
    vocal_activity(frames, rate, stop)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 16_000;

    /// A sung note: a 220 Hz tone with harmonics.
    fn voice(i: usize) -> f32 {
        let t = i as f32 / RATE as f32;
        (1..=8)
            .map(|h| (std::f32::consts::TAU * 220.0 * h as f32 * t).sin() / h as f32)
            .sum::<f32>()
            * 0.2
    }

    /// Cheap noise, different on each side.
    fn noise(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
    }

    #[test]
    fn a_centred_pitched_voice_stands_out_from_wide_noise() {
        // 0–2 s: noise spread wide; 2–4 s: a voice in the middle over quieter noise; 4–6 s:
        // noise again.
        let mut seed = 7;
        let frames: Vec<(f32, f32)> = (0..6 * RATE as usize)
            .map(|i| {
                let (l, r) = (noise(&mut seed) * 0.3, noise(&mut seed) * 0.3);
                if (2 * RATE as usize..4 * RATE as usize).contains(&i) {
                    (l * 0.2 + voice(i), r * 0.2 + voice(i))
                } else {
                    (l, r)
                }
            })
            .collect();
        let activity = vocal_activity(frames, RATE, &|| false).unwrap();
        assert!((activity.hop_ms - 32.0).abs() < 1.0, "{}", activity.hop_ms);
        for ms in [2_500, 3_000, 3_500] {
            assert!(activity.is_vocal(ms), "{ms}: {}", activity.at(ms));
        }
        for ms in [500, 1_000, 1_500, 4_800, 5_500] {
            assert!(!activity.is_vocal(ms), "{ms}: {}", activity.at(ms));
        }
        // The voice coming in is an onset.
        assert!(
            activity.onsets.iter().any(|&t| (1_850..2_250).contains(&t)),
            "{:?}",
            activity.onsets
        );
    }

    #[test]
    fn silence_is_never_vocal_and_stop_is_heard() {
        let activity = vocal_activity(vec![(0.0, 0.0); RATE as usize], RATE, &|| false).unwrap();
        assert!(activity.level.iter().all(|&v| v == 0.0));
        assert!(activity.onsets.is_empty());
        assert!(matches!(
            vocal_activity(vec![(0.0, 0.0); RATE as usize], RATE, &|| true),
            Err(AnalysisError::Cancelled)
        ));
    }
}
