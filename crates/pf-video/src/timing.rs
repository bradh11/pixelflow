//! When each video frame is, and which sequence frame and stretch of music go with it.
//!
//! Every time is worked out from the frame's number, never by adding up frame lengths, so a long
//! video doesn't drift: frame `n` is at exactly `start + n / fps` seconds (rounded down to the
//! microsecond).

/// The video frames of a stretch of the sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameClock {
    start_ms: u64,
    fps: u32,
    frames: u64,
}

impl FrameClock {
    /// Frames at `fps` from `start_ms` until `end_ms` (the last one starts before `end_ms`).
    pub fn new(start_ms: u64, end_ms: u64, fps: u32) -> Self {
        let fps = fps.max(1);
        let span = end_ms.saturating_sub(start_ms);
        Self {
            start_ms,
            fps,
            frames: (span * u64::from(fps)).div_ceil(1000),
        }
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    pub fn fps(&self) -> u32 {
        self.fps
    }

    pub fn start_ms(&self) -> u64 {
        self.start_ms
    }

    /// How long the video plays, in microseconds: `frames / fps`.
    pub fn duration_us(&self) -> u64 {
        self.frames * 1_000_000 / u64::from(self.fps)
    }

    /// When frame `n` shows, in microseconds from the start of the sequence.
    pub fn time_us(&self, n: u64) -> u64 {
        self.start_ms * 1000 + n * 1_000_000 / u64::from(self.fps)
    }

    /// The sequence frame showing at video frame `n`: the one playback would be on then.
    pub fn sequence_frame(&self, n: u64, frame_ms: u32) -> u64 {
        self.time_us(n) / (u64::from(frame_ms.max(1)) * 1000)
    }

    /// The music samples (at `rate` per second) the video plays over: (first, count).
    pub fn samples(&self, rate: u32) -> (u64, u64) {
        let rate = u64::from(rate);
        let first = self.start_ms * rate / 1000;
        let count = self.frames * rate / u64::from(self.fps);
        (first, count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_cover_the_range() {
        let clock = FrameClock::new(0, 1000, 30);
        assert_eq!(clock.frames(), 30);
        assert_eq!(
            FrameClock::new(0, 1001, 30).frames(),
            31,
            "a part frame still shows"
        );
        assert_eq!(FrameClock::new(500, 500, 30).frames(), 0);
        assert_eq!(
            FrameClock::new(2000, 1000, 30).frames(),
            0,
            "a backwards range is empty"
        );
        assert_eq!(clock.duration_us(), 1_000_000);
    }

    #[test]
    fn frame_times_are_n_over_fps() {
        let clock = FrameClock::new(10_000, 20_000, 30);
        assert_eq!(clock.time_us(0), 10_000_000);
        assert_eq!(clock.time_us(1), 10_033_333);
        assert_eq!(clock.time_us(2), 10_066_666);
        assert_eq!(clock.time_us(30), 11_000_000);
        let sixty = FrameClock::new(0, 1000, 60);
        assert_eq!(sixty.time_us(59), 983_333);
    }

    #[test]
    fn no_drift_over_long_ranges() {
        // Ten hours at 30 and 60 fps: every whole second lands exactly on a frame.
        for fps in [30, 60] {
            let clock = FrameClock::new(0, 36_000_000, fps);
            assert_eq!(clock.frames(), 36_000 * u64::from(fps));
            for second in [1u64, 59, 3600, 35_999] {
                assert_eq!(clock.time_us(second * u64::from(fps)), second * 1_000_000);
            }
            assert_eq!(clock.duration_us(), 36_000_000_000);
            // Each frame starts no more than a microsecond before its exact time.
            let n = clock.frames() - 1;
            let exact = n as f64 * 1e6 / f64::from(fps);
            assert!((exact - clock.time_us(n) as f64) < 1.0);
        }
    }

    #[test]
    fn the_sequence_frame_is_the_one_playing_then() {
        // 25 ms sequence frames under 30 fps video: 0, 33.3, 66.6, 100 ms.
        let clock = FrameClock::new(0, 1000, 30);
        let frames: Vec<u64> = (0..4).map(|n| clock.sequence_frame(n, 25)).collect();
        assert_eq!(frames, vec![0, 1, 2, 4]);
        // From a later start, frames count from the sequence's own start.
        let later = FrameClock::new(1000, 2000, 30);
        assert_eq!(later.sequence_frame(0, 50), 20);
        assert_eq!(later.sequence_frame(3, 50), 22);
    }

    #[test]
    fn sound_matches_the_frames() {
        let clock = FrameClock::new(1500, 4500, 30);
        assert_eq!(clock.samples(44_100), (66_150, 132_300));
        assert_eq!(clock.samples(48_000), (72_000, 144_000));
        // A part frame at the end: the sound lasts exactly as long as the frames.
        let odd = FrameClock::new(0, 1001, 30);
        assert_eq!(odd.samples(48_000).1, 31 * 48_000 / 30);
    }
}
