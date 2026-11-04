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
