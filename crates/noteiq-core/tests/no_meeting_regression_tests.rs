mod common;

use common::{capture, instant, meeting};
use noteiq_core::destination::{Destination, save_capture};
use tempfile::tempdir;

#[test]
fn choosing_no_meeting_updates_saved_markdown_without_changing_note_text() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let mut capture = capture();
    capture.set_body("Decision: keep the launch date.");
    capture.connect(meeting());
    let path = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .expect("initial meeting note is saved");
    let connected = std::fs::read_to_string(&path).unwrap();
    assert!(connected.contains("meeting:\n  id: \"event-42\""));
    assert!(connected.contains("attendees:\n    - \"Attendee\""));

    capture.disconnect();
    save_capture(&mut capture, &destination, instant()).unwrap();

    let disconnected = std::fs::read_to_string(path).unwrap();
    assert!(!disconnected.contains("\nmeeting:"));
    assert!(!disconnected.contains("attendees:"));
    assert!(disconnected.contains("Decision: keep the launch date."));
    assert_eq!(connected.lines().nth(2), disconnected.lines().nth(2));
}
