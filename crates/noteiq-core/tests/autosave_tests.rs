use noteiq_core::autosave::AutosaveSchedule;
use std::time::{Duration, Instant};

#[test]
fn saves_after_pause_and_caps_continuous_typing_interval() {
    let start = Instant::now();
    let mut schedule = AutosaveSchedule::default();
    schedule.changed(start);

    assert!(!schedule.is_due(start + Duration::from_millis(799)));
    assert!(schedule.is_due(start + Duration::from_millis(800)));

    schedule.saved();
    for second in 1..10 {
        schedule.changed(start + Duration::from_secs(second));
        assert!(
            !schedule.is_due(start + Duration::from_secs(second)),
            "typing continues before the periodic deadline"
        );
    }
    assert!(schedule.is_due(start + Duration::from_secs(11)));
}

#[test]
fn a_failed_autosave_backs_off_instead_of_retrying_every_tick() {
    let start = Instant::now();
    let mut schedule = AutosaveSchedule::default();
    schedule.changed(start);
    let failed_at = start + Duration::from_secs(1);
    assert!(schedule.is_due(failed_at));

    schedule.failed(failed_at);

    assert!(schedule.is_dirty(), "the unsaved change is still pending");
    assert!(!schedule.is_due(failed_at + Duration::from_millis(200)));
    assert!(!schedule.is_due(failed_at + Duration::from_millis(4_999)));
    assert!(schedule.is_due(failed_at + Duration::from_secs(5)));
}

#[test]
fn repeated_failures_back_off_up_to_one_minute() {
    let start = Instant::now();
    let mut schedule = AutosaveSchedule::default();
    schedule.changed(start);
    let mut now = start + Duration::from_secs(1);
    let mut waits = Vec::new();
    for _ in 0..7 {
        schedule.failed(now);
        let mut wait = 0;
        while !schedule.is_due(now + Duration::from_secs(wait)) {
            wait += 1;
        }
        waits.push(wait);
        now += Duration::from_secs(wait);
    }

    assert_eq!(waits, [5, 10, 20, 40, 60, 60, 60]);
}

#[test]
fn typing_during_backoff_does_not_trigger_an_early_retry() {
    let start = Instant::now();
    let mut schedule = AutosaveSchedule::default();
    schedule.changed(start);
    schedule.failed(start + Duration::from_secs(1));

    schedule.changed(start + Duration::from_secs(2));

    assert!(!schedule.is_due(start + Duration::from_secs(3)));
    assert!(schedule.is_due(start + Duration::from_secs(6)));
}

#[test]
fn a_successful_save_clears_the_backoff() {
    let start = Instant::now();
    let mut schedule = AutosaveSchedule::default();
    schedule.changed(start);
    schedule.failed(start + Duration::from_secs(1));
    schedule.failed(start + Duration::from_secs(7));

    schedule.saved();
    assert!(!schedule.is_dirty());
    schedule.changed(start + Duration::from_secs(8));

    assert!(
        schedule.is_due(start + Duration::from_secs(9)),
        "only the idle delay applies after a success"
    );
    schedule.failed(start + Duration::from_secs(9));
    assert!(!schedule.is_due(start + Duration::from_secs(13)));
    assert!(
        schedule.is_due(start + Duration::from_secs(14)),
        "the backoff restarts at its first step"
    );
}
