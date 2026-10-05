//! Decoding a music file to mono samples, a few at a time, for analysis.

use crate::decode::open_decoder;
use crate::error::AudioError;
use rodio::{Decoder, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// A music file decoded on the fly to mono samples (channels averaged) at its own sample rate.
/// Nothing is held in memory beyond the decoder's buffer, so long songs are fine.
pub struct MonoSamples {
    decoder: Decoder<BufReader<File>>,
    channels: usize,
    rate: u32,
}

impl MonoSamples {
    /// Opens `path` for decoding.
    pub fn open(path: &Path) -> Result<Self, AudioError> {
        let decoder = open_decoder(path)?;
        Ok(Self {
            channels: usize::from(decoder.channels().get()),
            rate: decoder.sample_rate().get(),
            decoder,
        })
    }

    /// Samples per second.
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }
}

impl Iterator for MonoSamples {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let first = self.decoder.next()?;
        let mut sum = first;
        for _ in 1..self.channels {
            sum += self.decoder.next().unwrap_or(0.0);
        }
        Some(sum / self.channels as f32)
    }
}

impl std::fmt::Debug for MonoSamples {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MonoSamples")
            .field("channels", &self.channels)
            .field("rate", &self.rate)
            .finish_non_exhaustive()
    }
}
