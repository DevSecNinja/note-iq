mod common;

use common::{instant, meeting};
use noteiq_core::meeting::{MeetingSelection, select_current_meeting};
use noteiq_core::model::Meeting;

fn second_meeting() -> Meeting {
    let mut other = meeting();
    other.id = "event-43".to_owned();
    other.title = Some("Budget review".to_owned());
    other
}

#[test]
fn auto_connects_only_when_exactly_one_current_meeting_is_eligible() {
    let selected = select_current_meeting(&[meeting()], instant(), true);
    assert!(matches!(selected, MeetingSelection::Connected(_)));

    let overlap = select_current_meeting(&[meeting(), second_meeting()], instant(), true);
    assert!(matches!(overlap, MeetingSelection::NeedsChoice(events) if events.len() == 2));
}

#[test]
fn distinguishes_no_match_and_disabled_auto_connect() {
    assert_eq!(
        select_current_meeting(&[], instant(), true),
        MeetingSelection::NoMeeting
    );
    assert!(matches!(
        select_current_meeting(&[meeting()], instant(), false),
        MeetingSelection::NeedsChoice(events) if events.len() == 1
    ));
}

#[test]
fn ignores_cancelled_declined_and_out_of_interval_events() {
    let mut cancelled = meeting();
    cancelled.cancelled = true;
    let mut declined = meeting();
    declined.declined = true;
    let mut ended = meeting();
    ended.end = Some(chrono::DateTime::parse_from_rfc3339("2026-11-03T14:15:00+01:00").unwrap());

    assert_eq!(
        select_current_meeting(&[cancelled, declined, ended], instant(), true),
        MeetingSelection::NoMeeting
    );
}
