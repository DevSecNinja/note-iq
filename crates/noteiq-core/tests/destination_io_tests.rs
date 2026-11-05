mod common;

use common::{capture, instant, meeting, raw_capture};
use noteiq_core::{
    destination::{Destination, DestinationError, finish_capture, save_capture},
    diagnostics::FailureClass,
    note_filename::NoteFilename,
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
    assert_eq!(
        raw_capture(&fs::read_to_string(&path).unwrap()),
        "Final title\n\nFinal body\n"
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
    assert_eq!(
        raw_capture(&fs::read_to_string(path).unwrap()),
        "Only once, edited\n"
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
fn late_meeting_discovery_renames_the_note_without_changing_its_timestamp_or_folder() {
    let temp = tempdir().unwrap();
    let other = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let mut capture = capture();
    capture.set_body("Action item");
    let first = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .unwrap();

    capture.auto_connect(meeting());
    let later = instant() + chrono::TimeDelta::days(35);
    let renamed = save_capture(
        &mut capture,
        &Destination::new(other.path().to_path_buf()),
        later,
    )
    .unwrap()
    .unwrap();
    assert_eq!(renamed.parent(), first.parent());
    assert_eq!(
        renamed.file_name().unwrap(),
        "2026-11-03_1415_project-review.md"
    );
    assert!(!first.exists());
    assert_eq!(files_under(temp.path()), vec![renamed.clone()]);
    assert!(files_under(other.path()).is_empty());
    let text = fs::read_to_string(&renamed).unwrap();
    assert!(text.contains(&capture.id.to_string()));
    assert!(text.contains("\ncalendar_event_id: \"event-42\"\n"));
    assert!(text.contains("\ncaptured: 2026-11-03T14:15:00.000+01:00\n"));
    assert!(text.contains("\nupdated: 2026-12-08T14:15:00.000+01:00\n"));
    assert_eq!(raw_capture(&text), "Action item\n");
    assert_eq!(capture.path.as_ref(), Some(&renamed));
}

#[test]
fn switching_meetings_uses_an_unused_filename_and_preserves_other_notes() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let mut other = capture();
    other.set_body("Other note");
    other.connect(meeting());
    let occupied = save_capture(&mut other, &destination, instant())
        .unwrap()
        .unwrap();
    let other_text = fs::read_to_string(&occupied).unwrap();

    let mut capture = capture();
    capture.set_body("My note");
    let mut earlier = meeting();
    earlier.title = Some("Earlier sync".into());
    capture.connect(earlier);
    let first = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .unwrap();
    capture.connect(meeting());
    let renamed = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .unwrap();
    assert_eq!(
        renamed.file_name().unwrap(),
        "2026-11-03_1415_project-review-2.md"
    );
    assert!(!first.exists());
    assert_eq!(fs::read_to_string(occupied).unwrap(), other_text);
    assert_eq!(files_under(temp.path()).len(), 2);
}

#[test]
fn connecting_a_meeting_with_the_existing_filename_does_not_add_a_suffix() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let mut capture = capture();
    capture.set_body("Project review");
    let first = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .unwrap();
    capture.connect(meeting());
    assert_eq!(
        save_capture(&mut capture, &destination, instant())
            .unwrap()
            .unwrap(),
        first
    );
    assert_eq!(files_under(temp.path()), vec![first]);
}

#[test]
fn changing_the_filename_setting_applies_to_new_notes_and_never_renames_saved_ones() {
    let temp = tempdir().unwrap();
    let original = Destination::new(temp.path().to_path_buf());
    let changed = Destination::new(temp.path().to_path_buf())
        .with_note_filename(NoteFilename::parse("{date}-{title}").unwrap());
    let mut capture = capture();
    capture.set_body("Planning");
    let first = save_capture(&mut capture, &original, instant())
        .unwrap()
        .unwrap();
    assert_eq!(first.file_name().unwrap(), "2026-11-03_1415_planning.md");

    capture.set_body("Planning, edited");
    assert_eq!(
        save_capture(&mut capture, &changed, instant())
            .unwrap()
            .unwrap(),
        first
    );
    capture.connect(meeting());
    let renamed = save_capture(&mut capture, &changed, instant())
        .unwrap()
        .unwrap();
    assert_eq!(
        renamed.file_name().unwrap(),
        "2026-11-03_1415_project-review.md",
        "a meeting rename keeps the template the note started with"
    );

    let mut next = common::capture();
    next.set_body("Retro");
    let next_path = save_capture(&mut next, &changed, instant())
        .unwrap()
        .unwrap();
    assert_eq!(next_path.file_name().unwrap(), "2026-11-03-retro.md");
    assert_eq!(next_path.parent(), renamed.parent());
    assert_eq!(files_under(temp.path()), vec![next_path, renamed]);
}

#[test]
fn a_filename_without_a_title_stays_put_when_the_meeting_changes() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf())
        .with_note_filename(NoteFilename::parse("{date}_{time}").unwrap());
    let mut capture = capture();
    capture.set_body("Action item");
    let first = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .unwrap();
    assert_eq!(first.file_name().unwrap(), "2026-11-03_1415.md");

    capture.connect(meeting());
    assert_eq!(
        save_capture(&mut capture, &destination, instant())
            .unwrap()
            .unwrap(),
        first
    );
    capture.disconnect();
    assert_eq!(
        save_capture(&mut capture, &destination, instant())
            .unwrap()
            .unwrap(),
        first
    );
    assert!(
        !fs::read_to_string(&first)
            .unwrap()
            .contains("calendar_event_id"),
        "the disconnected meeting is removed from the note"
    );

    let mut same_minute = common::capture();
    same_minute.set_body("Another item");
    let second = save_capture(&mut same_minute, &destination, instant())
        .unwrap()
        .unwrap();
    assert_eq!(second.file_name().unwrap(), "2026-11-03_1415-2.md");
    assert_eq!(files_under(temp.path()), vec![second, first]);
}

#[test]
fn filename_changes_never_overwrite_an_externally_replaced_note() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let mut capture = capture();
    capture.set_body("Action item");
    let first = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .unwrap();
    fs::write(&first, "An unrelated external file").unwrap();
    capture.connect(meeting());
    assert!(matches!(
        save_capture(&mut capture, &destination, instant()),
        Err(DestinationError::ExternalConflict(_))
    ));
    assert_eq!(capture.path.as_ref(), Some(&first));
    assert_eq!(files_under(temp.path()), vec![first.clone()]);
    assert_eq!(
        fs::read_to_string(first).unwrap(),
        "An unrelated external file"
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
    assert_eq!(
        raw_capture(&fs::read_to_string(path).unwrap()),
        "After deletion\n"
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
fn failed_empty_note_deletion_preserves_its_path_for_the_next_close_attempt() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let mut capture = capture();
    capture.set_body("Mine");
    let path = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    capture.set_body("");

    assert!(matches!(
        finish_capture(&mut capture),
        Err(DestinationError::Io(_))
    ));
    assert_eq!(capture.path.as_deref(), Some(path.as_path()));
    assert!(matches!(
        finish_capture(&mut capture),
        Err(DestinationError::Io(_))
    ));
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
