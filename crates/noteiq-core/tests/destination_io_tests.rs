mod common;

use common::{capture, instant};
use noteiq_core::{
    destination::{Destination, DestinationError, finish_capture, save_capture},
    diagnostics::FailureClass,
};
use std::{
    fs,
    io::{Error, ErrorKind},
    path::{Path, PathBuf},
};
use tempfile::tempdir;
use uuid::Uuid;

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return files;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(files_under(&path));
        } else {
            files.push(path);
        }
    }
    files.sort();
    files
}

#[test]
fn repeated_saves_replace_the_note_in_place_without_temporary_leftovers() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let mut capture = capture();
    let mut path = None;

    for body in ["Draft one", "Draft two", "Final title\n\nFinal body"] {
        capture.set_body(body);
        let saved = save_capture(&mut capture, &destination, instant())
            .unwrap()
            .unwrap();
        assert_eq!(path.get_or_insert_with(|| saved.clone()), &saved);
    }

    let path = path.unwrap();
    assert_eq!(files_under(temp.path()), vec![path.clone()]);
    assert!(
        fs::read_to_string(&path)
            .unwrap()
            .ends_with("---\nFinal title\n\nFinal body\n")
    );
}

#[test]
fn a_note_is_written_to_exactly_one_destination_even_after_settings_change() {
    let vault = tempdir().unwrap();
    let onedrive = tempdir().unwrap();
    let mut capture = capture();
    capture.set_body("Only once");
    let path = save_capture(
        &mut capture,
        &Destination::new(vault.path().to_path_buf()),
        instant(),
    )
    .unwrap()
    .unwrap();

    capture.set_body("Only once, edited");
    let updated = save_capture(
        &mut capture,
        &Destination::new(onedrive.path().to_path_buf()),
        instant(),
    )
    .unwrap()
    .unwrap();

    assert_eq!(updated, path);
    assert!(path.starts_with(vault.path()));
    assert_eq!(files_under(vault.path()), vec![path.clone()]);
    assert!(files_under(onedrive.path()).is_empty());
    assert!(
        fs::read_to_string(path)
            .unwrap()
            .ends_with("Only once, edited\n")
    );
}

#[test]
fn notes_with_the_same_minute_and_title_get_distinct_files() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let mut first = capture();
    let mut second = capture();
    first.set_body("Standup");
    second.set_body("Standup");

    let first_path = save_capture(&mut first, &destination, instant())
        .unwrap()
        .unwrap();
    let second_path = save_capture(&mut second, &destination, instant())
        .unwrap()
        .unwrap();

    assert_ne!(first_path, second_path);
    assert_eq!(
        second_path.file_name().unwrap(),
        "2026-11-03_1415_standup-2.md"
    );
    assert!(
        fs::read_to_string(&first_path)
            .unwrap()
            .contains(&first.id.to_string())
    );
    assert!(
        fs::read_to_string(&second_path)
            .unwrap()
            .contains(&second.id.to_string())
    );
}

#[test]
fn a_destination_that_is_a_file_is_reported_as_unavailable() {
    let temp = tempdir().unwrap();
    let not_a_folder = temp.path().join("notes.txt");
    fs::write(&not_a_folder, "existing").unwrap();
    let mut capture = capture();
    capture.set_body("Keep this draft");

    let error = save_capture(&mut capture, &Destination::new(not_a_folder), instant()).unwrap_err();

    assert!(matches!(error, DestinationError::RootUnavailable(_)));
    assert_eq!(error.failure_class(), FailureClass::DestinationUnavailable);
    assert!(capture.path.is_none());
    assert_eq!(capture.body, "Keep this draft");
}

#[test]
fn folder_creation_failure_preserves_the_draft_and_writes_nothing() {
    let temp = tempdir().unwrap();
    let blocker = temp.path().join("captures");
    fs::write(&blocker, "a file where the captures folder should be").unwrap();
    let mut capture = capture();
    capture.set_body("Keep this draft");

    let error = save_capture(
        &mut capture,
        &Destination::new(temp.path().to_path_buf()),
        instant(),
    )
    .unwrap_err();

    assert!(matches!(error, DestinationError::Io(_)));
    assert!(capture.path.is_none());
    assert_eq!(capture.body, "Keep this draft");
    assert_eq!(files_under(temp.path()), vec![blocker]);
}

#[test]
fn an_externally_deleted_note_is_recreated_at_the_same_path() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let mut capture = capture();
    capture.set_body("Before deletion");
    let path = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .unwrap();
    fs::remove_file(&path).unwrap();

    capture.set_body("After deletion");
    let recreated = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .unwrap();

    assert_eq!(recreated, path);
    assert!(
        fs::read_to_string(path)
            .unwrap()
            .ends_with("After deletion\n")
    );
}

#[test]
fn closing_an_empty_note_never_deletes_a_file_owned_by_another_note() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let mut capture = capture();
    capture.set_body("Mine");
    let path = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .unwrap();
    let replaced = fs::read_to_string(&path)
        .unwrap()
        .replace(&capture.id.to_string(), &Uuid::new_v4().to_string());
    fs::write(&path, &replaced).unwrap();

    capture.set_body("");
    let error = finish_capture(&mut capture).unwrap_err();

    assert!(matches!(error, DestinationError::ExternalConflict(_)));
    assert_eq!(fs::read_to_string(&path).unwrap(), replaced);
    assert_eq!(capture.path.as_deref(), Some(path.as_path()));
}

#[test]
fn write_errors_are_classified_for_friendly_guidance_and_diagnostics() {
    let cases = [
        (
            DestinationError::RootUnavailable(PathBuf::from("missing")),
            FailureClass::DestinationUnavailable,
        ),
        (
            DestinationError::ExternalConflict(PathBuf::from("note.md")),
            FailureClass::ExternalConflict,
        ),
        (
            DestinationError::Io(Error::from(ErrorKind::PermissionDenied)),
            FailureClass::PermissionDenied,
        ),
        (
            DestinationError::Io(Error::from(ErrorKind::Other)),
            FailureClass::Unknown,
        ),
        (DestinationError::NoAvailableName, FailureClass::Unknown),
    ];

    for (error, expected) in cases {
        assert_eq!(error.failure_class(), expected, "{error}");
    }
}
