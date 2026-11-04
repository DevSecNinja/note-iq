mod common;

use common::{capture, instant};
use noteiq_core::destination::{Destination, capture_path};
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
