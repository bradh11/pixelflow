//! Clocks that sequence playback follows: the music itself, or a silent stopwatch.

use crate::error::AudioError;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::time::{Duration, Instant};

/// Something with a play position: lights follow it so they stay in step with the music.
pub trait AudioClock {
    /// Plays from `position`.
    fn start(&mut self, position: Duration);
    fn pause(&mut self);
    fn resume(&mut self);
    fn seek(&mut self, position: Duration);
    /// Where playback is now.
    fn position(&self) -> Duration;
    /// 0.0 (silent) to 1.0 (full).
    fn set_volume(&mut self, volume: f32);
}

/// A stopwatch that keeps time without sound (sequences with no music, and tests).
#[derive(Debug)]
pub struct SilentClock {
    /// Position when the clock last started or was paused.
    base: Duration,
    /// When it started running; `None` while paused.
    since: Option<Instant>,
}

impl SilentClock {
    pub fn new() -> Self {
        Self {
            base: Duration::ZERO,
            since: None,
        }
    }
}

impl Default for SilentClock {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioClock for SilentClock {
    fn start(&mut self, position: Duration) {
        self.base = position;
        self.since = Some(Instant::now());
    }

    fn pause(&mut self) {
        self.base = self.position();
        self.since = None;
    }

    fn resume(&mut self) {
        if self.since.is_none() {
            self.since = Some(Instant::now());
        }
    }

    fn seek(&mut self, position: Duration) {
        self.base = position;
        if self.since.is_some() {
            self.since = Some(Instant::now());
        }
    }

    fn position(&self) -> Duration {
        self.base + self.since.map_or(Duration::ZERO, |s| s.elapsed())
    }

    fn set_volume(&mut self, _volume: f32) {}
}

/// Plays a music file on the default sound output; its play position is the clock.
pub struct MusicPlayer {
    // Keeps the output device open for as long as the player lives.
    _output: MixerDeviceSink,
    player: Player,
}

impl MusicPlayer {
    /// Opens `path` on the default output, paused at the start.
    pub fn open(path: &Path) -> Result<Self, AudioError> {
        let shown = path.display().to_string();
        let file = File::open(path).map_err(|source| AudioError::Open {
            path: shown.clone(),
            source,
        })?;
        let decoder = Decoder::try_from(BufReader::new(file)).map_err(|e| AudioError::Decode {
            path: shown.clone(),
            reason: e.to_string(),
        })?;
        let output =
            DeviceSinkBuilder::open_default_sink().map_err(|e| AudioError::NoOutput(e.to_string()))?;
        let player = Player::connect_new(output.mixer());
        player.pause();
        player.append(decoder);
        Ok(Self {
            _output: output,
            player,
        })
    }
}

impl AudioClock for MusicPlayer {
    fn start(&mut self, position: Duration) {
        let _ = self.player.try_seek(position);
        self.player.play();
    }

    fn pause(&mut self) {
        self.player.pause();
    }

    fn resume(&mut self) {
        self.player.play();
    }

    fn seek(&mut self, position: Duration) {
        let _ = self.player.try_seek(position);
    }

    fn position(&self) -> Duration {
        self.player.get_pos()
    }

    fn set_volume(&mut self, volume: f32) {
        self.player.set_volume(volume.clamp(0.0, 1.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_silent_clock_runs_pauses_and_seeks() {
        let mut clock = SilentClock::new();
        assert_eq!(clock.position(), Duration::ZERO);
        clock.start(Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(30));
        let running = clock.position();
        assert!(running >= Duration::from_millis(5030), "{running:?}");
        clock.pause();
        let paused = clock.position();
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(clock.position(), paused, "paused clocks stand still");
        clock.seek(Duration::from_secs(60));
        assert_eq!(clock.position(), Duration::from_secs(60));
        clock.resume();
        std::thread::sleep(Duration::from_millis(20));
        assert!(clock.position() > Duration::from_secs(60));
    }
}
