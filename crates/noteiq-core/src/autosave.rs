use std::time::{Duration, Instant};

const IDLE_DELAY: Duration = Duration::from_millis(800);
const MAX_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug, Default)]
pub struct AutosaveSchedule {
    first_unsaved_change: Option<Instant>,
    last_change: Option<Instant>,
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
        now.saturating_duration_since(last) >= IDLE_DELAY
            || now.saturating_duration_since(first) >= MAX_INTERVAL
    }

    pub fn saved(&mut self) {
        self.first_unsaved_change = None;
        self.last_change = None;
    }

    pub fn is_dirty(&self) -> bool {
        self.first_unsaved_change.is_some()
    }
}
