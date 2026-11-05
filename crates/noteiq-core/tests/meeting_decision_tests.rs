mod common;

use chrono::DateTime;
use common::{capture, meeting};
use noteiq_core::meeting::{MeetingSelection, apply_automatic_selection};
use noteiq_core::model::{Meeting, MeetingDecision};
use std::path::PathBuf;

/// The meeting after [`meeting`], which starts 15 minutes after the capture was created.
fn next_meeting() -> Meeting {
    let mut next = meeting();
    next.id = "event-43".to_owned();
    next.title = Some("Budget review".to_owned());
    next.start = Some(DateTime::parse_from_rfc3339("2026-11-03T14:30:00+01:00").unwrap());
    next.end = Some(DateTime::parse_from_rfc3339("2026-11-03T15:00:00+01:00").unwrap());
    next
}

fn auto_connected(meeting: Meeting) -> MeetingSelection {
    MeetingSelection::Connected(meeting)
}

#[test]
fn an_empty_unsaved_note_moves_on_to_the_next_meeting_in_progress() {
    let mut capture = capture();
    apply_automatic_selection(&mut capture, auto_connected(meeting()));
    assert_eq!(capture.meeting_decision, MeetingDecision::Automatic);

    apply_automatic_selection(&mut capture, auto_connected(next_meeting()));

    assert_eq!(capture.meeting, Some(next_meeting()));
    assert_eq!(capture.meeting_decision, MeetingDecision::Automatic);
}

#[test]
fn an_automatic_meeting_stays_once_the_note_has_text() {
    let mut capture = capture();
    apply_automatic_selection(&mut capture, auto_connected(meeting()));
    capture.set_body("Decision: ship on Friday");

    apply_automatic_selection(&mut capture, auto_connected(next_meeting()));

    assert_eq!(capture.meeting, Some(meeting()));
    assert_eq!(capture.meeting_decision, MeetingDecision::Automatic);
}

#[test]
fn an_automatic_meeting_stays_on_a_saved_note_whose_text_was_erased() {
    let mut capture = capture();
    apply_automatic_selection(&mut capture, auto_connected(meeting()));
    capture.path = Some(PathBuf::from("2026-11-03_1415_project-review.md"));

    apply_automatic_selection(&mut capture, auto_connected(next_meeting()));

    assert_eq!(capture.meeting, Some(meeting()));
}

#[test]
fn a_later_lookup_refreshes_the_details_of_the_same_meeting() {
    let mut capture = capture();
    capture.set_body("Notes");
    apply_automatic_selection(&mut capture, auto_connected(meeting()));
    let mut moved = meeting();
    moved.title = Some("Project review (moved)".to_owned());
    moved.end = Some(DateTime::parse_from_rfc3339("2026-11-03T14:45:00+01:00").unwrap());

    apply_automatic_selection(&mut capture, auto_connected(moved.clone()));

    assert_eq!(capture.meeting, Some(moved));
    assert_eq!(capture.meeting_decision, MeetingDecision::Automatic);
}

#[test]
fn text_written_well_before_a_meeting_is_not_filed_under_it() {
    let mut later = next_meeting();
    later.start = Some(DateTime::parse_from_rfc3339("2026-11-03T14:31:00+01:00").unwrap());

    let mut written = capture();
    written.set_body("Thought from before the meeting");
    apply_automatic_selection(&mut written, auto_connected(later.clone()));
    assert_eq!(written.meeting, None);
    assert_eq!(written.meeting_decision, MeetingDecision::Pending);

    let mut empty = capture();
    apply_automatic_selection(&mut empty, auto_connected(later.clone()));
    assert_eq!(empty.meeting, Some(later));
}

#[test]
fn lookups_without_a_single_meeting_leave_the_note_unchanged() {
    for selection in [
        MeetingSelection::NoMeeting,
        MeetingSelection::NeedsChoice(vec![meeting(), next_meeting()]),
    ] {
        let mut capture = capture();
        apply_automatic_selection(&mut capture, auto_connected(meeting()));

        apply_automatic_selection(&mut capture, selection);

        assert_eq!(capture.meeting, Some(meeting()));
        assert_eq!(capture.meeting_decision, MeetingDecision::Automatic);
    }
}

#[test]
fn explicit_disconnect_wins_over_a_delayed_automatic_lookup() {
    let mut capture = capture();
    capture.disconnect();

    apply_automatic_selection(&mut capture, MeetingSelection::Connected(meeting()));

    assert!(capture.meeting.is_none());
    assert_eq!(
        capture.meeting_decision,
        noteiq_core::model::MeetingDecision::Disconnected
    );
}

#[test]
fn explicit_meeting_selection_wins_over_a_delayed_lookup() {
    let mut capture = capture();
    let selected = meeting();
    capture.connect(selected.clone());

    let mut delayed = meeting();
    delayed.id = "different-event".to_owned();
    apply_automatic_selection(&mut capture, MeetingSelection::Connected(delayed));

    assert_eq!(capture.meeting, Some(selected));
    assert_eq!(
        capture.meeting_decision,
        noteiq_core::model::MeetingDecision::Selected
    );
}
