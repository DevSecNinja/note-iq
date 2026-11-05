mod common;

use common::{capture, instant, meeting, raw_capture};
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
    assert!(connected.contains("\nmeeting: \"Project review\"\n"));
    assert!(connected.contains("\ncalendar_event_id: \"event-42\"\n"));
    assert!(connected.contains("\nrequired_invitees:\n  - \"Invitee\"\n"));
    assert!(connected.contains("\n## Meeting details\n"));
    assert!(connected.contains("- **Required invitees:**\n  - Invitee\n"));

    capture.disconnect();
    let disconnected_path = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .unwrap();

    assert!(
        !path.exists(),
        "the old meeting filename must not be left behind"
    );
    assert_eq!(
        disconnected_path.file_name().unwrap(),
        "2026-11-03_1415_decision-keep-the-launch-date.md"
    );
    let disconnected = std::fs::read_to_string(disconnected_path).unwrap();
    assert!(!disconnected.contains("\nmeeting:"));
    assert!(!disconnected.contains("meeting_start:"));
    assert!(!disconnected.contains("organizer:"));
    assert!(!disconnected.contains("attendees:"));
    assert!(!disconnected.contains("invitees:"));
    assert!(!disconnected.contains("calendar_event_id:"));
    assert!(!disconnected.contains("## Meeting details"));
    assert!(!disconnected.contains("Project review"));
    assert!(disconnected.contains("\n# Decision: keep the launch date.\n"));
    assert_eq!(
        raw_capture(&disconnected),
        "Decision: keep the launch date.\n"
    );
    assert_eq!(id_line(&connected), id_line(&disconnected));
}

fn id_line(note: &str) -> &str {
    note.lines()
        .find(|line| line.starts_with("id: "))
        .expect("note has an id property")
}
