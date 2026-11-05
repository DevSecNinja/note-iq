mod common;

use common::{capture, instant, meeting, raw_capture};
use noteiq_core::markdown::render_markdown;

#[test]
fn serializes_vault_properties_separately_from_the_raw_capture() {
    let mut capture = capture();
    capture.set_body("# Review\n\nKeep `quoted: text` and 日本語.");
    capture.connect(meeting());

    let rendered = render_markdown(&capture, instant()).expect("valid Markdown");

    assert!(rendered.starts_with("---\nkind: capture\nsource: manual\n"));
    assert!(rendered.contains("timezone: \"Europe/Amsterdam\""));
    assert!(rendered.contains("\nmeeting: \"Project review\"\n"));
    assert!(rendered.contains("\ncalendar_event_id: \"event-42\"\n"));
    assert!(rendered.contains("\n# Project review\n"));
    assert!(rendered.ends_with("\n\n# Review\n\nKeep `quoted: text` and 日本語.\n\n</details>\n"));
    assert_eq!(
        raw_capture(&rendered),
        "# Review\n\nKeep `quoted: text` and 日本語.\n"
    );
}

#[test]
fn omits_all_meeting_fields_after_explicit_disconnect() {
    let mut capture = capture();
    capture.set_body("Keep this note.");
    capture.connect(meeting());
    capture.disconnect();

    let rendered = render_markdown(&capture, instant()).expect("valid Markdown");

    assert!(!rendered.contains("\nmeeting:"));
    assert!(!rendered.contains("invitees:"));
    assert!(!rendered.contains("calendar_event_id:"));
    assert!(!rendered.contains("## Meeting details"));
    assert!(rendered.contains("\n# Keep this note.\n"));
    assert!(rendered.ends_with("\n\nKeep this note.\n\n</details>\n"));
}

#[test]
fn emits_an_empty_raw_capture_without_inventing_a_title_property() {
    let rendered = render_markdown(&capture(), instant()).expect("valid Markdown");

    assert!(!rendered.contains("title:"));
    assert!(!rendered.contains("meeting:"));
    assert!(rendered.contains("\n# Quick note\n"));
    assert!(rendered.ends_with("capture. -->\n\n</details>\n"));
    assert_eq!(raw_capture(&rendered), "");
}
