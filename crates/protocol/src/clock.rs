//! Time and identifiers, injected rather than read from the machine, so that a test decides both
//! and two runs over the same input produce the same events.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};

/// Where the current time comes from. Nothing below the runtime reads the machine's clock, so that
/// a test decides what "now" is and a replayed log produces the same answers twice.
pub trait Clock {
    /// The time now, in UTC.
    fn now(&self) -> DateTime<Utc>;
}

/// Where an identifier that Farik cannot derive from what it already has comes from.
pub trait IdSource {
    /// A new session id.
    fn session_id(&self) -> String;
}

/// A clock that always answers the same time. For tests, in this crate and in every other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedClock {
    /// The time it answers.
    pub at: DateTime<Utc>,
}

impl FixedClock {
    /// A clock fixed at `at`.
    #[must_use]
    pub fn new(at: DateTime<Utc>) -> Self {
        Self { at }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.at
    }
}

/// A clock a test moves by hand, for a block that has to age or a sleep that has to end. For
/// tests, in this crate and in every other.
#[derive(Debug)]
pub struct MovableClock(Mutex<DateTime<Utc>>);

impl MovableClock {
    /// A clock at `at` until it is moved.
    #[must_use]
    pub fn new(at: DateTime<Utc>) -> Self {
        Self(Mutex::new(at))
    }

    /// Moves the clock to `at`.
    pub fn set(&self, at: DateTime<Utc>) {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = at;
    }
}

impl Clock for MovableClock {
    fn now(&self) -> DateTime<Utc> {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// An identifier source that hands out `session-1`, `session-2`, and so on. For tests, in this
/// crate and in every other.
#[derive(Debug, Default)]
pub struct SequentialIds {
    handed_out: AtomicU64,
}

impl SequentialIds {
    /// A source whose first identifier is `session-1`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            handed_out: AtomicU64::new(0),
        }
    }
}

impl IdSource for SequentialIds {
    fn session_id(&self) -> String {
        format!(
            "session-{}",
            self.handed_out.fetch_add(1, Ordering::Relaxed) + 1
        )
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::{Clock, FixedClock, IdSource, SequentialIds};

    fn at() -> DateTime<Utc> {
        "2026-09-17T10:00:00Z"
            .parse::<DateTime<Utc>>()
            .expect("a fixed timestamp")
    }

    #[test]
    fn answers_the_same_time_however_often_it_is_asked() {
        let clock = FixedClock::new(at());
        assert_eq!(clock.now(), at());
        assert_eq!(clock.now(), at());
    }

    #[test]
    fn hands_out_a_new_session_id_each_time() {
        let ids = SequentialIds::new();
        assert_eq!(ids.session_id(), "session-1");
        assert_eq!(ids.session_id(), "session-2");
        assert_eq!(ids.session_id(), "session-3");
    }

    #[test]
    fn starts_a_default_source_at_the_first_id() {
        let ids = SequentialIds::default();
        assert_eq!(ids.session_id(), "session-1");
    }
}
