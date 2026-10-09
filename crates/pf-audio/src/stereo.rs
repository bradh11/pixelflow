//! Decoding a music file to left and right samples, for telling the middle of the mix (where
//! the voice usually sits) from its sides.

use crate::decode::{FileDecoder, open_decoder};
use crate::error::AudioError;
use crate::progress::ReadPosition;
use rodio::Source;
use std::path::Path;

/// A music file decoded on the fly to (left, right) pairs at its own sample rate. A mono file
/// gives the same sample on both sides; past two channels, only the first two are kept.
pub struct StereoFrames {
    decoder: FileDecoder,
    channels: usize,
    rate: u32,
    position: ReadPosition,
}

impl StereoFrames {
    pub fn open(path: &Path) -> Result<Self, AudioError> {
        let (decoder, position) = open_decoder(path)?;
        Ok(Self {
            channels: usize::from(decoder.channels().get()),
            rate: decoder.sample_rate().get(),
            decoder,
            position,
        })
    }

    /// Pairs per second.
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// How far into the file decoding has got, for showing progress.
    pub fn position(&self) -> ReadPosition {
        self.position.clone()
    }
}

impl Iterator for StereoFrames {
    type Item = (f32, f32);

    fn next(&mut self) -> Option<(f32, f32)> {
        let left = self.decoder.next()?;
        if self.channels == 1 {
            return Some((left, left));
        }
        let right = self.decoder.next().unwrap_or(0.0);
        for _ in 2..self.channels {
            self.decoder.next();
        }
        Some((left, right))
    }
}

impl std::fmt::Debug for StereoFrames {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StereoFrames")
            .field("channels", &self.channels)
            .field("rate", &self.rate)
            .finish_non_exhaustive()
    }
}
