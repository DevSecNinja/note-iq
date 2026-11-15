use chrono::{DateTime, FixedOffset, TimeDelta};
use noteiq_core::{
    destination::{Destination, DestinationError, finish_capture, save_capture},
    model::{Capture, CaptureSource, Meeting},
    saved_note::find_meeting_note,
};
use std::fs;
use tempfile::tempdir;

fn now() -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339("2026-11-06T09:00:00+01:00").unwrap()
}

fn meeting(id: &str, at: DateTime<FixedOffset>) -> Meeting {
    Meeting {
        id: id.into(),
        title: Some("Weekly planning".into()),
        start: Some(at),
        end: Some(at + TimeDelta::minutes(30)),
        organizer: None,
        invitees: vec![],
        your_rsvp: None,
        entity_url: None,
        cancelled: false,
        declined: false,
        all_day: false,
        has_transcript: None,
    }
}

fn save(destination: &Destination, meeting: Meeting, body: &str) -> Capture {
    let mut capture = Capture::new(CaptureSource::Cli, now(), "Europe/Amsterdam".into());
    capture.connect(meeting);
    capture.author = Some("Example User".into());
    capture.tags = vec!["project".into()];
    capture.set_body(body);
    save_capture(&mut capture, destination, now()).unwrap();
    capture
}

#[test]
fn restores_identity_metadata_and_saves_to_the_original_path() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    let original = save(&destination, meeting("event", now()), "Original text");
    let mut restored = find_meeting_note(&destination, original.meeting.as_ref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(restored.id, original.id);
    assert_eq!(restored.created_at, original.created_at);
    assert_eq!(restored.author, original.author);
    assert_eq!(restored.tags, ["capture", "note", "project"]);
    assert_eq!(restored.path, original.path);
    assert_eq!(restored.body, "Original text\n");
    restored.set_body("Continued text");
    save_capture(&mut restored, &destination, now()).unwrap();
    assert_eq!(restored.path, original.path);
    assert!(
        fs::read_to_string(restored.path.unwrap())
            .unwrap()
            .ends_with("Continued text\n")
    );
}

#[test]
fn shared_series_ids_and_titles_do_not_mix_occurrences() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    let first = meeting("series", now());
    let second = meeting("series", now() + TimeDelta::days(7));
    let a = save(&destination, first.clone(), "First occurrence");
    let b = save(&destination, second.clone(), "Second occurrence");
    assert_eq!(
        find_meeting_note(&destination, &first).unwrap().unwrap().id,
        a.id
    );
    assert_eq!(
        find_meeting_note(&destination, &second)
            .unwrap()
            .unwrap()
            .id,
        b.id
    );
    assert!(
        find_meeting_note(&destination, &meeting("other", now()))
            .unwrap()
            .is_none()
    );
    let mut equivalent = first;
    equivalent.start = Some(
        equivalent
            .start
            .unwrap()
            .with_timezone(&FixedOffset::east_opt(0).unwrap()),
    );
    assert_eq!(
        find_meeting_note(&destination, &equivalent)
            .unwrap()
            .unwrap()
            .id,
        a.id
    );
}

#[test]
fn latest_updated_duplicate_wins_without_deleting_other_captures() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    let event = meeting("event", now());
    let a = save(&destination, event.clone(), "Older");
    let mut b = save(&destination, event.clone(), "Newer");
    save_capture(&mut b, &destination, now() + TimeDelta::minutes(1)).unwrap();
    assert_eq!(
        find_meeting_note(&destination, &event).unwrap().unwrap().id,
        b.id
    );
    assert!(a.path.unwrap().exists());
}

#[test]
fn preserves_refinements_custom_properties_and_crlf_when_continuing() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    let original = save(&destination, meeting("event", now()), "Original");
    let path = original.path.unwrap();
    let document = fs::read_to_string(&path)
        .unwrap()
        .replace(
            "processing: raw",
            "processing: refined\ncustom:\n  nested: true",
        )
        .replace(
            "## Summary\n\n",
            "## Summary\n\nImportant refined summary.\n\n",
        )
        .replace("## Follow-ups\n\n", "## Follow-ups\n\n- [ ] Follow up\n\n")
        .replace('\n', "\r\n");
    fs::write(&path, &document).unwrap();
    let mut restored = find_meeting_note(&destination, original.meeting.as_ref().unwrap())
        .unwrap()
        .unwrap();
    restored.set_body("Original\r\nContinued");
    save_capture(&mut restored, &destination, now()).unwrap();
    let updated = fs::read_to_string(&path).unwrap();
    assert_eq!(
        updated.split("## Notes").next(),
        document.split("## Notes").next()
    );
    assert!(updated.ends_with("Original\r\nContinued\r\n"));
}

#[test]
fn disconnect_removes_meeting_context_without_erasing_refinements_or_renaming() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    let original = save(&destination, meeting("event", now()), "Text");
    let path = original.path.clone().unwrap();
    let document = fs::read_to_string(&path)
        .unwrap()
        .replace("## Summary\n\n", "## Summary\n\nKeep this.\n\n");
    fs::write(&path, document).unwrap();
    let mut restored = find_meeting_note(&destination, original.meeting.as_ref().unwrap())
        .unwrap()
        .unwrap();
    restored.disconnect();
    save_capture(&mut restored, &destination, now()).unwrap();
    assert_eq!(restored.path, Some(path.clone()));
    let updated = fs::read_to_string(path).unwrap();
    assert!(!updated.contains("\nmeeting:"));
    assert!(!updated.contains("calendar_event_id:"));
    assert!(!updated.contains("## Meeting details"));
    assert!(updated.contains("Keep this."));
    assert!(updated.ends_with("Text\n"));
}

#[test]
fn external_edits_after_reopening_block_save_and_cleanup() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    let original = save(&destination, meeting("event", now()), "Text");
    let path = original.path.unwrap();
    let mut restored = find_meeting_note(&destination, original.meeting.as_ref().unwrap())
        .unwrap()
        .unwrap();
    let external = format!("{}External edit\n", fs::read_to_string(&path).unwrap());
    fs::write(&path, &external).unwrap();
    restored.set_body("Changed");
    assert!(matches!(
        save_capture(&mut restored, &destination, now()),
        Err(DestinationError::ExternalConflict(_))
    ));
    restored.set_body("");
    assert!(matches!(
        finish_capture(&mut restored),
        Err(DestinationError::ExternalConflict(_))
    ));
    assert_eq!(fs::read_to_string(path).unwrap(), external);
}

#[test]
fn empty_reopened_notes_preserve_the_document_and_missing_files_conflict() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    let original = save(&destination, meeting("event", now()), "Text");
    let path = original.path.unwrap();
    let mut restored = find_meeting_note(&destination, original.meeting.as_ref().unwrap())
        .unwrap()
        .unwrap();
    restored.set_body("");
    save_capture(&mut restored, &destination, now()).unwrap();
    finish_capture(&mut restored).unwrap();
    assert!(path.exists());
    fs::remove_file(&path).unwrap();
    assert!(matches!(
        save_capture(&mut restored, &destination, now()),
        Err(DestinationError::ExternalConflict(_))
    ));
}

#[test]
fn missing_history_is_not_an_error_but_unavailable_destination_is() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    assert!(
        find_meeting_note(&destination, &meeting("event", now()))
            .unwrap()
            .is_none()
    );
    let missing = Destination::new(vault.path().join("missing"));
    assert!(find_meeting_note(&missing, &meeting("event", now())).is_err());
}

#[test]
fn unsupported_matching_layout_reports_an_error_instead_of_creating_a_duplicate() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    let original = save(&destination, meeting("event", now()), "Text");
    let path = original.path.unwrap();
    let document = fs::read_to_string(&path)
        .unwrap()
        .replace("noteiq_schema: 4", "noteiq_schema: 3");
    fs::write(path, document).unwrap();
    assert!(find_meeting_note(&destination, original.meeting.as_ref().unwrap()).is_err());
}

#[test]
fn bom_fenced_headings_and_custom_yaml_heading_text_do_not_confuse_the_notes_boundary() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    let original = save(
        &destination,
        meeting("event", now()),
        "```markdown\n## Notes\n```\nReal text",
    );
    let path = original.path.as_ref().unwrap();
    let document = format!(
        "\u{feff}{}",
        fs::read_to_string(path).unwrap().replace(
            "processing: raw",
            "processing: refined\ncustom: |\n  ## Notes",
        )
    );
    fs::write(path, &document).unwrap();
    let mut restored = find_meeting_note(&destination, original.meeting.as_ref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(restored.body, "```markdown\n## Notes\n```\nReal text\n");
    restored.set_body("```markdown\n## Notes\n```\nContinued");
    save_capture(&mut restored, &destination, now()).unwrap();
    assert!(fs::read_to_string(path).unwrap().starts_with('\u{feff}'));
    assert_eq!(
        find_meeting_note(&destination, original.meeting.as_ref().unwrap())
            .unwrap()
            .unwrap()
            .body,
        "```markdown\n## Notes\n```\nContinued\n"
    );
}

#[test]
fn duplicate_notes_headings_and_malformed_occurrence_dates_are_rejected() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    let original = save(&destination, meeting("event", now()), "Text");
    let path = original.path.as_ref().unwrap();
    let document = fs::read_to_string(path).unwrap();
    fs::write(path, format!("{document}\n## Notes\nAmbiguous\n")).unwrap();
    assert!(find_meeting_note(&destination, original.meeting.as_ref().unwrap()).is_err());
    fs::write(
        path,
        document.replace(
            "meeting_start: 2026-11-06T09:00:00+01:00",
            "meeting_start: invalid",
        ),
    )
    .unwrap();
    assert!(find_meeting_note(&destination, original.meeting.as_ref().unwrap()).is_err());
}

#[test]
fn absent_start_matches_only_an_equally_absent_occurrence_start() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    let mut event = meeting("event", now());
    event.start = None;
    let original = save(&destination, event.clone(), "Undated");
    assert_eq!(
        find_meeting_note(&destination, &event).unwrap().unwrap().id,
        original.id
    );
    assert!(
        find_meeting_note(&destination, &meeting("event", now()))
            .unwrap()
            .is_none()
    );
    event.id.clear();
    assert!(find_meeting_note(&destination, &event).unwrap().is_none());
}

#[test]
fn disconnect_preserves_quoted_custom_keys_and_comments_following_managed_metadata() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    let original = save(&destination, meeting("event", now()), "Text");
    let path = original.path.as_ref().unwrap();
    let document = fs::read_to_string(path)
        .unwrap()
        .replace(
            "processing: raw",
            "processing: refined\ncustom: |\n  ## Meeting details\n  Keep yaml",
        )
        .replace("calendar_event_id:", "'calendar_event_id':")
        .replace(
            "id: \"",
            "# Preserve this comment\n\"custom:key\": keep\nid: \"",
        )
        .replace("timezone:", "# Preserve updated comment\ntimezone:");
    fs::write(path, document).unwrap();
    let mut restored = find_meeting_note(&destination, original.meeting.as_ref().unwrap())
        .unwrap()
        .unwrap();
    restored.disconnect();
    save_capture(&mut restored, &destination, now()).unwrap();
    let text = fs::read_to_string(path).unwrap();
    assert!(text.contains("\"custom:key\": keep"));
    assert!(text.contains("# Preserve this comment"));
    assert!(text.contains("# Preserve updated comment"));
    assert!(!text.contains("calendar_event_id"));
    assert!(text.contains("custom: |\n  ## Meeting details\n  Keep yaml"));
    assert!(!text.contains("\n## Meeting details"));
    save_capture(&mut restored, &destination, now() + TimeDelta::minutes(1)).unwrap();
    assert_eq!(
        fs::read_to_string(path)
            .unwrap()
            .matches("# Preserve updated comment")
            .count(),
        1
    );
}

#[test]
fn repeated_saves_with_mixed_newlines_keep_the_notes_boundary_and_author_changes() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    let original = save(&destination, meeting("event", now()), "Text");
    let path = original.path.as_ref().unwrap();
    let mut restored = find_meeting_note(&destination, original.meeting.as_ref().unwrap())
        .unwrap()
        .unwrap();
    restored.set_body("Mixed\r\nNewline");
    restored.author = Some("Updated User".into());
    save_capture(&mut restored, &destination, now()).unwrap();
    restored.set_body("Second save");
    save_capture(&mut restored, &destination, now() + TimeDelta::minutes(1)).unwrap();
    let text = fs::read_to_string(path).unwrap();
    assert!(text.contains("from: \"[[Updated User]]\""));
    assert!(text.contains("**From:** [[Updated User]]"));
    assert!(!text.contains("Example User"));
    assert!(text.ends_with("## Notes\n\nSecond save\r\n"));
    restored.author = None;
    save_capture(&mut restored, &destination, now() + TimeDelta::minutes(2)).unwrap();
    let text = fs::read_to_string(path).unwrap();
    assert!(!text.contains("\nfrom:"));
    assert!(!text.contains("**From:**"));
    restored.author = Some("Third User".into());
    restored.disconnect();
    save_capture(&mut restored, &destination, now() + TimeDelta::minutes(3)).unwrap();
    let text = fs::read_to_string(path).unwrap();
    assert_eq!(text.matches("\nfrom:").count(), 1);
    assert!(text.contains("**From:** [[Third User]]"));
}

#[test]
fn restored_uuid_quoting_does_not_block_saving_or_finishing() {
    for quote in ["", "'"] {
        let vault = tempdir().unwrap();
        let destination = Destination::new(vault.path().into());
        let original = save(&destination, meeting("event", now()), "Text");
        let path = original.path.as_ref().unwrap();
        let text = fs::read_to_string(path).unwrap().replace(
            &format!("id: \"{}\"", original.id),
            &format!("id: {quote}{}{quote}", original.id),
        );
        fs::write(path, text).unwrap();
        let mut restored = find_meeting_note(&destination, original.meeting.as_ref().unwrap())
            .unwrap()
            .unwrap();
        restored.set_body("Continued");
        save_capture(&mut restored, &destination, now()).unwrap();
        finish_capture(&mut restored).unwrap();
        assert_eq!(restored.path, original.path);
        assert_eq!(
            find_meeting_note(&destination, original.meeting.as_ref().unwrap())
                .unwrap()
                .unwrap()
                .body,
            "Continued\n"
        );
    }
}

#[test]
fn fractional_second_occurrences_round_trip_at_full_precision() {
    for fraction in [".123", ".123456", ".123456789"] {
        let vault = tempdir().unwrap();
        let destination = Destination::new(vault.path().into());
        let start =
            DateTime::parse_from_rfc3339(&format!("2026-11-06T09:00:00{fraction}+01:00")).unwrap();
        let event = meeting("event", start);
        let original = save(&destination, event.clone(), "Text");
        let mut restored = find_meeting_note(&destination, &event).unwrap().unwrap();
        assert_eq!(restored.id, original.id);
        let text = fs::read_to_string(original.path.as_ref().unwrap()).unwrap();
        assert!(text.contains(&format!(
            "meeting_start: 2026-11-06T09:00:00{fraction}+01:00"
        )));
        assert!(text.contains(&format!("meeting_end: 2026-11-06T09:30:00{fraction}+01:00")));
        restored.set_body("Continued");
        save_capture(&mut restored, &destination, now()).unwrap();
        assert_eq!(
            find_meeting_note(&destination, &event).unwrap().unwrap().id,
            original.id
        );
        assert!(
            find_meeting_note(&destination, &meeting("event", now()))
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn multiline_updated_values_are_replaced_without_stale_continuations() {
    for marker in [">-", "|-"] {
        let vault = tempdir().unwrap();
        let destination = Destination::new(vault.path().into());
        let original = save(&destination, meeting("event", now()), "Text");
        let path = original.path.as_ref().unwrap();
        let text = fs::read_to_string(path).unwrap().replace(
            "updated: 2026-11-06T09:00:00.000+01:00\n",
            &format!(
                "updated: {marker} # Keep inline comment\n  2026-11-06T09:00:00.000+01:00\n# Keep timestamp comment\n"
            ),
        );
        fs::write(path, text).unwrap();
        let mut restored = find_meeting_note(&destination, original.meeting.as_ref().unwrap())
            .unwrap()
            .unwrap();
        restored.set_body("Continued");
        let updated = now() + TimeDelta::minutes(1);
        save_capture(&mut restored, &destination, updated).unwrap();
        let text = fs::read_to_string(path).unwrap();
        assert!(!text.contains("\n  2026-11-06T09:00:00.000+01:00"));
        assert_eq!(text.matches("# Keep timestamp comment").count(), 1);
        assert_eq!(text.matches("# Keep inline comment").count(), 1);
        assert_eq!(
            find_meeting_note(&destination, original.meeting.as_ref().unwrap())
                .unwrap()
                .unwrap()
                .updated_at,
            updated
        );
    }
}

#[test]
fn missing_summary_keeps_regenerated_meeting_details_before_editable_notes() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    let original = save(&destination, meeting("event", now()), "Text");
    let path = original.path.as_ref().unwrap();
    let text = fs::read_to_string(path)
        .unwrap()
        .replace("## Summary\n\n## Follow-ups\n\n---\n\n", "");
    fs::write(path, text).unwrap();
    let mut restored = find_meeting_note(&destination, original.meeting.as_ref().unwrap())
        .unwrap()
        .unwrap();
    restored.disconnect();
    save_capture(&mut restored, &destination, now()).unwrap();
    let replacement = meeting("replacement", now() + TimeDelta::minutes(30));
    restored.connect(replacement.clone());
    save_capture(&mut restored, &destination, now()).unwrap();
    let text = fs::read_to_string(path).unwrap();
    assert!(text.find("## Meeting details").unwrap() < text.find("## Notes").unwrap());
    let mut reopened = find_meeting_note(&destination, &replacement)
        .unwrap()
        .unwrap();
    assert_eq!(reopened.body, "Text\n");
    reopened.set_body("Continued");
    save_capture(&mut reopened, &destination, now()).unwrap();
    assert_eq!(
        fs::read_to_string(path)
            .unwrap()
            .matches("## Notes")
            .count(),
        1
    );
    assert_eq!(
        find_meeting_note(&destination, &replacement)
            .unwrap()
            .unwrap()
            .body,
        "Continued\n"
    );
}

#[test]
fn ai_tidy_provenance_is_recorded_kept_on_reopen_and_removed_by_undo() {
    let vault = tempdir().unwrap();
    let destination = Destination::new(vault.path().into());
    let mut capture = save(&destination, meeting("event", now()), "rough text");
    let path = capture.path.clone().unwrap();
    assert!(!fs::read_to_string(&path).unwrap().contains("ai_tidied"));

    capture.set_body("Tidied text.");
    capture.ai_tidied = true;
    save_capture(&mut capture, &destination, now()).unwrap();
    let tidied = fs::read_to_string(&path).unwrap();
    assert!(tidied.contains("ai_refined: false\nai_tidied: on-device\nstatus: inbox\n"));

    // A refinement made in the vault survives, and reopening keeps the provenance.
    fs::write(
        &path,
        tidied.replace("processing: raw", "processing: refined"),
    )
    .unwrap();
    let mut restored = find_meeting_note(&destination, capture.meeting.as_ref().unwrap())
        .unwrap()
        .unwrap();
    assert!(restored.ai_tidied);
    restored.set_body("Tidied text, continued.");
    save_capture(&mut restored, &destination, now()).unwrap();
    let continued = fs::read_to_string(&path).unwrap();
    assert!(continued.contains("ai_tidied: on-device\n"));
    assert!(continued.contains("processing: refined\n"));

    restored.set_body("rough text");
    restored.ai_tidied = false;
    save_capture(&mut restored, &destination, now()).unwrap();
    let undone = fs::read_to_string(&path).unwrap();
    assert!(!undone.contains("ai_tidied"));
    assert!(undone.contains("processing: refined\n"));
    assert!(undone.ends_with("rough text\n"));
}
