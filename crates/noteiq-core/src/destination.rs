use crate::{
    diagnostics::FailureClass,
    markdown::{MarkdownError, render_markdown},
    model::Capture,
    note_filename::{NoteFilename, is_reserved_windows_name},
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
    /// Template for notes saved for the first time; saved notes keep the one they started with.
    pub note_filename: NoteFilename,
}

impl Destination {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            note_filename: NoteFilename::default(),
        }
    }

    pub fn with_note_filename(mut self, note_filename: NoteFilename) -> Self {
        self.note_filename = note_filename;
        self
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
    destination.capture_folder(now).join(capture_filename(
        capture,
        filename_template(capture, destination),
        now,
        collision_number,
    ))
}

fn filename_template<'a>(capture: &'a Capture, destination: &'a Destination) -> &'a NoteFilename {
    capture
        .filename_template
        .as_ref()
        .unwrap_or(&destination.note_filename)
}

fn meeting_slug(capture: &Capture) -> Option<String> {
    capture
        .meeting
        .as_ref()
        .and_then(|meeting| meeting.title.as_deref())
        .filter(|title| title.chars().any(char::is_alphanumeric))
        .map(slug_from_text)
}

fn capture_filename(
    capture: &Capture,
    template: &NoteFilename,
    at: DateTime<FixedOffset>,
    collision_number: u32,
) -> String {
    let slug = meeting_slug(capture).unwrap_or_else(|| slug_from_text(&capture.body));
    template.render(at, &slug, collision_number)
}

fn slug_from_text(text: &str) -> String {
    let title = text
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
    if is_reserved_windows_name(slug) {
        format!("note-{slug}")
    } else {
        slug.to_owned()
    }
}

/// Confirms a file is this note by finding its exact `id` property in the front matter.
/// Reads raw bytes so a multibyte character cut at the read limit cannot fail the check.
fn file_belongs_to(path: &Path, note_id: Uuid) -> Result<bool, std::io::Error> {
    const MAX_FRONT_MATTER_BYTES: u64 = 1 << 20;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_FRONT_MATTER_BYTES)
        .read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return Ok(false);
    }
    let expected = format!("id: \"{note_id}\"");
    for line in lines {
        let line = line.trim_end();
        if line == "---" {
            break;
        }
        if line == expected {
            return Ok(true);
        }
    }
    Ok(false)
}

fn next_unused_path(
    capture: &Capture,
    template: &NoteFilename,
    destination: &Destination,
    now: DateTime<FixedOffset>,
) -> Result<PathBuf, DestinationError> {
    let folder = match capture.path.as_ref() {
        Some(path) => path
            .parent()
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "note has no folder")
            })?
            .to_path_buf(),
        None => destination.capture_folder(now),
    };
    fs::create_dir_all(&folder)?;
    for number in 1..=MAX_COLLISION_ATTEMPTS {
        let candidate = folder.join(capture_filename(
            capture,
            template,
            capture.first_saved_at.unwrap_or(now),
            number,
        ));
        if capture.path.as_ref() == Some(&candidate) {
            return Ok(candidate);
        }
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

    let previous_path = capture.path.clone();
    check_restored_snapshot(capture)?;
    if capture.restored_document.is_none()
        && let Some(path) = previous_path.as_ref()
        && path.exists()
        && !file_belongs_to(path, capture.id)?
    {
        return Err(DestinationError::ExternalConflict(path.clone()));
    }
    let template = filename_template(capture, destination).clone();
    let current_meeting_slug = meeting_slug(capture);
    let path = match previous_path.as_ref() {
        Some(path)
            if capture.restored_document.is_some()
                || current_meeting_slug == capture.filename_meeting_slug
                || !template.uses_title() =>
        {
            path.clone()
        }
        _ => next_unused_path(capture, &template, destination, now)?,
    };
    let reserved_new_path = previous_path.as_ref() != Some(&path);

    let result = (|| {
        let contents = render_markdown(capture, now)?;
        let write_path = previous_path.as_ref().unwrap_or(&path);
        replace_atomically(write_path, contents.as_bytes())?;
        if write_path != &path {
            // Move the complete file over our reserved name; a failed rename leaves the
            // updated note at its original path so the next save can retry safely.
            fs::rename(write_path, &path)?;
        }
        Ok(contents)
    })();

    let contents = match result {
        Ok(contents) => contents,
        Err(error) => {
            if reserved_new_path && let Err(cleanup_error) = fs::remove_file(&path) {
                tracing::warn!(
                    failure_class = "filename_reservation_cleanup_failed",
                    error_kind = ?cleanup_error.kind(),
                    "Could not remove an unused filename reservation"
                );
            }
            return Err(error);
        }
    };

    capture.path = Some(path.clone());
    capture.updated_at = now;
    capture.first_saved_at.get_or_insert(now);
    capture.filename_meeting_slug = current_meeting_slug;
    capture.filename_template.get_or_insert(template);
    crate::saved_note::advance_snapshot(capture, contents);
    Ok(Some(path))
}

pub fn finish_capture(capture: &mut Capture) -> Result<(), DestinationError> {
    check_restored_snapshot(capture)?;
    if capture.restored_document.is_some() {
        return Ok(());
    }
    if capture.is_empty()
        && let Some(path) = capture.path.as_ref()
    {
        match fs::symlink_metadata(path) {
            Ok(_) => {
                if !file_belongs_to(path, capture.id)? {
                    return Err(DestinationError::ExternalConflict(path.clone()));
                }
                fs::remove_file(path)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        capture.path = None;
    }
    Ok(())
}

fn check_restored_snapshot(capture: &Capture) -> Result<(), DestinationError> {
    if let (Some(document), Some(path)) = (&capture.restored_document, &capture.path) {
        match fs::read_to_string(path) {
            Ok(contents) if contents == document.text => {}
            Ok(_) => return Err(DestinationError::ExternalConflict(path.clone())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(DestinationError::ExternalConflict(path.clone()));
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
