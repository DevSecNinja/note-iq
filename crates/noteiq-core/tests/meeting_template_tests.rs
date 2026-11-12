mod common;

use common::{capture, instant, invitee, meeting};
use noteiq_core::destination::{Destination, save_capture};
use noteiq_core::meeting::{MeetingSelection, apply_automatic_selection};
use noteiq_core::meeting_template::{
    DEFAULT_TEMPLATE_ID, MeetingTemplate, MeetingTemplateError, TemplateInsertion, TemplateStore,
    built_in_templates, default_template, general_template, insert_template, render_template,
    sync_capture_template, validate_body, validate_name,
};
use noteiq_core::model::{InviteeRole, Rsvp};
use noteiq_core::saved_note::find_meeting_note;
use noteiq_core::settings::Settings;
use tempfile::tempdir;

fn user_template(body: &str) -> MeetingTemplate {
    MeetingTemplate {
        id: "Weekly sync".to_owned(),
        name: "Weekly sync".to_owned(),
        body: body.to_owned(),
        built_in: false,
    }
}

#[test]
fn built_ins_start_with_general_and_nest_under_the_notes_heading() {
    let templates = built_in_templates();
    let names: Vec<_> = templates.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        ["General meeting", "1:1", "Customer call", "Stand-up"]
    );
    assert_eq!(templates[0].id, DEFAULT_TEMPLATE_ID);
    assert_eq!(general_template(), templates[0]);
    for template in &templates {
        assert!(template.built_in);
        validate_body(&template.body).unwrap();
        // `#` and `##` headings would be read as generated sections of the saved note.
        assert!(
            template
                .body
                .lines()
                .filter(|line| line.starts_with('#'))
                .all(|line| line.starts_with("### ")),
            "{} must only use ### headings",
            template.name
        );
    }
    assert!(templates[0].body.contains("### Action items"));
    assert!(templates[3].body.contains("### Blockers"));
}

#[test]
fn placeholders_are_filled_from_the_meeting() {
    let mut meeting = meeting();
    meeting.organizer = Some("Olivia Organizer <olivia@example.com>".to_owned());
    let mut room = invitee("Room 4");
    room.role = InviteeRole::Resource;
    let mut organizer = invitee("Olivia Organizer");
    organizer.rsvp = Rsvp::Organizer;
    let mut by_email = invitee("");
    by_email.name = None;
    by_email.email = Some("sam@example.com".to_owned());
    meeting.invitees = vec![invitee("Ann"), room, organizer, by_email];

    let rendered = render_template(
        "{title} on {date} at { TIME }\nBy {Organizer}\nWith {attendees}",
        Some(&meeting),
        instant(),
    );
    assert_eq!(
        rendered,
        "Project review on 2026-11-03 at 14:00\nBy Olivia Organizer\nWith Ann, sam@example.com"
    );
}

#[test]
fn meeting_times_use_the_local_offset_and_fall_back_to_now() {
    let mut meeting = meeting();
    meeting.start =
        Some(chrono::DateTime::parse_from_rfc3339("2026-11-03T23:30:00+00:00").unwrap());
    assert_eq!(
        render_template("{date} {time}", Some(&meeting), instant()),
        "2026-11-04 00:30"
    );
    assert_eq!(
        render_template(
            "[{title}] {date} {time} [{organizer}] [{attendees}]",
            None,
            instant()
        ),
        "[] 2026-11-03 14:15 [] []"
    );
}

#[test]
fn unknown_placeholders_are_kept_when_rendering_and_rejected_when_saving() {
    let body = "Owner: {owner}\n```\nfn main() {}\n```\n{ not a placeholder } {a-b}";
    assert_eq!(render_template(body, Some(&meeting()), instant()), body);
    assert!(matches!(
        validate_body(body),
        Err(MeetingTemplateError::UnknownPlaceholder(name)) if name == "owner"
    ));
    validate_body("```\nfn main() {}\n```\n{ not a placeholder } {title}").unwrap();
    assert_eq!(
        validate_body("{Owner}").unwrap_err().to_string(),
        "Unknown placeholder: {Owner}. Use {title}, {date}, {time}, {organizer}, or {attendees}."
    );
    assert!(matches!(
        validate_body(" \n "),
        Err(MeetingTemplateError::EmptyBody)
    ));
}

#[test]
fn template_names_must_be_safe_file_names() {
    assert_eq!(validate_name("  Weekly sync ").unwrap(), "Weekly sync");
    let rejected = [
        ("", "empty_name"),
        ("../evil", "path_separator"),
        ("a\\b", "path_separator"),
        ("bad:name", "invalid_character"),
        ("tab\tname", "control_character"),
        (".hidden", "edge_dot"),
        ("..", "edge_dot"),
        ("CON", "reserved_name"),
        ("General meeting", "built_in_name"),
        ("stand-up", "built_in_name"),
    ];
    for (name, kind) in rejected {
        assert_eq!(
            validate_name(name).unwrap_err().kind(),
            kind,
            "{name:?} must be rejected"
        );
    }
    assert_eq!(
        validate_name(&"x".repeat(61)).unwrap_err().kind(),
        "name_too_long"
    );
}

#[test]
fn user_templates_round_trip_as_markdown_files_beside_the_settings() {
    let folder = tempdir().unwrap();
    let store = TemplateStore::beside_settings(&folder.path().join("settings.toml"));
    assert_eq!(store.folder(), folder.path().join("templates"));
    assert!(store.user_templates().unwrap().is_empty(), "no folder yet");

    let created = store
        .create(" Weekly sync ", "### Wins\r\n\r\n- {title}\r\n")
        .unwrap();
    assert_eq!(created.id, "Weekly sync");
    assert!(!created.built_in);
    let file = folder.path().join("templates").join("Weekly sync.md");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "### Wins\n\n- {title}\n"
    );
    assert_eq!(store.get("Weekly sync").unwrap(), Some(created.clone()));
    let names: Vec<_> = store.list().into_iter().map(|t| t.name).collect();
    assert_eq!(
        names,
        [
            "General meeting",
            "1:1",
            "Customer call",
            "Stand-up",
            "Weekly sync"
        ]
    );

    let edited = store
        .update("Weekly sync", "Team sync", "### Wins\n\n- Shipped\n")
        .unwrap();
    assert_eq!(edited.id, "Team sync");
    assert!(!file.exists(), "renaming moves the file");
    assert_eq!(
        store.get("Team sync").unwrap().unwrap().body,
        "### Wins\n\n- Shipped\n"
    );
    let recased = store.update("Team sync", "team Sync", "Agenda").unwrap();
    assert_eq!(recased.id, "team Sync");
    assert_eq!(store.user_templates().unwrap(), vec![recased]);

    store.delete("team Sync").unwrap();
    assert!(store.user_templates().unwrap().is_empty());
    assert!(matches!(
        store.delete("team Sync"),
        Err(MeetingTemplateError::NotFound)
    ));
}

#[test]
fn the_store_rejects_duplicates_built_in_changes_and_paths_outside_its_folder() {
    let folder = tempdir().unwrap();
    let store = TemplateStore::new(folder.path().join("templates"));
    store.create("Retro", "### Went well").unwrap();
    store.create("Planning", "### Goals").unwrap();

    assert!(matches!(
        store.create("RETRO", "Other"),
        Err(MeetingTemplateError::DuplicateName(_))
    ));
    assert!(matches!(
        store.update("Planning", "retro", "Other"),
        Err(MeetingTemplateError::DuplicateName(_))
    ));
    assert!(matches!(
        store.create("Notes", "{owner}"),
        Err(MeetingTemplateError::UnknownPlaceholder(_))
    ));
    assert!(matches!(
        store.update(DEFAULT_TEMPLATE_ID, "Mine", "Text"),
        Err(MeetingTemplateError::ReadOnly)
    ));
    assert!(matches!(
        store.delete("stand-up"),
        Err(MeetingTemplateError::ReadOnly)
    ));
    std::fs::write(folder.path().join("secret.md"), "outside").unwrap();
    assert_eq!(store.get("../secret").unwrap(), None);
    assert!(matches!(
        store.delete("../secret"),
        Err(MeetingTemplateError::NotFound)
    ));
    assert!(folder.path().join("secret.md").exists());
    assert_eq!(store.get("Planning").unwrap().unwrap().body, "### Goals");
}

#[test]
fn files_that_are_not_usable_templates_are_skipped() {
    let folder = tempdir().unwrap();
    let templates = folder.path().join("templates");
    std::fs::create_dir_all(&templates).unwrap();
    std::fs::write(templates.join("General meeting.md"), "shadow").unwrap();
    std::fs::write(templates.join("notes.txt"), "not markdown").unwrap();
    std::fs::write(templates.join(" padded.md"), "untrimmed").unwrap();
    std::fs::write(templates.join("Hand edited.MD"), "### Mine").unwrap();
    let store = TemplateStore::new(templates);

    let user = store.user_templates().unwrap();
    assert_eq!(user.len(), 1);
    assert_eq!(user[0].name, "Hand edited");
    assert_eq!(store.get("general").unwrap(), Some(general_template()));
}

#[test]
fn a_missing_or_invalid_default_falls_back_to_general() {
    let folder = tempdir().unwrap();
    let store = TemplateStore::new(folder.path().join("templates"));
    assert_eq!(store.resolve("Deleted template"), general_template());
    assert_eq!(store.resolve("../../settings"), general_template());
    assert_eq!(store.resolve("stand-up").name, "Stand-up");

    let mut settings = Settings::default_for(folder.path());
    assert_eq!(default_template(&settings, &store), None, "off by default");
    settings.meeting_templates_enabled = true;
    settings.default_meeting_template = "Deleted template".to_owned();
    assert_eq!(
        default_template(&settings, &store),
        Some(general_template())
    );
    store.create("Retro", "### Went well").unwrap();
    settings.default_meeting_template = "Retro".to_owned();
    assert_eq!(default_template(&settings, &store).unwrap().name, "Retro");
}

#[test]
fn template_settings_round_trip_and_older_files_load_with_templates_off() {
    let folder = tempdir().unwrap();
    let path = folder.path().join("settings.toml");
    std::fs::write(&path, "auto_connect_meeting = false\n").unwrap();
    let loaded = Settings::load(&path, folder.path()).unwrap();
    assert!(!loaded.meeting_templates_enabled);
    assert_eq!(loaded.default_meeting_template, DEFAULT_TEMPLATE_ID);
    assert!(!loaded.auto_connect_meeting);

    let mut settings = loaded;
    settings.meeting_templates_enabled = true;
    settings.default_meeting_template = "Weekly sync".to_owned();
    settings.save(&path).unwrap();
    assert_eq!(Settings::load(&path, folder.path()).unwrap(), settings);
}

#[test]
fn connecting_an_empty_note_inserts_the_template_without_saving_it() {
    let folder = tempdir().unwrap();
    let mut capture = capture();
    let template = user_template("### {title}\n\n- ");
    capture.connect(meeting());
    assert!(sync_capture_template(
        &mut capture,
        Some(&template),
        instant()
    ));
    assert_eq!(capture.body, "### Project review\n\n- ");
    assert!(capture.has_untouched_template());
    assert!(capture.is_empty(), "an untouched template is not a note");
    let destination = Destination::new(folder.path().to_path_buf());
    assert_eq!(
        save_capture(&mut capture, &destination, instant()).unwrap(),
        None
    );
    assert!(
        !sync_capture_template(&mut capture, Some(&template), instant()),
        "the same meeting is only templated once"
    );
}

#[test]
fn templates_are_only_applied_when_turned_on_and_never_over_user_text() {
    let mut capture = capture();
    capture.connect(meeting());
    assert!(!sync_capture_template(&mut capture, None, instant()));
    assert_eq!(capture.body, "");

    let mut typed = common::capture();
    typed.set_body("My own notes");
    typed.connect(meeting());
    assert!(!sync_capture_template(
        &mut typed,
        Some(&general_template()),
        instant()
    ));
    assert_eq!(typed.body, "My own notes");

    // A note the user emptied on purpose is not refilled for the same meeting.
    let mut cleared = common::capture();
    cleared.connect(meeting());
    sync_capture_template(&mut cleared, Some(&general_template()), instant());
    cleared.set_body("");
    assert!(!sync_capture_template(
        &mut cleared,
        Some(&general_template()),
        instant()
    ));
    assert_eq!(cleared.body, "");
}

#[test]
fn automatic_connection_applies_the_template_and_follows_the_next_meeting() {
    let template = user_template("Notes for {title}");
    let mut capture = capture();
    apply_automatic_selection(&mut capture, MeetingSelection::Connected(meeting()));
    assert!(sync_capture_template(
        &mut capture,
        Some(&template),
        instant()
    ));
    assert_eq!(capture.body, "Notes for Project review");

    let mut next = meeting();
    next.id = "event-43".to_owned();
    next.title = Some("Retro".to_owned());
    apply_automatic_selection(&mut capture, MeetingSelection::Connected(next.clone()));
    assert_eq!(
        capture.meeting.as_ref().map(|m| m.id.as_str()),
        Some("event-43"),
        "an untouched template still lets the note move on"
    );
    assert!(sync_capture_template(
        &mut capture,
        Some(&template),
        instant()
    ));
    assert_eq!(capture.body, "Notes for Retro");
}

#[test]
fn turning_templates_off_drops_an_untouched_template_when_the_meeting_changes() {
    let template = user_template("Notes for {title}");
    let mut next = meeting();
    next.id = "event-43".to_owned();
    next.title = Some("Retro".to_owned());

    let mut untouched = capture();
    untouched.connect(meeting());
    sync_capture_template(&mut untouched, Some(&template), instant());
    untouched.connect(next.clone());
    assert!(sync_capture_template(&mut untouched, None, instant()));
    assert_eq!(untouched.body, "", "no details from the previous meeting");
    assert!(!untouched.has_untouched_template());

    let mut edited = capture();
    edited.connect(meeting());
    sync_capture_template(&mut edited, Some(&template), instant());
    edited.set_body("Notes for Project review\nShip it");
    edited.connect(next);
    assert!(!sync_capture_template(&mut edited, None, instant()));
    assert_eq!(edited.body, "Notes for Project review\nShip it");
}

#[test]
fn disconnecting_removes_only_an_untouched_template() {
    let template = general_template();
    let mut untouched = capture();
    untouched.connect(meeting());
    sync_capture_template(&mut untouched, Some(&template), instant());
    untouched.disconnect();
    assert!(sync_capture_template(
        &mut untouched,
        Some(&template),
        instant()
    ));
    assert_eq!(untouched.body, "");
    assert!(!untouched.has_untouched_template());

    let mut edited = capture();
    edited.connect(meeting());
    sync_capture_template(&mut edited, Some(&template), instant());
    let typed = format!("{}Decided to ship.", edited.body);
    edited.set_body(typed.clone());
    assert!(!edited.is_empty());
    edited.disconnect();
    assert!(!sync_capture_template(&mut edited, None, instant()));
    assert_eq!(edited.body, typed);
}

#[test]
fn choosing_a_template_replaces_an_empty_note_and_appends_to_text() {
    let stand_up = built_in_templates().pop().unwrap();
    let mut capture = capture();
    capture.connect(meeting());
    sync_capture_template(&mut capture, Some(&general_template()), instant());
    assert_eq!(
        insert_template(&mut capture, &stand_up, instant()),
        TemplateInsertion::Replaced
    );
    assert_eq!(capture.body, stand_up.body);
    assert!(capture.is_empty());

    capture.set_body("Typed first\n\n");
    assert_eq!(
        insert_template(&mut capture, &stand_up, instant()),
        TemplateInsertion::Appended
    );
    assert_eq!(capture.body, format!("Typed first\n\n{}", stand_up.body));
    assert!(!capture.is_empty());
    capture.disconnect();
    assert!(!sync_capture_template(&mut capture, None, instant()));
    assert!(capture.body.starts_with("Typed first"));
}

#[test]
fn a_saved_note_with_a_template_reopens_with_its_text() {
    let folder = tempdir().unwrap();
    let destination = Destination::new(folder.path().to_path_buf());
    let mut capture = capture();
    capture.connect(meeting());
    sync_capture_template(&mut capture, Some(&general_template()), instant());
    let body = capture
        .body
        .replace("### Decisions\n\n- ", "### Decisions\n\n- Ship it");
    capture.set_body(body.clone());
    save_capture(&mut capture, &destination, instant())
        .unwrap()
        .expect("edited template is saved");

    let reopened = find_meeting_note(&destination, &meeting())
        .unwrap()
        .expect("the note is found again");
    assert_eq!(reopened.body, body);
}

#[test]
fn section_headings_are_rejected_on_save_and_demoted_in_hand_edited_templates() {
    assert_eq!(
        validate_body("### Agenda\n\n## Notes\n")
            .unwrap_err()
            .kind(),
        "top_level_heading"
    );
    assert!(matches!(
        validate_body("  # Title"),
        Err(MeetingTemplateError::TopLevelHeading(heading)) if heading == "# Title"
    ));
    validate_body("### Agenda\n```\n## Notes\n```\n#hashtag\n    ## indented code\n").unwrap();
    let folder = tempdir().unwrap();
    let store = TemplateStore::new(folder.path().join("templates"));
    assert!(matches!(
        store.create("Clash", "## Notes"),
        Err(MeetingTemplateError::TopLevelHeading(_))
    ));

    // A file edited outside NoteIQ, and a meeting name that looks like a heading.
    std::fs::create_dir_all(store.folder()).unwrap();
    std::fs::write(
        store.folder().join("Edited.md"),
        "# Plan\r\n\r\n## Notes\r\n\r\n{title}\r\n```\r\n## kept in code\r\n```\r\n",
    )
    .unwrap();
    let mut meeting = meeting();
    meeting.title = Some("## Notes".to_owned());
    let template = store.get("Edited").unwrap().unwrap();
    let mut capture = capture();
    capture.connect(meeting.clone());
    sync_capture_template(&mut capture, Some(&template), instant());
    assert_eq!(
        capture.body,
        "### Plan\n\n### Notes\n\n### Notes\n```\n## kept in code\n```\n"
    );

    let destination = Destination::new(folder.path().to_path_buf());
    capture.set_body(format!("{}Typed\n", capture.body));
    save_capture(&mut capture, &destination, instant())
        .unwrap()
        .expect("edited template is saved");
    let reopened = find_meeting_note(&destination, &meeting)
        .unwrap()
        .expect("the note can be reopened");
    assert_eq!(reopened.body, capture.body);
}
