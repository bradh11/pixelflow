//! Clocks that sequence playback follows: the music itself ([`crate::MusicPlayer`]), or a silent
//! stopwatch.

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
    /// How fast it plays: 1.0 as written, 0.5 at half speed (the music's pitch drops with it).
    /// Clocks that can't change speed play as written.
    fn set_speed(&mut self, _speed: f32) {}
    /// Whether the music has played to its end. Clocks without music always have.
    fn finished(&self) -> bool {
        true
    }
    /// Why the music can't be heard, when something went wrong after it started (a plain sentence).
    fn problem(&self) -> Option<String> {
        None
    }
}

/// A stopwatch that keeps time without sound (sequences with no music, and tests).
#[derive(Debug)]
pub struct SilentClock {
    /// Position when the clock last started, was paused, or changed speed.
    base: Duration,
    /// When it started running; `None` while paused.
    since: Option<Instant>,
    speed: f64,
}

impl SilentClock {
    pub fn new() -> Self {
        Self {
            base: Duration::ZERO,
            since: None,
            speed: 1.0,
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
        self.base
            + self
                .since
                .map_or(Duration::ZERO, |s| s.elapsed().mul_f64(self.speed))
    }

    fn set_volume(&mut self, _volume: f32) {}

    fn set_speed(&mut self, speed: f32) {
        let speed = if speed.is_finite() && speed > 0.0 {
            f64::from(speed.min(1.0))
        } else {
            1.0
        };
        self.base = self.position();
        if self.since.is_some() {
            self.since = Some(Instant::now());
        }
        self.speed = speed;
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

    #[test]
    fn the_silent_clock_runs_slower() {
        let mut clock = SilentClock::new();
        clock.set_speed(0.5);
        let started = Instant::now();
        clock.start(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(100));
        let slowed = clock.position();
        let elapsed = started.elapsed();
        assert!(slowed >= Duration::from_millis(50), "{slowed:?}");
        assert!(
            slowed <= elapsed.mul_f64(0.5) + Duration::from_millis(1),
            "{slowed:?} after {elapsed:?}"
        );
        clock.set_speed(1.0);
        std::thread::sleep(Duration::from_millis(30));
        assert!(clock.position() >= slowed + Duration::from_millis(30));
    }
}
