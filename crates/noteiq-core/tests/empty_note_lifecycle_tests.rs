mod common;

use common::{capture, instant, raw_capture};
use noteiq_core::destination::{Destination, finish_capture, save_capture};
use tempfile::tempdir;

#[test]
fn clearing_saved_text_keeps_file_during_editing_and_deletes_on_close() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let mut capture = capture();
    capture.set_body("A note.");
    let path = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .expect("non-empty note is saved");
    assert!(path.exists());

    capture.set_body(" \n\t ");
    save_capture(&mut capture, &destination, instant()).unwrap();

    assert!(
        path.exists(),
        "autosave must not delete an actively edited note"
    );
    let cleared = std::fs::read_to_string(&path).unwrap();
    assert!(
        cleared.starts_with("---\nkind: capture\n"),
        "cleared text remains represented by a valid Markdown file"
    );
    assert!(raw_capture(&cleared).trim().is_empty());

    finish_capture(&mut capture).unwrap();
    assert!(!path.exists(), "closing an empty capture deletes its file");
}

#[test]
fn empty_unsaved_capture_never_creates_a_file() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let mut capture = capture();

    assert_eq!(
        save_capture(&mut capture, &destination, instant()).unwrap(),
        None
    );
    finish_capture(&mut capture).unwrap();

    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}
