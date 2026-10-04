//! Fixed-rate frame clock.

use std::time::{Duration, Instant};

/// Below this, the clock spins instead of sleeping, for low jitter.
const SPIN_WINDOW: Duration = Duration::from_millis(1);

/// Paces frames at a fixed rate. Deadlines advance by exactly one period, so timing does
/// not drift; if output falls more than a full period behind, the schedule restarts.
#[derive(Debug)]
pub struct FrameClock {
    period: Duration,
    next: Instant,
}

impl FrameClock {
    /// A clock for `fps` frames per second (clamped to 1–1000) whose first frame is due now.
    pub fn new(fps: u16) -> Self {
        Self {
            period: Duration::from_secs_f64(1.0 / f64::from(fps.clamp(1, 1000))),
            next: Instant::now(),
        }
    }

    pub fn period(&self) -> Duration {
        self.period
    }

    /// Waits until the next frame is due. Returns true when the frame is late (the deadline
    /// was missed by more than one period).
    pub fn wait(&mut self) -> bool {
        let now = Instant::now();
        if now < self.next {
            let remaining = self.next - now;
            if remaining > SPIN_WINDOW {
                std::thread::sleep(remaining - SPIN_WINDOW);
            }
            while Instant::now() < self.next {
                std::hint::spin_loop();
            }
        }
        let now = Instant::now();
        let late = now > self.next + self.period;
        self.next = if late {
            now + self.period
        } else {
            self.next + self.period
        };
        late
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paces_frames_at_the_requested_rate() {
        let mut clock = FrameClock::new(100);
        assert_eq!(clock.period(), Duration::from_millis(10));
        let start = Instant::now();
        for _ in 0..11 {
            clock.wait();
        }
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(100), "{elapsed:?}");
        assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    }

    #[test]
    fn reports_late_frames_and_restarts_the_schedule() {
        let mut clock = FrameClock::new(100);
        clock.wait();
        std::thread::sleep(Duration::from_millis(35));
        assert!(clock.wait());
    }
}
