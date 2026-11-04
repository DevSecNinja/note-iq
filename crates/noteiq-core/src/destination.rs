use crate::{
    diagnostics::FailureClass,
    markdown::{MarkdownError, render_markdown},
    model::Capture,
};
use chrono::{DateTime, FixedOffset};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use thiserror::Error;
use uuid::Uuid;

const MAX_SLUG_LENGTH: usize = 48;
const MAX_COLLISION_ATTEMPTS: u32 = 10_000;

#[derive(Debug, Error)]
pub enum DestinationError {
    #[error("The selected notes folder is not available: {0}")]
    RootUnavailable(PathBuf),
    #[error("The saved note changed outside NoteIQ: {0}")]
    ExternalConflict(PathBuf),
    #[error("File operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Markdown generation failed: {0}")]
    Markdown(#[from] MarkdownError),
    #[error("No unused filename could be allocated in the selected folder.")]
    NoAvailableName,
}

impl DestinationError {
    pub fn failure_class(&self) -> FailureClass {
        match self {
            Self::RootUnavailable(_) => FailureClass::DestinationUnavailable,
            Self::ExternalConflict(_) => FailureClass::ExternalConflict,
            Self::Io(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                FailureClass::PermissionDenied
            }
            Self::Io(_) | Self::Markdown(_) | Self::NoAvailableName => FailureClass::Unknown,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Destination {
    pub root: PathBuf,
}

impl Destination {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn capture_folder(&self, now: DateTime<FixedOffset>) -> PathBuf {
        self.root
            .join("captures")
            .join(now.format("%Y").to_string())
            .join(now.format("%m").to_string())
    }
}

pub fn capture_path(
    capture: &Capture,
    destination: &Destination,
    now: DateTime<FixedOffset>,
    collision_number: u32,
) -> PathBuf {
    let slug = slug_from_body(&capture.body);
    let suffix = if collision_number > 1 {
        format!("-{collision_number}")
    } else {
        String::new()
    };
    destination.capture_folder(now).join(format!(
        "{}_{}{}{}.md",
        now.format("%Y-%m-%d_%H%M"),
        slug,
        suffix,
        ""
    ))
}

fn slug_from_body(body: &str) -> String {
    let title = body
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("note")
        .trim_start_matches('#')
        .trim();

    let mut slug = String::new();
    let mut separator_pending = false;
    for character in title.chars() {
        if character.is_alphanumeric() {
            if separator_pending && !slug.is_empty() {
                slug.push('-');
            }
            separator_pending = false;
            slug.extend(character.to_lowercase());
            if slug.chars().count() >= MAX_SLUG_LENGTH {
                break;
            }
        } else {
            separator_pending = true;
        }
    }

    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "note" } else { slug };
    let reserved = matches!(
        slug.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    if reserved {
        format!("note-{slug}")
    } else {
        slug.to_owned()
    }
}

fn file_belongs_to(path: &Path, note_id: Uuid) -> Result<bool, std::io::Error> {
    let mut file = File::open(path)?;
    let mut prefix = String::new();
    Read::by_ref(&mut file)
        .take(1024)
        .read_to_string(&mut prefix)?;
    Ok(prefix.lines().any(|line| {
        line.strip_prefix("id: ")
            .is_some_and(|value| value == format!("\"{note_id}\""))
    }))
}

fn next_unused_path(
    capture: &Capture,
    destination: &Destination,
    now: DateTime<FixedOffset>,
) -> Result<PathBuf, DestinationError> {
    let folder = destination.capture_folder(now);
    fs::create_dir_all(&folder)?;
    for number in 1..=MAX_COLLISION_ATTEMPTS {
        let candidate = capture_path(capture, destination, now, number);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                drop(file);
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(DestinationError::NoAvailableName)
}

fn replace_atomically(path: &Path, contents: &[u8]) -> Result<(), DestinationError> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "note path has no parent")
    })?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| DestinationError::Io(error.error))?;
    Ok(())
}

pub fn save_capture(
    capture: &mut Capture,
    destination: &Destination,
    now: DateTime<FixedOffset>,
) -> Result<Option<PathBuf>, DestinationError> {
    if capture.is_empty() && capture.path.is_none() {
        return Ok(None);
    }
    if !destination.root.is_dir() {
        return Err(DestinationError::RootUnavailable(destination.root.clone()));
    }

    let is_new = capture.path.is_none();
    let path = match &capture.path {
        Some(path) => path.clone(),
        None => next_unused_path(capture, destination, now)?,
    };
    if !is_new && path.exists() && !file_belongs_to(&path, capture.id)? {
        return Err(DestinationError::ExternalConflict(path));
    }

    let result = (|| {
        let contents = render_markdown(capture, now)?;
        replace_atomically(&path, contents.as_bytes())
    })();

    if let Err(error) = result {
        if is_new {
            let _ = fs::remove_file(&path);
        }
        return Err(error);
    }

    capture.path = Some(path.clone());
    capture.updated_at = now;
    Ok(Some(path))
}

pub fn finish_capture(capture: &mut Capture) -> Result<(), DestinationError> {
    if capture.is_empty()
        && let Some(path) = capture.path.take()
        && path.exists()
    {
        if !file_belongs_to(&path, capture.id)? {
            capture.path = Some(path.clone());
            return Err(DestinationError::ExternalConflict(path));
        }
        fs::remove_file(path)?;
    }
    Ok(())
}
