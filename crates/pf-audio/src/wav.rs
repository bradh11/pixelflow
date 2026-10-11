//! A song made small for speech recognition: decoded to mono, brought down to a low sample rate,
//! and written as 16-bit WAV.

use crate::error::AudioError;
use crate::mono::MonoSamples;
use crate::progress::{Progress, no_progress, reported};
use std::path::Path;

/// Samples between checks for a stop.
const CHECK_EVERY: usize = 1 << 16;

/// The most of a music file that's ever decoded into memory or analysed: 4 hours, the longest a
/// sequence can be. What comes after is left out.
pub const MAX_SONG_MS: u64 = 4 * 60 * 60 * 1000;

/// How many samples (per channel) of sound at `rate` Hz [`MAX_SONG_MS`] is.
pub fn song_samples(rate: u32) -> usize {
    usize::try_from(MAX_SONG_MS.saturating_mul(u64::from(rate)) / 1000).unwrap_or(usize::MAX)
}

/// Brings mono samples at `from` Hz down (or up) to `to` Hz: each output sample is the mean of
/// the input samples it covers (a box filter, enough to keep speech clear of aliasing), or a
/// straight line between neighbours when going up.
#[derive(Debug, Clone)]
pub struct Resampler {
    /// Input samples per output sample.
    step: f64,
    /// Where the next output sample ends, in input samples.
    next_end: f64,
    position: f64,
    sum: f64,
    count: u32,
    last: f32,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        let step = f64::from(from.max(1)) / f64::from(to.max(1));
        Self {
            step,
            next_end: step,
            position: 0.0,
            sum: 0.0,
            count: 0,
            last: 0.0,
        }
    }

    /// Takes one input sample, handing out any output samples it completes.
    pub fn push(&mut self, sample: f32, out: &mut Vec<f32>) {
        self.position += 1.0;
        self.sum += f64::from(sample);
        self.count += 1;
        if self.step < 1.0 {
            // Going up: a line from the last sample to this one.
            while self.next_end <= self.position {
                let t = (self.next_end - (self.position - 1.0)) as f32;
                out.push(self.last + (sample - self.last) * t);
                self.next_end += self.step;
            }
            self.last = sample;
            self.sum = 0.0;
            self.count = 0;
            return;
        }
        while self.next_end <= self.position {
            out.push((self.sum / f64::from(self.count.max(1))) as f32);
            self.sum = 0.0;
            self.count = 0;
            self.next_end += self.step;
        }
    }
}

/// Decodes a music file to mono at `rate` Hz, giving up when `stop` says so.
pub fn mono_at_rate(path: &Path, rate: u32, stop: &dyn Fn() -> bool) -> Result<Vec<f32>, AudioError> {
    mono_at_rate_reporting(path, rate, stop, &no_progress)
}

/// Like [`mono_at_rate`], telling `progress` how far it has got (0–1, ending at 1 when done).
/// At most the first [`MAX_SONG_MS`] of the file is decoded.
pub fn mono_at_rate_reporting(
    path: &Path,
    rate: u32,
    stop: &dyn Fn() -> bool,
    progress: &dyn Fn(f32),
) -> Result<Vec<f32>, AudioError> {
    let samples = MonoSamples::open(path)?;
    let most = song_samples(samples.sample_rate());
    mono_of(samples, most, rate, stop, progress)
}

/// The first `most` of `samples`, at `rate` Hz.
fn mono_of(
    samples: MonoSamples,
    most: usize,
    rate: u32,
    stop: &dyn Fn() -> bool,
    progress: &dyn Fn(f32),
) -> Result<Vec<f32>, AudioError> {
    let progress = Progress::new(progress);
    let mut resampler = Resampler::new(samples.sample_rate(), rate);
    let read = samples.position();
    let mut out = Vec::new();
    for (i, sample) in reported(samples, read, &progress, 1.0).take(most).enumerate() {
        if i % CHECK_EVERY == 0 && stop() {
            return Err(AudioError::Stopped);
        }
        resampler.push(sample, &mut out);
    }
    progress.finish();
    Ok(out)
}

/// Mono samples (-1 to 1) as a 16-bit PCM WAV file.
pub fn wav_bytes(samples: &[f32], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    // PCM, one channel.
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
        let mut r = Resampler::new(from, to);
        let mut out = Vec::new();
        for &s in input {
            r.push(s, &mut out);
        }
        out
    }

    #[test]
    fn resampling_keeps_the_length_in_time() {
        let second = vec![0.5; 44_100];
        let out = resample(&second, 44_100, 16_000);
        assert!((15_999..=16_000).contains(&out.len()), "{}", out.len());
        assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-6));
        // Going up draws lines between the samples.
        let out = resample(&[0.0, 1.0, 0.0], 8_000, 16_000);
        assert_eq!(out.len(), 6);
        assert_eq!(out, [0.0, 0.0, 0.5, 1.0, 0.5, 0.0]);
    }

    #[test]
    fn a_wav_file_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        let tone: Vec<f32> = (0..16_000).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let bytes = wav_bytes(&tone, 16_000);
        assert_eq!(bytes.len(), 44 + 32_000);
        std::fs::write(&path, bytes).unwrap();
        let back = mono_at_rate(&path, 16_000, &|| false).unwrap();
        assert_eq!(back.len(), 16_000);
        assert!(back.iter().zip(&tone).all(|(a, b)| (a - b).abs() < 1e-3));
        assert!(matches!(
            mono_at_rate(&path, 16_000, &|| true),
            Err(AudioError::Stopped)
        ));
    }

    #[test]
    fn no_more_of_a_file_than_a_song_can_be_long_is_decoded() {
        // Four hours, whatever the rate.
        assert_eq!(song_samples(8_000), 8_000 * 4 * 60 * 60);
        assert_eq!(song_samples(48_000), 48_000 * 4 * 60 * 60);
        // What's past the most that's kept is left out: here, the second half of a second.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("second.wav");
        std::fs::write(&path, wav_bytes(&vec![0.25; 16_000], 16_000)).unwrap();
        let open = || MonoSamples::open(&path).unwrap();
        let half = mono_of(open(), 8_000, 16_000, &|| false, &no_progress).unwrap();
        assert_eq!(half.len(), 8_000);
        let all = mono_of(open(), song_samples(16_000), 16_000, &|| false, &no_progress).unwrap();
        assert_eq!(all.len(), 16_000);
    }
}
