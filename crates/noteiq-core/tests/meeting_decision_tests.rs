mod common;

use common::{capture, meeting};
use noteiq_core::meeting::{MeetingSelection, apply_automatic_selection};

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
