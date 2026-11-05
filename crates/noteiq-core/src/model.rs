use crate::note_filename::NoteFilename;
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSource {
    Shortcut,
    Tray,
    Cli,
    Uri,
    NewNote,
}

impl CaptureSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shortcut => "shortcut",
            Self::Tray => "tray",
            Self::Cli => "cli",
            Self::Uri => "uri",
            Self::NewNote => "new_note",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeetingDecision {
    Automatic,
    Selected,
    Disconnected,
    Pending,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InviteeRole {
    Required,
    Optional,
    Resource,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rsvp {
    Accepted,
    Tentative,
    Declined,
    NotResponded,
    Organizer,
    Unknown,
}

impl Rsvp {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Tentative => "tentative",
            Self::Declined => "declined",
            Self::NotResponded => "not_responded",
            Self::Organizer => "organizer",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Invitee {
    pub name: Option<String>,
    pub email: Option<String>,
    pub role: InviteeRole,
    pub rsvp: Rsvp,
    pub has_attended: Option<bool>,
}

impl Invitee {
    pub fn display_contact(&self) -> String {
        format_contact(self.name.as_deref(), self.email.as_deref()).unwrap_or_default()
    }
}

pub fn format_contact(name: Option<&str>, email: Option<&str>) -> Option<String> {
    match (name, email) {
        (Some(name), Some(email)) if !name.eq_ignore_ascii_case(email) => {
            Some(format!("{name} <{email}>"))
        }
        (_, Some(email)) => Some(email.to_owned()),
        (Some(name), None) => Some(name.to_owned()),
        (None, None) => None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Meeting {
    pub id: String,
    pub title: Option<String>,
    pub start: Option<DateTime<FixedOffset>>,
    pub end: Option<DateTime<FixedOffset>>,
    pub organizer: Option<String>,
    #[serde(default)]
    pub invitees: Vec<Invitee>,
    #[serde(default)]
    pub your_rsvp: Option<Rsvp>,
    #[serde(skip)]
    pub entity_url: Option<String>,
    #[serde(default)]
    pub cancelled: bool,
    #[serde(default)]
    pub declined: bool,
    #[serde(default)]
    pub all_day: bool,
}

#[derive(Clone, Debug)]
pub struct Capture {
    pub id: Uuid,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
    pub timezone: String,
    pub source: CaptureSource,
    /// Display name of the person taking the note, written as the vault's `from` wikilink.
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub body: String,
    pub meeting: Option<Meeting>,
    pub meeting_decision: MeetingDecision,
    pub path: Option<PathBuf>,
    pub(crate) first_saved_at: Option<DateTime<FixedOffset>>,
    pub(crate) filename_meeting_slug: Option<String>,
    /// Fixed at the first save, so changing the setting never renames notes already saved.
    pub(crate) filename_template: Option<NoteFilename>,
}

impl Capture {
    pub fn new(source: CaptureSource, created_at: DateTime<FixedOffset>, timezone: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at,
            updated_at: created_at,
            timezone,
            source,
            author: None,
            tags: Vec::new(),
            body: String::new(),
            meeting: None,
            meeting_decision: MeetingDecision::Pending,
            path: None,
            first_saved_at: None,
            filename_meeting_slug: None,
            filename_template: None,
        }
    }

    pub fn set_body(&mut self, body: impl Into<String>) {
        self.body = body.into();
    }

    pub fn connect(&mut self, meeting: Meeting) {
        self.meeting = Some(meeting);
        self.meeting_decision = MeetingDecision::Selected;
    }

    pub fn auto_connect(&mut self, meeting: Meeting) {
        if self.meeting_decision == MeetingDecision::Disconnected
            || self.meeting_decision == MeetingDecision::Selected
        {
            return;
        }
        self.meeting = Some(meeting);
        self.meeting_decision = MeetingDecision::Automatic;
    }

    pub fn disconnect(&mut self) {
        self.meeting = None;
        self.meeting_decision = MeetingDecision::Disconnected;
    }

    pub fn is_empty(&self) -> bool {
        self.body.trim().is_empty()
    }
}
