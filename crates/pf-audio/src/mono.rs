//! Decoding a music file to mono samples, a few at a time, for analysis.

use crate::decode::{FileDecoder, next_sample, open_decoder};
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

/// Every channel's sample at one point in the music, taken together: the channels mixed to one
/// (averaged), the highest and the lowest of them, and the left one's.
///
/// The mix alone can't say how loud the music is: two channels that differ mix to less than the
/// louder one, a sound on one side alone to half of it, and two that mirror each other to
/// nothing. The highest and lowest are the louder channel's. The left channel's is what xLights
/// takes as the music's level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelSample {
    pub mix: f32,
    pub high: f32,
    pub low: f32,
    /// The left channel's sample: the first channel's, however many there are.
    pub left: f32,
}

impl ChannelSample {
    /// One channel's sample: the mix, the highest, the lowest, and the left are all it.
    pub fn mono(sample: f32) -> Self {
        Self {
            mix: sample,
            high: sample,
            low: sample,
            left: sample,
        }
    }

    /// A left and a right sample.
    pub fn stereo(left: f32, right: f32) -> Self {
        Self {
            mix: (left + right) / 2.0,
            high: left.max(right),
            low: left.min(right),
            left,
        }
    }
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

    /// The same samples, each with the highest and lowest of the channels it was mixed from, and
    /// the left channel's.
    pub fn with_extremes(self) -> ChannelSamples {
        ChannelSamples(self)
    }

    /// The next sample of every channel, taken together.
    fn read(&mut self) -> Option<ChannelSample> {
        // The channel count and rate are read once, when the file opens. A file whose format
        // changes part way (chained Ogg streams, rare in music files) is then averaged with the
        // first part's channel count and timed at its rate: channels may pair up wrongly and
        // times drift for the rest of the file. Beat detection only needs the loudness envelope,
        // so this is accepted rather than handled; playback decodes separately and isn't affected.
        let first = next_sample(&mut self.decoder)?;
        let (mut sum, mut high, mut low) = (first, first, first);
        for _ in 1..self.channels {
            let sample = next_sample(&mut self.decoder).unwrap_or(0.0);
            sum += sample;
            high = high.max(sample);
            low = low.min(sample);
        }
        Some(ChannelSample {
            mix: sum / self.channels as f32,
            high,
            low,
            left: first,
        })
    }
}

impl Iterator for MonoSamples {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        self.read().map(|sample| sample.mix)
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

/// A music file's mono samples with the highest, lowest, and left channel beside each (see
/// [`MonoSamples::with_extremes`]).
#[derive(Debug)]
pub struct ChannelSamples(MonoSamples);

impl Iterator for ChannelSamples {
    type Item = ChannelSample;

    fn next(&mut self) -> Option<ChannelSample> {
        self.0.read()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_channel_is_its_own_mix_and_extremes() {
        assert_eq!(
            ChannelSample::mono(-0.25),
            ChannelSample {
                mix: -0.25,
                high: -0.25,
                low: -0.25,
                left: -0.25
            }
        );
    }

    #[test]
    fn two_channels_mix_to_their_mean_and_keep_the_louder_ones_extremes() {
        // A sound on one side alone mixes to half of it; the extremes still have all of it.
        assert_eq!(
            ChannelSample::stereo(0.0, 0.8),
            ChannelSample {
                mix: 0.4,
                high: 0.8,
                low: 0.0,
                left: 0.0
            }
        );
        // Channels that mirror each other mix to nothing.
        assert_eq!(
            ChannelSample::stereo(0.5, -0.5),
            ChannelSample {
                mix: 0.0,
                high: 0.5,
                low: -0.5,
                left: 0.5
            }
        );
        // The same on both sides is what one channel would be.
        assert_eq!(ChannelSample::stereo(0.3, 0.3), ChannelSample::mono(0.3));
    }
}
