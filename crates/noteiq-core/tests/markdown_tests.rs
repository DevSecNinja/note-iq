mod common;

use common::{capture, instant, meeting, raw_capture};
use noteiq_core::markdown::render_markdown;

#[test]
fn serializes_vault_properties_separately_from_editable_notes() {
    let mut capture = capture();
    capture.set_body("# Review\n\nKeep `quoted: text` and 日本語.");
    capture.connect(meeting());

    let rendered = render_markdown(&capture, instant()).expect("valid Markdown");

    assert!(rendered.starts_with("---\nkind: capture\nsource: manual\n"));
    assert!(rendered.contains("timezone: \"Europe/Amsterdam\""));
    assert!(rendered.contains("\nmeeting: \"Project review\"\n"));
    assert!(rendered.contains("\ncalendar_event_id: \"event-42\"\n"));
    assert!(rendered.contains("\n# Project review\n"));
    assert!(rendered.ends_with("\n## Notes\n\n# Review\n\nKeep `quoted: text` and 日本語.\n"));
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
    assert!(rendered.ends_with("\n## Notes\n\nKeep this note.\n"));
}

#[test]
fn emits_an_empty_notes_section_without_inventing_a_title_property() {
    let rendered = render_markdown(&capture(), instant()).expect("valid Markdown");

    assert!(!rendered.contains("title:"));
    assert!(!rendered.contains("meeting:"));
    assert!(rendered.contains("\n# Quick note\n"));
    assert!(rendered.ends_with("\n## Notes\n\n"));
    assert_eq!(raw_capture(&rendered), "");
}

#[test]
fn markdown_is_not_wrapped_in_html_or_an_invented_code_block() {
    let mut capture = capture();
    let body = "- [ ] Follow up\n\n**Decision:** ship it.\n\n[Plan](plan.md)\n";
    capture.set_body(body);

    let rendered = render_markdown(&capture, instant()).unwrap();

    assert!(rendered.ends_with(&format!("## Notes\n\n{body}")));
    assert!(!rendered.contains("<details>"));
    assert!(!rendered.contains("<summary>"));
    assert!(!rendered.contains("```"));
    assert!(!rendered.contains("frozen"));
    assert_eq!(raw_capture(&rendered), body);
}

#[test]
fn preserves_code_fences_written_by_the_note_taker() {
    let mut capture = capture();
    let body = "Example:\n\n```rust\nlet answer = 42;\n```\n\n- [ ] Try this\n";
    capture.set_body(body);

    let rendered = render_markdown(&capture, instant()).unwrap();

    assert_eq!(raw_capture(&rendered), body);
    assert_eq!(rendered.matches("```").count(), 2);
}
