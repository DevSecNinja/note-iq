mod common;

use common::{capture, instant, meeting};
use noteiq_core::markdown::render_markdown;

#[test]
fn serializes_front_matter_separately_from_markdown_body() {
    let mut capture = capture();
    capture.set_body("# Review\n\nKeep `quoted: text` and 日本語.");
    capture.connect(meeting());

    let rendered = render_markdown(&capture, instant()).expect("valid Markdown");

    assert!(rendered.starts_with("---\nschema: 1\n"));
    assert!(rendered.contains("timezone: \"Europe/London\""));
    assert!(rendered.contains("meeting:\n  id: \"event-42\""));
    assert!(rendered.ends_with("\n---\n# Review\n\nKeep `quoted: text` and 日本語.\n"));
}

#[test]
fn omits_all_meeting_fields_after_explicit_disconnect() {
    let mut capture = capture();
    capture.set_body("Keep this note.");
    capture.connect(meeting());
    capture.disconnect();

    let rendered = render_markdown(&capture, instant()).expect("valid Markdown");

    assert!(!rendered.contains("\nmeeting:"));
    assert!(!rendered.contains("attendees:"));
    assert!(rendered.ends_with("\n---\nKeep this note.\n"));
}

#[test]
fn emits_an_empty_body_without_inventing_a_title() {
    let rendered = render_markdown(&capture(), instant()).expect("valid Markdown");

    assert!(!rendered.contains("title:"));
    assert!(!rendered.contains("meeting:"));
    assert!(rendered.ends_with("\n---\n\n"));
}
