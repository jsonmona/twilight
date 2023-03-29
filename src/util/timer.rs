use std::time::{Duration, Instant};

/// A convenient way to do something on a regular interval.
/// Logging performance metrics, for instance.
pub struct Timer {
    prev_time: Instant,
    duration: Duration,
}

impl Timer {
    pub fn new(duration: Duration) -> Self {
        Timer {
            prev_time: Instant::now(),
            duration,
        }
    }

    /// Returns true if enough time has passed
    pub fn poll(&mut self) -> bool {
        let now = Instant::now();
        if now - self.prev_time > self.duration {
            self.prev_time = now;
            true
        } else {
            false
        }
    }
}
