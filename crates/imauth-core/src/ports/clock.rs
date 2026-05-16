use chrono::{DateTime, Utc};

/// Wall-clock boundary for use cases that need a notion of "now".
/// Concrete impls live in adapters; tests inject deterministic clocks
/// via `MockClock`.
#[cfg_attr(test, mockall::automock)]
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// Production implementation backed by the OS wall clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_returns_recent_utc() {
        let before = Utc::now();
        let now = SystemClock.now();
        let after = Utc::now();
        assert!(now >= before && now <= after);
    }

    #[test]
    fn mock_clock_returns_injected_time() {
        let fixed = DateTime::parse_from_rfc3339("2026-01-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut clock = MockClock::new();
        clock.expect_now().return_const(fixed);
        assert_eq!(clock.now(), fixed);
    }
}
