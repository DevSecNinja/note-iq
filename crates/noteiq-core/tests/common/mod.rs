use chrono::{DateTime, FixedOffset};
use noteiq_core::model::{Capture, CaptureSource, Meeting};

pub fn instant() -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339("2026-11-03T14:15:00+01:00").expect("fixed test time")
}

#[allow(dead_code)]
pub fn capture() -> Capture {
    Capture::new(
        CaptureSource::Shortcut,
        instant(),
        "Europe/London".to_owned(),
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
        attendees: vec!["Attendee".to_owned()],
        cancelled: false,
        declined: false,
    }
}
