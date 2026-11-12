use crate::{meeting_reminder::is_reminder_key, model::CaptureSource};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchRequest {
    NewNote,
    /// A meeting reminder was clicked; the key identifies the meeting in the reminder history.
    MeetingReminder(String),
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum LaunchError {
    #[error("Unknown command. Use `noteiq new` to open a capture.")]
    UnknownCommand,
    #[error("The noteiq link is invalid. Use `noteiq://new`.")]
    InvalidUri,
}

pub fn parse_launch_request(arguments: &[String]) -> Result<LaunchRequest, LaunchError> {
    match arguments {
        [_, command] if command == "new" => Ok(LaunchRequest::NewNote),
        _ => Err(LaunchError::UnknownCommand),
    }
}

const REMINDER_URI_PREFIX: &str = "noteiq://reminder/";

impl LaunchRequest {
    pub fn from_uri(uri: &str) -> Result<Self, LaunchError> {
        if uri.eq_ignore_ascii_case("noteiq://new") || uri.eq_ignore_ascii_case("noteiq://new/") {
            return Ok(Self::NewNote);
        }
        let key = uri
            .get(..REMINDER_URI_PREFIX.len())
            .filter(|prefix| prefix.eq_ignore_ascii_case(REMINDER_URI_PREFIX))
            .map(|_| &uri[REMINDER_URI_PREFIX.len()..])
            .map(|key| key.strip_suffix('/').unwrap_or(key).to_ascii_lowercase())
            .filter(|key| is_reminder_key(key))
            .ok_or(LaunchError::InvalidUri)?;
        Ok(Self::MeetingReminder(key))
    }
}

/// The link a meeting reminder opens.
pub fn reminder_uri(key: &str) -> String {
    format!("{REMINDER_URI_PREFIX}{key}")
}

/// What a second NoteIQ process asks the running instance to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ForwardRequest {
    Show(CaptureSource),
    MeetingReminder(String),
}

/// The line a second NoteIQ process sends to the running instance.
pub fn forward_message(source: CaptureSource) -> String {
    format!("show {}", source.as_str())
}

pub fn forward_request_message(request: &ForwardRequest) -> String {
    match request {
        ForwardRequest::Show(source) => forward_message(*source),
        ForwardRequest::MeetingReminder(key) => format!("reminder {key}"),
    }
}

/// Any local process can reach the single-instance channel, so only the exact messages
/// `forward_message` produces for external launch routes are accepted.
pub fn parse_forward_message(message: &str) -> Option<CaptureSource> {
    match message.trim() {
        "show cli" => Some(CaptureSource::Cli),
        "show uri" => Some(CaptureSource::Uri),
        "show tray" => Some(CaptureSource::Tray),
        _ => None,
    }
}

/// Like [`parse_forward_message`], also accepting a reminder with a well-formed key.
pub fn parse_forward_request(message: &str) -> Option<ForwardRequest> {
    if let Some(key) = message.trim().strip_prefix("reminder ") {
        return is_reminder_key(key).then(|| ForwardRequest::MeetingReminder(key.to_owned()));
    }
    parse_forward_message(message).map(ForwardRequest::Show)
}
