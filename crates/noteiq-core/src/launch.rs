use crate::model::CaptureSource;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchRequest {
    NewNote,
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

impl LaunchRequest {
    pub fn from_uri(uri: &str) -> Result<Self, LaunchError> {
        if uri.eq_ignore_ascii_case("noteiq://new") || uri.eq_ignore_ascii_case("noteiq://new/") {
            Ok(Self::NewNote)
        } else {
            Err(LaunchError::InvalidUri)
        }
    }
}

/// The line a second NoteIQ process sends to the running instance.
pub fn forward_message(source: CaptureSource) -> String {
    format!("show {}", source.as_str())
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
