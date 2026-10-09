//! Decoding a music file to mono samples, a few at a time, for analysis.

use crate::decode::{FileDecoder, open_decoder};
use crate::error::AudioError;
use crate::progress::ReadPosition;
use rodio::Source;
use std::path::Path;

/// A music file decoded on the fly to mono samples (channels averaged) at its own sample rate.
/// Nothing is held in memory beyond the decoder's buffer, so long songs are fine.
pub struct MonoSamples {
    decoder: FileDecoder,
    channels: usize,
    rate: u32,
    position: ReadPosition,
}

impl MonoSamples {
    /// Opens `path` for decoding.
    pub fn open(path: &Path) -> Result<Self, AudioError> {
        let (decoder, position) = open_decoder(path)?;
        Ok(Self {
            channels: usize::from(decoder.channels().get()),
            rate: decoder.sample_rate().get(),
            decoder,
            position,
        })
    }

    /// Samples per second.
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// How far into the file decoding has got, for showing progress.
    pub fn position(&self) -> ReadPosition {
        self.position.clone()
    }
}

impl Iterator for MonoSamples {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        // The channel count and rate are read once, when the file opens. A file whose format
        // changes part way (chained Ogg streams, rare in music files) is then averaged with the
        // first part's channel count and timed at its rate: channels may pair up wrongly and
        // times drift for the rest of the file. Beat detection only needs the loudness envelope,
        // so this is accepted rather than handled; playback decodes separately and isn't affected.
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
