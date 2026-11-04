mod common;

use common::{capture, instant, meeting};
use noteiq_core::destination::{Destination, capture_path};
use noteiq_core::note_filename::NoteFilename;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn creates_capture_paths_in_selected_year_month_with_a_safe_slug() {
    let temp = tempdir().unwrap();
    let mut capture = capture();
    capture.set_body("Design review: launch/notes *today*");
    let destination = Destination::new(temp.path().to_path_buf());

    let path = capture_path(&capture, &destination, instant(), 1);
    let relative = path.strip_prefix(temp.path()).unwrap();

    let expected = Path::new("captures")
        .join("2026")
        .join("11")
        .join("2026-11-03_1415_design-review-launch-notes-today.md");
    assert_eq!(relative, expected);
}

#[test]
fn collision_suffix_is_stable_and_does_not_replace_an_existing_note() {
    let temp = tempdir().unwrap();
    let mut capture = capture();
    capture.set_body("Planning");
    let destination = Destination::new(temp.path().to_path_buf());
    let first = capture_path(&capture, &destination, instant(), 1);
    let second = capture_path(&capture, &destination, instant(), 2);

    assert_eq!(first.file_name().unwrap(), "2026-11-03_1415_planning.md");
    assert_eq!(second.file_name().unwrap(), "2026-11-03_1415_planning-2.md");
}

#[test]
fn meeting_title_takes_priority_and_is_sanitized_like_note_text() {
    let mut capture = capture();
    capture.set_body("Unrelated action item");
    let mut event = meeting();
    event.title = Some("Design review: launch/notes *today*".into());
    capture.connect(event);
    let destination = Destination::new("notes".into());
    assert_eq!(
        capture_path(&capture, &destination, instant(), 1)
            .file_name()
            .unwrap(),
        "2026-11-03_1415_design-review-launch-notes-today.md"
    );
}

#[test]
fn missing_or_unusable_meeting_names_fall_back_to_note_text() {
    for title in [None, Some(""), Some(" \n "), Some("///???")] {
        let mut capture = capture();
        capture.set_body("Action item");
        let mut event = meeting();
        event.title = title.map(str::to_owned);
        capture.connect(event);
        assert_eq!(
            capture_path(&capture, &Destination::new("notes".into()), instant(), 1)
                .file_name()
                .unwrap(),
            "2026-11-03_1415_action-item.md"
        );
    }
}

#[test]
fn the_filename_setting_names_new_notes_in_the_same_folder() {
    let mut capture = capture();
    capture.set_body("Planning");
    let destination = Destination::new("notes".into())
        .with_note_filename(NoteFilename::parse("{date}-{title}").unwrap());

    let path = capture_path(&capture, &destination, instant(), 1);

    assert_eq!(
        path,
        Path::new("notes")
            .join("captures")
            .join("2026")
            .join("11")
            .join("2026-11-03-planning.md")
    );
}
