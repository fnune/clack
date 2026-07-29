use core::time::Duration;

/// Not `std::time::Instant`: tests need to construct arbitrary points in time,
/// which `Instant` does not allow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(Duration);

impl Timestamp {
    pub const ZERO: Self = Self(Duration::ZERO);

    pub const fn since_start(elapsed: Duration) -> Self {
        Self(elapsed)
    }

    pub fn elapsed_since(self, earlier: Self) -> Duration {
        self.0.saturating_sub(earlier.0)
    }
}

pub trait Clock {
    fn now(&self) -> Timestamp;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_saturates_when_ordering_is_reversed() {
        let early = Timestamp::since_start(Duration::from_millis(10));
        let late = Timestamp::since_start(Duration::from_millis(40));

        assert_eq!(late.elapsed_since(early), Duration::from_millis(30));
        assert_eq!(early.elapsed_since(late), Duration::ZERO);
    }
}
