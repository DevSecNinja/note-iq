use chrono::{DateTime, FixedOffset};
use noteiq_core::model::{Capture, CaptureSource, Invitee, InviteeRole, Meeting, Rsvp};

#[allow(dead_code)]
pub fn invitee(name: &str) -> Invitee {
    Invitee {
        name: Some(name.to_owned()),
        email: None,
        role: InviteeRole::Required,
        rsvp: Rsvp::Unknown,
        has_attended: None,
    }
}

pub fn instant() -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339("2026-11-03T14:15:00+01:00").expect("fixed test time")
}

/// A capture taken in Amsterdam, whose offset on the test date is +01:00.
#[allow(dead_code)]
pub fn capture() -> Capture {
    Capture::new(
        CaptureSource::Shortcut,
        instant(),
        "Europe/Amsterdam".to_owned(),
    )
}

#[allow(dead_code)]
pub fn meeting() -> Meeting {
    Meeting {
        id: "event-42".to_owned(),
        title: Some("Project review".to_owned()),
        start: Some(DateTime::parse_from_rfc3339("2026-11-03T14:00:00+01:00").unwrap()),
        end: Some(DateTime::parse_from_rfc3339("2026-11-03T14:30:00+01:00").unwrap()),
        organizer: Some("Organizer".to_owned()),
        invitees: vec![invitee("Invitee")],
        your_rsvp: None,
        entity_url: None,
        cancelled: false,
        declined: false,
        all_day: false,
    }
}

/// Returns the newline-terminated text in the editable Notes section.
#[allow(dead_code)]
pub fn raw_capture(note: &str) -> &str {
    note.split_once("\n## Notes\n\n")
        .expect("note has an editable Notes section")
        .1
}
