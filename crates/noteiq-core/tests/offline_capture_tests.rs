mod common;

use common::{capture, instant};
use noteiq_core::{
    destination::{Destination, save_capture},
    workiq::{WorkiqClient, WorkiqError},
};
use tempfile::tempdir;

#[test]
fn note_capture_saves_without_a_workiq_executable() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let mut capture = capture();
    capture.set_body("Offline note");

    assert!(matches!(
        WorkiqClient::new(temp.path().join("missing-workiq.exe")).discover_meetings(instant()),
        Err(WorkiqError::ExecutableMissing)
    ));
    let saved = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .expect("note saving is independent from WorkIQ");

    assert!(saved.exists());
    assert!(
        std::fs::read_to_string(saved)
            .unwrap()
            .contains("Offline note")
    );
}
