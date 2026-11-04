mod common;

use chrono::{DateTime, Duration, FixedOffset};
use common::{capture, instant, meeting};
use noteiq_core::{
    destination::{Destination, save_capture},
    markdown::render_markdown,
    model::{Capture, CaptureSource, Meeting},
};
use std::path::Path;
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

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("front_matter")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("fixture {} is readable: {error}", path.display()))
        .replace("\r\n", "\n")
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

fn split_note(rendered: &str) -> (Yaml, String) {
    let rest = rendered
        .strip_prefix("---\n")
        .expect("front matter opens the note");
    let end = rest.find("\n---\n").expect("front matter is closed");
    let mut documents = YamlLoader::load_from_str(&rest[..end]).expect("front matter is YAML");
    assert_eq!(documents.len(), 1);
    (
        documents.remove(0),
        rest[end + "\n---\n".len()..].to_owned(),
    )
}

fn strings(value: &Yaml) -> Vec<&str> {
    value
        .as_vec()
        .expect("YAML sequence")
        .iter()
        .map(|item| item.as_str().expect("YAML string"))
        .collect()
}

fn unicode_capture() -> Capture {
    let mut capture = fixed_capture("# Café ✅\n\nNotes with emoji 🎉 and 日本語.");
    capture.tags = vec!["über".to_owned(), "日本".to_owned()];
    capture.connect(Meeting {
        title: Some("Café sync ☕ 日本語".to_owned()),
        organizer: Some("Zoë Ångström".to_owned()),
        attendees: vec!["Zoë Ångström".to_owned(), "José Müller".to_owned()],
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
        attendees: vec!["- not a list item".to_owned(), "key: value".to_owned()],
        ..meeting()
    });
    capture
}

#[test]
fn rendered_notes_match_the_front_matter_fixtures_exactly() {
    let mut with_meeting = fixed_capture("Decision: ship the beta.");
    with_meeting.connect(meeting());
    let mut no_meeting = with_meeting.clone();
    no_meeting.disconnect();

    let cases = [
        ("meeting.md", with_meeting),
        ("no_meeting.md", no_meeting),
        ("unicode.md", unicode_capture()),
        ("quotes.md", quotes_capture()),
        ("empty_body.md", fixed_capture("")),
    ];

    for (name, capture) in cases {
        let rendered = render_markdown(&capture, later()).expect("renders");
        assert_eq!(rendered, fixture(name), "fixture {name} drifted");
    }
}

#[test]
fn every_fixture_has_parseable_front_matter_with_the_original_values() {
    for name in FIXTURES {
        let (front_matter, _) = split_note(&fixture(name));
        assert_eq!(front_matter["schema"].as_i64(), Some(1), "{name}");
        assert_eq!(front_matter["id"].as_str(), Some(FIXED_ID), "{name}");
        assert_eq!(
            front_matter["created_at"].as_str(),
            Some("2026-11-03T14:15:00+01:00"),
            "{name}"
        );
        assert_eq!(front_matter["source"].as_str(), Some("shortcut"), "{name}");
    }

    let (quotes, body) = split_note(&fixture("quotes.md"));
    assert_eq!(
        quotes["meeting"]["title"].as_str(),
        Some(r#"Q4 "Kickoff": plan # 1 at C:\Temp"#)
    );
    assert_eq!(
        quotes["meeting"]["organizer"].as_str(),
        Some("O'Brien, Pat")
    );
    assert_eq!(
        strings(&quotes["meeting"]["attendees"]),
        vec!["- not a list item", "key: value"]
    );
    assert_eq!(
        body,
        "She said \"go\".\n---\nThis line is body text, not front matter.\n"
    );

    let (unicode, _) = split_note(&fixture("unicode.md"));
    assert_eq!(strings(&unicode["tags"]), vec!["über", "日本"]);
    assert_eq!(
        unicode["meeting"]["title"].as_str(),
        Some("Café sync ☕ 日本語")
    );

    let (no_meeting, _) = split_note(&fixture("no_meeting.md"));
    assert!(no_meeting["meeting"].is_badvalue());
}

#[test]
fn note_id_and_created_at_are_stable_across_saves_while_updated_at_advances() {
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
    let (front_matter, body) = split_note(&std::fs::read_to_string(&second_path).unwrap());
    let expected_id = capture.id.to_string();
    assert_eq!(front_matter["id"].as_str(), Some(expected_id.as_str()));
    assert_eq!(
        front_matter["created_at"].as_str(),
        Some(instant().to_rfc3339().as_str())
    );
    assert_eq!(
        front_matter["updated_at"].as_str(),
        Some(later().to_rfc3339().as_str())
    );
    assert_eq!(capture.updated_at, later());
    assert_eq!(body, "A different first line\n");
}

#[test]
fn missing_meeting_details_are_omitted_instead_of_written_as_nulls() {
    let mut capture = fixed_capture("Sparse meeting");
    capture.connect(Meeting {
        title: None,
        organizer: None,
        attendees: Vec::new(),
        ..meeting()
    });

    let rendered = render_markdown(&capture, later()).unwrap();
    let (front_matter, _) = split_note(&rendered);

    assert_eq!(front_matter["meeting"]["id"].as_str(), Some("event-42"));
    assert!(front_matter["meeting"]["title"].is_badvalue());
    assert!(front_matter["meeting"]["organizer"].is_badvalue());
    assert_eq!(front_matter["meeting"]["attendees"].as_vec(), Some(&vec![]));
    assert!(!rendered.contains("null"));
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
        let (front_matter, _) = split_note(&render_markdown(&capture, instant()).unwrap());

        assert_eq!(front_matter["source"].as_str(), Some(expected));
        assert_eq!(front_matter["timezone"].as_str(), Some("Europe/Amsterdam"));
        assert_eq!(
            strings(&front_matter["tags"]),
            vec!["meeting-note", "follow-up"]
        );
    }
}
