mod common;

use chrono::{DateTime, FixedOffset, TimeDelta};
use common::{invitee, meeting};
use noteiq_core::{
    meeting_reminder::{
        MAX_DELAY_MINUTES, MAX_DELIVERY_ATTEMPTS, ReminderHistory, ReminderTracker,
        is_reminder_key, reminder_delay, reminder_key, reminder_text,
    },
    model::{Meeting, Rsvp},
};

const DELAY: TimeDelta = TimeDelta::minutes(5);

/// `meeting()` ends at 14:30.
fn end() -> DateTime<FixedOffset> {
    meeting().end.unwrap()
}

fn after_end(minutes: i64) -> DateTime<FixedOffset> {
    end() + TimeDelta::minutes(minutes)
}

/// A tracker whose latest lookup started `minutes` after the meeting ended, with the user in a
/// call while the meetings were in progress.
fn looked_up(meetings: &[Meeting], minutes: i64) -> ReminderTracker {
    let mut tracker = ReminderTracker::default();
    tracker.observe(meetings, after_end(minutes));
    tracker.mark_joined(end() - TimeDelta::minutes(10));
    tracker
}

fn due_ids(tracker: &ReminderTracker, now: DateTime<FixedOffset>) -> Vec<String> {
    tracker
        .due(now, DELAY, &ReminderHistory::default(), &[])
        .into_iter()
        .map(|meeting| meeting.id)
        .collect()
}

#[test]
fn reminds_after_the_delay_unless_the_meeting_was_transcribed() {
    let untranscribed = Meeting {
        has_transcript: Some(false),
        ..meeting()
    };
    let unknown = Meeting {
        id: "unknown".into(),
        has_transcript: None,
        ..meeting()
    };
    let transcribed = Meeting {
        id: "transcribed".into(),
        has_transcript: Some(true),
        ..meeting()
    };
    let tracker = looked_up(&[untranscribed, unknown, transcribed], 6);

    assert!(due_ids(&tracker, after_end(4)).is_empty(), "too early");
    assert_eq!(due_ids(&tracker, after_end(6)), ["event-42", "unknown"]);
}

#[test]
fn only_meetings_the_user_was_in_a_call_for_are_reminded() {
    let mut tracker = ReminderTracker::default();
    let earlier = Meeting {
        id: "earlier".into(),
        start: Some(end() - TimeDelta::hours(2)),
        end: Some(end() - TimeDelta::minutes(90)),
        ..meeting()
    };
    tracker.observe(
        &[meeting(), earlier.clone()],
        end() - TimeDelta::minutes(20),
    );
    assert!(tracker.any_in_progress(end() - TimeDelta::minutes(10)));
    assert!(!tracker.any_in_progress(after_end(1)));

    tracker.mark_joined(end() - TimeDelta::minutes(10));
    tracker.observe(&[], after_end(6));
    assert!(tracker.has_joined(&meeting()));
    assert!(!tracker.has_joined(&earlier));
    assert_eq!(due_ids(&tracker, after_end(6)), ["event-42"]);
    assert!(
        !tracker.needs_lookup(after_end(6), DELAY, &ReminderHistory::default(), &[]),
        "a meeting the user skipped needs no lookup either"
    );
}

#[test]
fn skips_meetings_that_need_no_note() {
    let skipped = [
        Meeting {
            cancelled: true,
            ..meeting()
        },
        Meeting {
            declined: true,
            ..meeting()
        },
        Meeting {
            your_rsvp: Some(Rsvp::Declined),
            ..meeting()
        },
        Meeting {
            all_day: true,
            ..meeting()
        },
        Meeting {
            start: Some(end() - TimeDelta::days(2)),
            ..meeting()
        },
        // Lunch or focus time: organised by the user with nobody invited.
        Meeting {
            your_rsvp: Some(Rsvp::Organizer),
            invitees: Vec::new(),
            ..meeting()
        },
    ];
    for skipped in skipped {
        let tracker = looked_up(std::slice::from_ref(&skipped), 6);
        assert!(due_ids(&tracker, after_end(6)).is_empty(), "{skipped:?}");
    }

    let organized_with_guests = Meeting {
        your_rsvp: Some(Rsvp::Organizer),
        invitees: vec![invitee("Guest")],
        ..meeting()
    };
    let tracker = looked_up(&[organized_with_guests], 6);
    assert_eq!(due_ids(&tracker, after_end(6)), ["event-42"]);
}

#[test]
fn skips_meetings_already_noted_or_reminded() {
    let tracker = looked_up(&[meeting()], 6);
    let now = after_end(6);

    assert!(
        tracker
            .due(now, DELAY, &ReminderHistory::default(), &[meeting()])
            .is_empty()
    );

    let mut history = ReminderHistory::default();
    history.record(&meeting(), now);
    assert!(tracker.due(now, DELAY, &history, &[]).is_empty());
    assert!(!tracker.needs_lookup(now, DELAY, &history, &[]));
}

#[test]
fn waits_for_a_lookup_after_the_delay_before_judging_the_transcript() {
    let mut tracker = looked_up(&[meeting()], -10);
    let history = ReminderHistory::default();

    assert!(!tracker.needs_lookup(after_end(4), DELAY, &history, &[]));
    assert!(tracker.needs_lookup(after_end(6), DELAY, &history, &[]));
    assert!(due_ids(&tracker, after_end(6)).is_empty());

    let transcribed = Meeting {
        has_transcript: Some(true),
        ..meeting()
    };
    tracker.observe(&[transcribed], after_end(6));
    assert!(!tracker.needs_lookup(after_end(7), DELAY, &history, &[]));
    assert!(due_ids(&tracker, after_end(7)).is_empty());
}

#[test]
fn a_meeting_missing_from_a_later_lookup_is_judged_on_what_was_last_seen() {
    let mut tracker = looked_up(&[meeting()], -10);
    tracker.observe(&[], after_end(6));
    assert_eq!(due_ids(&tracker, after_end(6)), ["event-42"]);
}

#[test]
fn never_reminds_about_meetings_that_ended_hours_ago() {
    let tracker = looked_up(&[meeting()], 3 * 60);
    assert!(due_ids(&tracker, after_end(3 * 60)).is_empty());
    assert!(!tracker.needs_lookup(after_end(3 * 60), DELAY, &ReminderHistory::default(), &[]));

    let tracker = looked_up(&[meeting()], 6);
    assert!(due_ids(&tracker, after_end(3 * 60)).is_empty());
}

#[test]
fn history_survives_a_restart_and_forgets_old_reminders() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("meeting-reminders.json");
    let sent_at = after_end(6);
    let mut history = ReminderHistory::default();
    let key = history.record(&meeting(), sent_at);
    history.save(&path).unwrap();

    let mut loaded = ReminderHistory::load(&path);
    assert!(loaded.contains(&meeting()));
    assert_eq!(loaded.meeting(&key), Some(&meeting()));

    assert!(!loaded.prune(sent_at + TimeDelta::days(6)));
    assert!(loaded.prune(sent_at + TimeDelta::days(8)));
    assert!(!loaded.contains(&meeting()));
}

#[test]
fn unreadable_history_starts_empty() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("meeting-reminders.json");
    assert_eq!(ReminderHistory::load(&path), ReminderHistory::default());
    std::fs::write(&path, "not json").unwrap();
    assert_eq!(ReminderHistory::load(&path), ReminderHistory::default());
}

#[test]
fn reminder_keys_identify_one_occurrence() {
    let key = reminder_key(&meeting());
    assert!(is_reminder_key(&key));
    assert_eq!(key, reminder_key(&meeting()));
    let next_week = Meeting {
        start: meeting().start.map(|start| start + TimeDelta::weeks(1)),
        ..meeting()
    };
    assert_ne!(key, reminder_key(&next_week));
}

#[test]
fn a_time_zone_change_keeps_the_occurrence_and_its_reminder() {
    let utc = chrono::FixedOffset::east_opt(0).unwrap();
    let in_utc = Meeting {
        start: meeting().start.map(|start| start.with_timezone(&utc)),
        end: meeting().end.map(|end| end.with_timezone(&utc)),
        ..meeting()
    };
    assert_eq!(reminder_key(&in_utc), reminder_key(&meeting()));

    let mut history = ReminderHistory::default();
    history.record(&meeting(), after_end(6));
    assert!(history.contains(&in_utc));
}

#[test]
fn entries_saved_with_an_older_key_still_suppress_their_meeting() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("meeting-reminders.json");
    let legacy = serde_json::json!({"reminders": [{
        "key": "ffffffffffffffff",
        "meeting": meeting(),
        "sent_at": after_end(6),
    }]});
    std::fs::write(&path, legacy.to_string()).unwrap();

    let history = ReminderHistory::load(&path);
    assert!(history.contains(&meeting()));
    assert_eq!(history.meeting("ffffffffffffffff"), Some(&meeting()));
    let tracker = looked_up(&[meeting()], 6);
    assert!(tracker.due(after_end(6), DELAY, &history, &[]).is_empty());
}

#[test]
fn a_reminder_counts_as_shown_only_once_delivered_and_failures_are_retried_a_few_times() {
    let tracker = looked_up(&[meeting()], 6);
    let now = after_end(6);
    let mut history = ReminderHistory::default();

    let key = history.begin(&meeting());
    assert!(
        tracker.due(now, DELAY, &history, &[]).is_empty(),
        "in flight"
    );
    assert!(!history.contains(&meeting()));
    assert_eq!(history.meeting(&key), Some(&meeting()));

    history.failed(&key);
    for _ in 1..MAX_DELIVERY_ATTEMPTS {
        assert_eq!(tracker.due(now, DELAY, &history, &[]).len(), 1, "retried");
        let key = history.begin(&meeting());
        history.failed(&key);
    }
    assert!(
        tracker.due(now, DELAY, &history, &[]).is_empty(),
        "given up"
    );
    assert!(!tracker.needs_lookup(now, DELAY, &history, &[]));

    let mut history = ReminderHistory::default();
    let key = history.begin(&meeting());
    history.delivered(&key, now);
    assert!(history.contains(&meeting()));
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("meeting-reminders.json");
    history.begin(&next_meeting());
    history.save(&path).unwrap();
    let loaded = ReminderHistory::load(&path);
    assert!(loaded.contains(&meeting()));
    assert!(
        !loaded.blocks(&next_meeting()),
        "unconfirmed reminders are not saved"
    );
}

fn next_meeting() -> Meeting {
    Meeting {
        id: "event-43".into(),
        ..meeting()
    }
}
#[test]
fn reminder_delay_is_capped() {
    assert_eq!(reminder_delay(5), TimeDelta::minutes(5));
    assert_eq!(
        reminder_delay(u32::MAX),
        TimeDelta::minutes(i64::from(MAX_DELAY_MINUTES))
    );
}

#[test]
fn reminder_text_names_the_meeting_time_and_organizer() {
    let cet = FixedOffset::east_opt(3600).unwrap();
    let text = reminder_text(
        &Meeting {
            organizer: Some("Alex Morgan <alex@example.test>".into()),
            has_transcript: Some(false),
            ..meeting()
        },
        cet,
    );
    assert_eq!(text.title, "Write a quick note for Project review?");
    assert_eq!(text.details, "14:00\u{2013}14:30 \u{b7} Alex Morgan");
    assert!(text.body.contains("wasn't transcribed"));

    let untitled = reminder_text(
        &Meeting {
            title: None,
            organizer: None,
            ..meeting()
        },
        cet,
    );
    assert_eq!(untitled.title, "Write a quick note about your meeting?");
    assert_eq!(untitled.details, "14:00\u{2013}14:30");
}

#[test]
fn reminder_times_are_shown_in_the_users_current_offset() {
    let utc = FixedOffset::east_opt(0).unwrap();
    let stored_in_utc = Meeting {
        organizer: None,
        start: meeting().start.map(|start| start.with_timezone(&utc)),
        end: meeting().end.map(|end| end.with_timezone(&utc)),
        ..meeting()
    };
    let cet = FixedOffset::east_opt(3600).unwrap();
    assert_eq!(
        reminder_text(&stored_in_utc, cet).details,
        "14:00\u{2013}14:30"
    );
    let cest = FixedOffset::east_opt(2 * 3600).unwrap();
    assert_eq!(
        reminder_text(&stored_in_utc, cest).details,
        "15:00\u{2013}15:30"
    );
}
