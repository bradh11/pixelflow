//! Per-controller health with exponential backoff after send failures.

use std::time::{Duration, Instant};

const MIN_BACKOFF: Duration = Duration::from_millis(250);
const MAX_BACKOFF: Duration = Duration::from_secs(5);

/// How a controller's output is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerState {
    /// Sending normally.
    Ok,
    /// Sends are failing; retrying with backoff while other controllers keep running.
    Degraded,
    /// The controller's address could not be resolved; nothing is sent.
    Unresolved,
}

#[derive(Debug, Clone)]
pub(crate) struct Health {
    pub state: ControllerState,
    backoff: Duration,
    retry_at: Option<Instant>,
}

impl Health {
    pub fn new(state: ControllerState) -> Self {
        Self {
            state,
            backoff: MIN_BACKOFF,
            retry_at: None,
        }
    }

    /// Whether to attempt sending at `now`.
    pub fn ready(&self, now: Instant) -> bool {
        match self.state {
            ControllerState::Ok => true,
            ControllerState::Degraded => self.retry_at.is_none_or(|at| now >= at),
            ControllerState::Unresolved => false,
        }
    }

    pub fn on_success(&mut self) {
        self.state = ControllerState::Ok;
        self.backoff = MIN_BACKOFF;
        self.retry_at = None;
    }

    /// Marks the controller degraded and schedules the next attempt, doubling the wait each
    /// time up to five seconds.
    pub fn on_failure(&mut self, now: Instant) {
        self.state = ControllerState::Degraded;
        self.retry_at = Some(now + self.backoff);
        self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_back_off_exponentially_up_to_five_seconds_and_success_resets() {
        let t0 = Instant::now();
        let mut health = Health::new(ControllerState::Ok);
        assert!(health.ready(t0));

        health.on_failure(t0);
        assert_eq!(health.state, ControllerState::Degraded);
        assert!(!health.ready(t0 + Duration::from_millis(249)));
        assert!(health.ready(t0 + Duration::from_millis(250)));

        health.on_failure(t0);
        assert!(!health.ready(t0 + Duration::from_millis(499)));
        assert!(health.ready(t0 + Duration::from_millis(500)));

        for _ in 0..10 {
            health.on_failure(t0);
        }
        assert!(health.ready(t0 + Duration::from_secs(5)));

        health.on_success();
        assert_eq!(health.state, ControllerState::Ok);
        health.on_failure(t0);
        assert!(health.ready(t0 + Duration::from_millis(250)));
    }

    #[test]
    fn unresolved_controllers_never_send() {
        let health = Health::new(ControllerState::Unresolved);
        assert!(!health.ready(Instant::now() + Duration::from_secs(60)));
    }
}
