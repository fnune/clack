use std::time::Instant;

use clack_core::{Clock, Timestamp};

#[derive(Clone, Copy, Debug)]
pub struct MonotonicClock {
    start: Instant,
}

impl MonotonicClock {
    pub fn started() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::started()
    }
}

impl Clock for MonotonicClock {
    fn now(&self) -> Timestamp {
        Timestamp::since_start(self.start.elapsed())
    }
}
