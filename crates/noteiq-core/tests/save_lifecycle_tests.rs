mod common;

use common::{capture, instant};
use noteiq_core::destination::{Destination, DestinationError, save_capture};
use tempfile::tempdir;

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
