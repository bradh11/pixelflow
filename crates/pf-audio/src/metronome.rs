//! A metronome: a short click on every beat, for lining the preview up with what is heard (see
//! [`crate::MusicPlayer::metronome`]). It never ends, and a jump lands on the exact sample, so the
//! click times are known to the sample.

use rodio::source::SeekError;
use rodio::{ChannelCount, Sample, SampleRate, Source};
use std::f32::consts::TAU;
use std::time::Duration;

/// Samples a second.
pub const METRONOME_RATE: u32 = 48_000;
/// How long a click rings.
const CLICK: Duration = Duration::from_millis(30);
/// Its pitch, and the pitch of the first beat of each bar of four.
const CLICK_HZ: f32 = 1_000.0;
const ACCENT_HZ: f32 = 1_500.0;

/// Clicks every `every`, the first at time zero.
#[derive(Debug, Clone)]
pub struct Metronome {
    period: u64,
    click: u64,
    /// The next sample's number.
    at: u64,
}

impl Metronome {
    pub fn new(every: Duration) -> Self {
        let samples = |d: Duration| (d.as_secs_f64() * f64::from(METRONOME_RATE)).round() as u64;
        let period = samples(every).max(2);
        Self {
            period,
            click: samples(CLICK).min(period / 2).max(1),
            at: 0,
        }
    }

    /// Sample `i` of the clicks.
    fn sample(&self, i: u64) -> Sample {
        let beat = i / self.period;
        let into = i % self.period;
        if into >= self.click {
            return 0.0;
        }
        let hz = if beat.is_multiple_of(4) {
            ACCENT_HZ
        } else {
            CLICK_HZ
        };
        let t = into as f32 / METRONOME_RATE as f32;
        // A quick rise (no pop) then a ring that dies away.
        let rise = (into as f32 / 48.0).min(1.0);
        let decay = (-t * 160.0).exp();
        0.6 * rise * decay * (TAU * hz * t).sin()
    }
}

impl Iterator for Metronome {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let sample = self.sample(self.at);
        self.at += 1;
        Some(sample)
    }
}

impl Source for Metronome {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        ChannelCount::new(1).unwrap_or(ChannelCount::MIN)
    }

    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(METRONOME_RATE).unwrap_or(SampleRate::MIN)
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.at = (pos.as_secs_f64() * f64::from(METRONOME_RATE)).round() as u64;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicks_start_on_the_beat_and_are_silent_between() {
        let mut clicks = Metronome::new(Duration::from_millis(500));
        let samples: Vec<Sample> = clicks.by_ref().take(48_000).collect();
        let loud = |range: std::ops::Range<usize>| samples[range].iter().any(|s| s.abs() > 0.05);
        assert!(loud(0..240), "a click at 0 ms");
        assert!(!loud(2_000..24_000), "nothing between beats");
        assert!(loud(24_000..24_240), "a click at 500 ms");
        assert!(!loud(23_900..24_000), "nothing just before the beat");
    }

    #[test]
    fn a_jump_lands_on_the_sample() {
        let mut clicks = Metronome::new(Duration::from_millis(500));
        clicks.try_seek(Duration::from_millis(1_000)).unwrap();
        let fresh = Metronome::new(Duration::from_millis(500));
        let after: Vec<Sample> = clicks.take(100).collect();
        let expected: Vec<Sample> = (48_000..48_100).map(|i| fresh.sample(i)).collect();
        assert_eq!(after, expected);
    }
}
