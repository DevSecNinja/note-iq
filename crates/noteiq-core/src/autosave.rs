use std::time::{Duration, Instant};

const IDLE_DELAY: Duration = Duration::from_millis(800);
const MAX_INTERVAL: Duration = Duration::from_secs(10);
const RETRY_BACKOFF_START: Duration = Duration::from_secs(5);
const RETRY_BACKOFF_MAX: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
pub struct AutosaveSchedule {
    first_unsaved_change: Option<Instant>,
    last_change: Option<Instant>,
    consecutive_failures: u32,
    retry_not_before: Option<Instant>,
}

impl AutosaveSchedule {
    pub fn changed(&mut self, now: Instant) {
        self.first_unsaved_change.get_or_insert(now);
        self.last_change = Some(now);
    }

    pub fn is_due(&self, now: Instant) -> bool {
        let (Some(first), Some(last)) = (self.first_unsaved_change, self.last_change) else {
            return false;
        };
        if self.retry_not_before.is_some_and(|retry| now < retry) {
            return false;
        }
        now.saturating_duration_since(last) >= IDLE_DELAY
            || now.saturating_duration_since(first) >= MAX_INTERVAL
    }

    pub fn saved(&mut self) {
        self.first_unsaved_change = None;
        self.last_change = None;
        self.consecutive_failures = 0;
        self.retry_not_before = None;
    }

    /// Delays the next automatic attempt, doubling up to a cap, so a broken destination is not
    /// retried (and logged) on every timer tick. Explicit saves are never throttled.
    pub fn failed(&mut self, now: Instant) {
        self.first_unsaved_change.get_or_insert(now);
        self.last_change.get_or_insert(now);
        let delay = RETRY_BACKOFF_START
            .saturating_mul(1 << self.consecutive_failures.min(6))
            .min(RETRY_BACKOFF_MAX);
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.retry_not_before = Some(now.checked_add(delay).unwrap_or(now));
    }

    pub fn is_dirty(&self) -> bool {
        self.first_unsaved_change.is_some()
    }
}
