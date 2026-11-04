mod common;

use common::{capture, instant, meeting};
use noteiq_core::capture::CaptureController;
use noteiq_core::destination::{Destination, DestinationError, save_capture};
use std::time::{Duration, Instant};
use tempfile::tempdir;

#[test]
fn meeting_changes_wait_for_the_typing_pause_and_never_save_an_empty_draft() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let now = Instant::now();
    let mut controller = CaptureController::new(capture());

    controller.connect_meeting(meeting(), now);
    controller.disconnect_meeting(now);
    assert!(
        !controller.is_dirty(),
        "an empty draft that was never saved has nothing to write"
    );

    controller.edit("Decision", now);
    controller.save(&destination, instant()).unwrap().unwrap();
    controller.connect_meeting(meeting(), now);
    assert!(controller.is_dirty());
    assert!(
        !controller.autosave_due(now + Duration::from_millis(100)),
        "moving through the picker doesn't rewrite the file at every step"
    );
    assert!(controller.autosave_due(now + Duration::from_secs(1)));

    controller.edit("", now);
    controller.save(&destination, instant()).unwrap().unwrap();
    controller.disconnect_meeting(now);
    assert!(
        controller.is_dirty(),
        "a saved note records the change even after its text was erased"
    );
}

#[test]
fn a_failed_save_pauses_autosave_retries_and_keeps_the_draft_until_the_destination_returns() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("notes");
    let destination = Destination::new(root.clone());
    let mut controller = CaptureController::new(capture());
    controller.edit("Keep this draft", Instant::now());

    assert!(controller.save(&destination, instant()).is_err());
    let after_failure = Instant::now();

    assert!(controller.is_dirty());
    assert_eq!(controller.capture.body, "Keep this draft");
    assert!(
        !controller.autosave_due(after_failure + Duration::from_secs(1)),
        "a broken destination is not retried on every timer tick"
    );
    assert!(controller.autosave_due(after_failure + Duration::from_secs(6)));

    std::fs::create_dir(&root).unwrap();
    let path = controller.save(&destination, instant()).unwrap().unwrap();
    assert!(path.exists(), "an explicit save is never throttled");
    assert!(!controller.is_dirty());
}

#[test]
fn external_replacement_is_reported_without_overwriting_or_retargeting_the_note() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let mut capture = capture();
    capture.set_body("Original content");
    let path = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .unwrap();
    let replacement = std::fs::read_to_string(&path)
        .unwrap()
        .replace(&capture.id.to_string(), "some-other-note");
    std::fs::write(&path, &replacement).unwrap();
    capture.set_body("New content");

    let error = save_capture(&mut capture, &destination, instant()).unwrap_err();

    assert!(matches!(error, DestinationError::ExternalConflict(_)));
    assert_eq!(capture.path.as_deref(), Some(path.as_path()));
    assert_eq!(std::fs::read_to_string(path).unwrap(), replacement);
    assert_eq!(capture.body, "New content");
}

#[test]
fn unavailable_destination_preserves_the_unsaved_body_and_does_not_create_a_note() {
    let temp = tempdir().unwrap();
    let missing_root = temp.path().join("not-created");
    let destination = Destination::new(missing_root.clone());
    let mut capture = capture();
    capture.set_body("Keep this draft");

    let error = save_capture(&mut capture, &destination, instant()).unwrap_err();

    assert!(matches!(error, DestinationError::RootUnavailable(_)));
    assert_eq!(capture.body, "Keep this draft");
    assert!(capture.path.is_none());
    assert!(!missing_root.exists());
}
