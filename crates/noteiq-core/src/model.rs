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
pub struct Meeting {
    pub id: String,
    pub title: Option<String>,
    pub start: Option<DateTime<FixedOffset>>,
    pub end: Option<DateTime<FixedOffset>>,
    pub organizer: Option<String>,
    #[serde(default)]
    pub attendees: Vec<String>,
    #[serde(default)]
    pub cancelled: bool,
    #[serde(default)]
    pub declined: bool,
}

#[derive(Clone, Debug)]
pub struct Capture {
    pub id: Uuid,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
    pub timezone: String,
    pub source: CaptureSource,
    pub tags: Vec<String>,
    pub body: String,
    pub meeting: Option<Meeting>,
    pub meeting_decision: MeetingDecision,
    pub path: Option<PathBuf>,
}

impl Capture {
    pub fn new(source: CaptureSource, created_at: DateTime<FixedOffset>, timezone: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            created_at,
            updated_at: created_at,
            timezone,
            source,
            tags: Vec::new(),
            body: String::new(),
            meeting: None,
            meeting_decision: MeetingDecision::Pending,
            path: None,
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
