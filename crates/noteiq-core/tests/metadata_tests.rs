mod common;

use chrono::{DateTime, Duration, FixedOffset};
use common::{capture, instant, invitee, meeting, raw_capture};
use noteiq_core::{
    destination::{Destination, save_capture},
    markdown::render_markdown,
    model::{Capture, CaptureSource, Invitee, InviteeRole, Meeting, Rsvp},
};
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use uuid::Uuid;
use yaml_rust2::{Yaml, YamlLoader};

const FIXED_ID: &str = "00000000-0000-4000-8000-000000000042";
const FIXTURES: [&str; 5] = [
    "meeting.md",
    "no_meeting.md",
    "unicode.md",
    "quotes.md",
    "empty_body.md",
];
/// Set this variable to `1` to rewrite the fixtures from the current renderer, then review the diff.
const BLESS_VARIABLE: &str = "NOTEIQ_BLESS_FIXTURES";
const MEETING_KEYS: [&str; 10] = [
    "meeting",
    "meeting_start",
    "meeting_end",
    "organizer",
    "your_rsvp",
    "required_invitees",
    "optional_invitees",
    "resource_invitees",
    "other_invitees",
    "calendar_event_id",
];

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("front_matter")
        .join(name)
}

fn fixture(name: &str) -> String {
    let path = fixture_path(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("fixture {} is readable: {error}", path.display()))
        .replace("\r\n", "\n")
}

fn blessing() -> bool {
    std::env::var_os(BLESS_VARIABLE).is_some_and(|value| value == "1")
}

fn later() -> DateTime<FixedOffset> {
    instant() + Duration::minutes(5)
}

fn fixed_capture(body: &str) -> Capture {
    let mut capture = capture();
    capture.id = Uuid::parse_str(FIXED_ID).unwrap();
    capture.set_body(body);
    capture
}

fn front_matter(note: &str) -> Yaml {
    let rest = note
        .strip_prefix("---\n")
        .expect("front matter opens the note");
    let end = rest.find("\n---\n").expect("front matter is closed");
    let mut documents = YamlLoader::load_from_str(&rest[..end]).expect("front matter is YAML");
    assert_eq!(documents.len(), 1);
    documents.remove(0)
}

fn strings(value: &Yaml) -> Vec<&str> {
    value
        .as_vec()
        .expect("YAML sequence")
        .iter()
        .map(|item| item.as_str().expect("YAML string"))
        .collect()
}

fn person(name: &str, email: Option<&str>, role: InviteeRole, rsvp: Rsvp) -> Invitee {
    Invitee {
        email: email.map(str::to_owned),
        role,
        rsvp,
        ..invitee(name)
    }
}

fn meeting_capture() -> Capture {
    let mut capture = fixed_capture("Decision: ship the beta.\n\n- [ ] Send the deck to Sam");
    capture.author = Some("Alex Example".to_owned());
    capture.connect(Meeting {
        your_rsvp: Some(Rsvp::Accepted),
        invitees: vec![
            Invitee {
                has_attended: Some(true),
                ..person(
                    "Sam Lee",
                    Some("sam@example.com"),
                    InviteeRole::Required,
                    Rsvp::Accepted,
                )
            },
            person("Riya Patel", None, InviteeRole::Optional, Rsvp::Tentative),
            person(
                "Room 4.12",
                Some("room412@example.com"),
                InviteeRole::Resource,
                Rsvp::Accepted,
            ),
        ],
        ..meeting()
    });
    capture
}

fn unicode_capture() -> Capture {
    let mut capture = fixed_capture("# Café ✅\n\nNotes with emoji 🎉 and 日本語.");
    capture.tags = vec!["über".to_owned(), "日本".to_owned()];
    capture.connect(Meeting {
        title: Some("Café sync ☕ 日本語".to_owned()),
        organizer: Some("Zoë Ångström".to_owned()),
        invitees: vec![invitee("Zoë Ångström"), invitee("José Müller")],
        ..meeting()
    });
    capture
}

fn quotes_capture() -> Capture {
    let mut capture =
        fixed_capture("She said \"go\".\n---\nThis line is body text, not front matter.");
    capture.connect(Meeting {
        title: Some(r#"Q4 "Kickoff": plan # 1 at C:\Temp"#.to_owned()),
        organizer: Some("O'Brien, Pat".to_owned()),
        invitees: vec![invitee("- not a list item"), invitee("key: value")],
        ..meeting()
    });
    capture
}

#[test]
fn rendered_notes_match_the_front_matter_fixtures_exactly() {
    let with_meeting = meeting_capture();
    let mut no_meeting = with_meeting.clone();
    no_meeting.disconnect();

    let cases = [
        ("meeting.md", with_meeting),
        ("no_meeting.md", no_meeting),
        ("unicode.md", unicode_capture()),
        ("quotes.md", quotes_capture()),
        ("empty_body.md", fixed_capture("")),
    ];
    assert_eq!(cases.each_ref().map(|(name, _)| *name), FIXTURES);

    for (name, capture) in cases {
        let rendered = render_markdown(&capture, later()).expect("renders");
        if blessing() {
            std::fs::write(fixture_path(name), &rendered).expect("fixture is writable");
            continue;
        }
        assert_eq!(
            rendered,
            fixture(name),
            "fixture {name} drifted; if the change is intended, rerun with {BLESS_VARIABLE}=1 and review the diff"
        );
    }
}

#[test]
fn every_fixture_has_parseable_front_matter_with_the_original_values() {
    for name in FIXTURES {
        let properties = front_matter(&fixture(name));
        let text = |key: &str| properties[key].as_str();
        assert_eq!(text("kind"), Some("capture"), "{name}");
        assert_eq!(text("source"), Some("manual"), "{name}");
        assert_eq!(text("date"), Some("2026-11-03"), "{name}");
        assert_eq!(
            text("captured"),
            Some("2026-11-03T14:15:00.000+01:00"),
            "{name}"
        );
        assert_eq!(text("processing"), Some("raw"), "{name}");
        assert_eq!(properties["ai_refined"].as_bool(), Some(false), "{name}");
        assert_eq!(text("status"), Some("inbox"), "{name}");
        assert!(
            strings(&properties["tags"]).starts_with(&["capture", "note"]),
            "{name}"
        );
        assert_eq!(text("id"), Some(FIXED_ID), "{name}");
        assert_eq!(
            text("updated"),
            Some("2026-11-03T14:20:00.000+01:00"),
            "{name}"
        );
        assert_eq!(text("timezone"), Some("Europe/Amsterdam"), "{name}");
        assert_eq!(text("captured_via"), Some("shortcut"), "{name}");
        assert_eq!(properties["noteiq_schema"].as_i64(), Some(3), "{name}");
    }

    let meeting_note = front_matter(&fixture("meeting.md"));
    assert_eq!(meeting_note["from"].as_str(), Some("[[Alex Example]]"));
    assert_eq!(meeting_note["meeting"].as_str(), Some("Project review"));
    assert_eq!(
        meeting_note["meeting_start"].as_str(),
        Some("2026-11-03T14:00:00+01:00")
    );
    assert_eq!(
        meeting_note["meeting_end"].as_str(),
        Some("2026-11-03T14:30:00+01:00")
    );
    assert_eq!(meeting_note["organizer"].as_str(), Some("Organizer"));
    assert_eq!(meeting_note["your_rsvp"].as_str(), Some("accepted"));
    assert_eq!(
        strings(&meeting_note["required_invitees"]),
        vec!["Sam Lee <sam@example.com>"]
    );
    assert_eq!(
        strings(&meeting_note["optional_invitees"]),
        vec!["Riya Patel"]
    );
    assert_eq!(
        strings(&meeting_note["resource_invitees"]),
        vec!["Room 4.12 <room412@example.com>"]
    );
    assert!(meeting_note["other_invitees"].is_badvalue());
    assert_eq!(meeting_note["calendar_event_id"].as_str(), Some("event-42"));

    let no_meeting = fixture("no_meeting.md");
    let no_meeting_properties = front_matter(&no_meeting);
    for key in MEETING_KEYS {
        assert!(
            no_meeting_properties[key].is_badvalue(),
            "{key} leaves with the meeting"
        );
    }
    assert_eq!(
        no_meeting_properties["from"].as_str(),
        Some("[[Alex Example]]")
    );
    assert!(no_meeting.contains("\n# Decision: ship the beta.\n"));
    assert!(!no_meeting.contains("## Meeting details"));

    let quotes = fixture("quotes.md");
    let quotes_properties = front_matter(&quotes);
    assert_eq!(
        quotes_properties["meeting"].as_str(),
        Some(r#"Q4 "Kickoff": plan # 1 at C:\Temp"#)
    );
    assert_eq!(
        quotes_properties["organizer"].as_str(),
        Some("O'Brien, Pat")
    );
    assert_eq!(
        strings(&quotes_properties["required_invitees"]),
        vec!["- not a list item", "key: value"]
    );
    assert!(quotes.contains("\n# Q4 \"Kickoff\": plan \\# 1 at C:\\\\Temp\n"));
    assert_eq!(
        raw_capture(&quotes),
        "She said \"go\".\n---\nThis line is body text, not front matter.\n"
    );

    let unicode = front_matter(&fixture("unicode.md"));
    assert_eq!(
        strings(&unicode["tags"]),
        vec!["capture", "note", "über", "日本"]
    );
    assert_eq!(unicode["meeting"].as_str(), Some("Café sync ☕ 日本語"));
    assert_eq!(
        strings(&unicode["required_invitees"]),
        vec!["Zoë Ångström", "José Müller"]
    );

    let empty = fixture("empty_body.md");
    assert!(front_matter(&empty)["from"].is_badvalue());
    assert!(empty.contains("\n# Quick note\n\n**Captured:** 2026-11-03 14:15\n"));
    assert_eq!(raw_capture(&empty), "");
}

#[test]
fn note_id_and_capture_time_are_stable_across_saves_while_updated_advances() {
    let temp = tempdir().unwrap();
    let destination = Destination::new(temp.path().to_path_buf());
    let mut capture = capture();
    capture.set_body("First line becomes the file name");
    let first_path = save_capture(&mut capture, &destination, instant())
        .unwrap()
        .unwrap();

    capture.set_body("A different first line");
    let second_path = save_capture(&mut capture, &destination, later())
        .unwrap()
        .unwrap();

    assert_eq!(first_path, second_path, "file names stay stable");
    let note = std::fs::read_to_string(&second_path).unwrap();
    let properties = front_matter(&note);
    let expected_id = capture.id.to_string();
    assert_eq!(properties["id"].as_str(), Some(expected_id.as_str()));
    assert_eq!(properties["date"].as_str(), Some("2026-11-03"));
    assert_eq!(
        properties["captured"].as_str(),
        Some("2026-11-03T14:15:00.000+01:00")
    );
    assert_eq!(
        properties["updated"].as_str(),
        Some("2026-11-03T14:20:00.000+01:00")
    );
    assert_eq!(capture.updated_at, later());
    assert_eq!(raw_capture(&note), "A different first line\n");
}

#[test]
fn missing_meeting_details_are_omitted_instead_of_written_as_nulls() {
    let mut capture = fixed_capture("Sparse meeting");
    capture.connect(Meeting {
        title: None,
        start: None,
        end: None,
        organizer: None,
        invitees: Vec::new(),
        ..meeting()
    });

    let rendered = render_markdown(&capture, later()).unwrap();
    let properties = front_matter(&rendered);

    assert_eq!(properties["calendar_event_id"].as_str(), Some("event-42"));
    for key in [
        "meeting",
        "meeting_start",
        "meeting_end",
        "organizer",
        "your_rsvp",
        "resource_invitees",
        "other_invitees",
    ] {
        assert!(properties[key].is_badvalue(), "{key} is omitted");
    }
    assert_eq!(properties["required_invitees"].as_vec(), Some(&vec![]));
    assert_eq!(properties["optional_invitees"].as_vec(), Some(&vec![]));
    assert!(!rendered.contains("null"));
    assert!(
        !rendered.contains("## Meeting details"),
        "a meeting without details adds no empty section"
    );
    assert!(rendered.contains("\n# Sparse meeting\n"));
}

#[test]
fn every_launch_source_is_recorded_with_a_stable_identifier() {
    let cases = [
        (CaptureSource::Shortcut, "shortcut"),
        (CaptureSource::Tray, "tray"),
        (CaptureSource::Cli, "cli"),
        (CaptureSource::Uri, "uri"),
        (CaptureSource::NewNote, "new_note"),
    ];

    for (source, expected) in cases {
        let mut capture = Capture::new(source, instant(), "Europe/Amsterdam".to_owned());
        capture.set_body("Source check");
        capture.tags = vec!["meeting-note".to_owned(), "follow-up".to_owned()];
        let properties = front_matter(&render_markdown(&capture, instant()).unwrap());

        assert_eq!(properties["captured_via"].as_str(), Some(expected));
        assert_eq!(
            properties["source"].as_str(),
            Some("manual"),
            "the vault property says who wrote the note, not how it was opened"
        );
        assert_eq!(properties["timezone"].as_str(), Some("Europe/Amsterdam"));
        assert_eq!(
            strings(&properties["tags"]),
            vec!["capture", "note", "meeting-note", "follow-up"]
        );
    }
}
