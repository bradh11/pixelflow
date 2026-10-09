//! The song as the aligner hears it: 16 kHz, the lead vocal brought forward. Not a separation
//! model, but what a lead vocal usually is (as `pf_analysis`'s vocal track finds it): **in the
//! middle of the mix**. Each frequency of the mid signal ((L + R) / 2) is kept by how alike the
//! two sides are there, and turned down where they differ (wide guitars, stereo keys, reverb),
//! then made back into sound. The lowest frequencies (kick, bass) go too; a mono song is all
//! middle, and is only filtered.

use crate::AlignError;
use pf_audio::{Progress, Resampler, StereoFrames, reported};
use realfft::RealFftPlanner;
use realfft::num_complex::Complex;
use std::path::Path;

/// The sample rate the model hears (Hz).
pub const RATE: u32 = 16_000;
/// Samples per analysis window (32 ms) and between windows (8 ms).
const WINDOW: usize = 512;
const HOP: usize = 128;
/// Under this (Hz), nothing is kept: kick and bass, not voice.
const LOW_CUT_HZ: f32 = 90.0;
/// How strongly a difference between the sides turns a frequency down (as the vocal track's).
const SIDE_WEIGHT: f32 = 1.5;
/// Samples between checks for a stop.
const CHECK_EVERY: usize = 1 << 15;

/// The song at `path` with its voice brought forward, mono at [`RATE`].
pub fn centre_voice(
    path: &Path,
    stop: &dyn Fn() -> bool,
    progress: &dyn Fn(f32),
) -> Result<Vec<f32>, AlignError> {
    let frames = StereoFrames::open(path).map_err(|e| AlignError::Audio(e.to_string()))?;
    let read = frames.position();
    let rate = frames.sample_rate();
    let progress = Progress::new(progress);
    let mut left = Resampler::new(rate, RATE);
    let mut right = Resampler::new(rate, RATE);
    let (mut l, mut r) = (Vec::new(), Vec::new());
    for (i, (a, b)) in reported(frames, read, &progress, 0.7).enumerate() {
        if i % CHECK_EVERY == 0 && stop() {
            return Err(AlignError::Cancelled);
        }
        left.push(a, &mut l);
        right.push(b, &mut r);
    }
    let n = l.len().min(r.len());
    l.truncate(n);
    r.truncate(n);
    let centre = centre_of(&l, &r, stop)?;
    progress.finish();
    Ok(centre)
}

/// `left` and `right` (at [`RATE`]) made into one, keeping what's in the middle (see the module
/// notes).
pub fn centre_of(left: &[f32], right: &[f32], stop: &dyn Fn() -> bool) -> Result<Vec<f32>, AlignError> {
    let n = left.len().min(right.len());
    let mut planner = RealFftPlanner::<f32>::new();
    let forward = planner.plan_fft_forward(WINDOW);
    let inverse = planner.plan_fft_inverse(WINDOW);
    // A square-root Hann window both ways: overlapped at a quarter, they add up evenly.
    let window: Vec<f32> = (0..WINDOW)
        .map(|i| (std::f32::consts::PI * i as f32 / WINDOW as f32).sin())
        .collect();
    let low_bin = (LOW_CUT_HZ * WINDOW as f32 / RATE as f32).ceil() as usize;
    let mut out = vec![0.0f32; n + WINDOW];
    let mut weight = vec![0.0f32; n + WINDOW];
    let (mut lb, mut rb) = (forward.make_input_vec(), forward.make_input_vec());
    let (mut ls, mut rs) = (forward.make_output_vec(), forward.make_output_vec());
    let mut scratch = forward.make_scratch_vec();
    let mut back_scratch = inverse.make_scratch_vec();
    let mut mid = inverse.make_output_vec();
    let mut spectrum = inverse.make_input_vec();
    // Windows start half a window before the first sample, so every sample is covered evenly.
    let mut start: isize = -(WINDOW as isize / 2);
    let mut count = 0usize;
    while start < n as isize {
        if count.is_multiple_of(CHECK_EVERY / HOP) && stop() {
            return Err(AlignError::Cancelled);
        }
        count += 1;
        for i in 0..WINDOW {
            let at = start + i as isize;
            let (a, b) = if at >= 0 && (at as usize) < n {
                (left[at as usize], right[at as usize])
            } else {
                (0.0, 0.0)
            };
            lb[i] = a * window[i];
            rb[i] = b * window[i];
        }
        forward
            .process_with_scratch(&mut lb, &mut ls, &mut scratch)
            .map_err(|e| AlignError::Audio(e.to_string()))?;
        forward
            .process_with_scratch(&mut rb, &mut rs, &mut scratch)
            .map_err(|e| AlignError::Audio(e.to_string()))?;
        for (k, s) in spectrum.iter_mut().enumerate() {
            let m = (ls[k] + rs[k]) * 0.5;
            let side = ((ls[k] - rs[k]) * 0.5).norm();
            let centred = (1.0 - SIDE_WEIGHT * side / (m.norm() + 1e-9)).clamp(0.0, 1.0);
            *s = if k < low_bin {
                Complex::new(0.0, 0.0)
            } else {
                m * (centred * centred)
            };
        }
        // The real transform wants the ends purely real.
        spectrum[0].im = 0.0;
        if let Some(last) = spectrum.last_mut() {
            last.im = 0.0;
        }
        inverse
            .process_with_scratch(&mut spectrum, &mut mid, &mut back_scratch)
            .map_err(|e| AlignError::Audio(e.to_string()))?;
        for i in 0..WINDOW {
            let at = start + i as isize + WINDOW as isize / 2;
            if at >= 0 && (at as usize) < out.len() {
                out[at as usize] += mid[i] * window[i] / WINDOW as f32;
                weight[at as usize] += window[i] * window[i];
            }
        }
        start += HOP as isize;
    }
    let half = WINDOW / 2;
    Ok((0..n)
        .map(|i| {
            let w = weight[i + half];
            if w > 1e-6 { out[i + half] / w } else { 0.0 }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, n: usize, gain: f32) -> Vec<f32> {
        (0..n)
            .map(|i| gain * (2.0 * std::f32::consts::PI * hz * i as f32 / RATE as f32).sin())
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn the_middle_is_kept_and_the_sides_turned_down() {
        let n = RATE as usize;
        // A centred 440 Hz "voice" comes back as it was.
        let voice = tone(440.0, n, 0.5);
        let out = centre_of(&voice, &voice, &|| false).unwrap();
        assert_eq!(out.len(), n);
        let inner = 2_000..n - 2_000;
        let error: Vec<f32> = inner.clone().map(|i| out[i] - voice[i]).collect();
        assert!(rms(&error) < 0.02 * rms(&voice[inner.clone()]), "{}", rms(&error));
        // A 1 kHz tone on the left only is turned well down.
        let guitar = tone(1_000.0, n, 0.5);
        let silence = vec![0.0; n];
        let out = centre_of(&guitar, &silence, &|| false).unwrap();
        assert!(rms(&out[inner.clone()]) < 0.05 * rms(&guitar), "{}", rms(&out));
        // A centred 50 Hz bass too.
        let bass = tone(50.0, n, 0.5);
        let out = centre_of(&bass, &bass, &|| false).unwrap();
        assert!(rms(&out[inner]) < 0.1 * rms(&bass));
        // Stop stops.
        assert!(matches!(
            centre_of(&voice, &voice, &|| true),
            Err(AlignError::Cancelled)
        ));
    }
}
