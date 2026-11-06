//! Reads capture history without rewriting vault refinements.

use crate::{
    destination::Destination,
    markdown::NOTE_SCHEMA,
    model::{Capture, CaptureSource, Meeting, MeetingDecision},
};
use chrono::{DateTime, FixedOffset};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};
use thiserror::Error;
use uuid::Uuid;
use yaml_rust2::{Yaml, YamlLoader};

const MAX_NOTE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum SavedNoteError {
    #[error("The selected notes folder is unavailable.")]
    DestinationUnavailable,
    #[error("Could not read capture history: {0}")]
    Io(#[from] std::io::Error),
    #[error(
        "A saved capture has invalid metadata or an unsupported layout. Open it in your vault to inspect it."
    )]
    InvalidCapture,
}

#[derive(Clone, Debug)]
pub(crate) struct RestoredDocument {
    pub text: String,
    pub notes_start: usize,
    pub meeting: Option<Meeting>,
    pub author: Option<String>,
}

pub use crate::meeting::same_occurrence;

/// Discovery is synchronous in the core; callers must run it off their UI thread.
pub fn find_meeting_note(
    destination: &Destination,
    meeting: &Meeting,
) -> Result<Option<Capture>, SavedNoteError> {
    if !destination.root.is_dir() {
        return Err(SavedNoteError::DestinationUnavailable);
    }
    if meeting.id.trim().is_empty() {
        return Ok(None);
    }
    let history = destination.root.join("captures");
    let mut best: Option<Capture> = None;
    for year in children(&history)? {
        if !year.file_type()?.is_dir() {
            continue;
        }
        for month in children(&year.path())? {
            if !month.file_type()?.is_dir() {
                continue;
            }
            for entry in children(&month.path())? {
                if !entry.file_type()?.is_file()
                    || !entry
                        .path()
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
                {
                    continue;
                }
                if let Some(capture) = read_candidate(&entry.path(), meeting)?
                    && best.as_ref().is_none_or(|previous| {
                        (capture.updated_at, &capture.path) > (previous.updated_at, &previous.path)
                    })
                {
                    best = Some(capture);
                }
            }
        }
    }
    Ok(best)
}

fn children(path: &Path) -> Result<Vec<fs::DirEntry>, SavedNoteError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
        Ok(metadata) if metadata.file_type().is_symlink() => return Ok(Vec::new()),
        Ok(_) => {}
    }
    Ok(fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?)
}

fn read_candidate(path: &Path, meeting: &Meeting) -> Result<Option<Capture>, SavedNoteError> {
    let mut text = String::new();
    File::open(path)?
        .take(MAX_NOTE_BYTES + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > MAX_NOTE_BYTES {
        return Err(SavedNoteError::InvalidCapture);
    }
    let Some((front_start, front_end, _)) = front_matter(&text) else {
        return Ok(None);
    };
    let documents = YamlLoader::load_from_str(&text[front_start..front_end])
        .map_err(|_| SavedNoteError::InvalidCapture)?;
    if documents.len() != 1 {
        return Err(SavedNoteError::InvalidCapture);
    }
    let properties = &documents[0];
    if properties["kind"].as_str() != Some("capture") {
        return Ok(None);
    }
    if properties["calendar_event_id"].as_str() != Some(meeting.id.as_str()) {
        return Ok(None);
    }
    let start = optional_time(&properties["meeting_start"])?;
    if start != meeting.start {
        return Ok(None);
    }
    if properties["noteiq_schema"].as_i64() != Some(i64::from(NOTE_SCHEMA)) {
        return Err(SavedNoteError::InvalidCapture);
    }
    let created = required_time(&properties["captured"])?;
    let updated = required_time(&properties["updated"])?;
    let id = properties["id"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or(SavedNoteError::InvalidCapture)?;
    let timezone = properties["timezone"]
        .as_str()
        .ok_or(SavedNoteError::InvalidCapture)?;
    let source = match properties["captured_via"].as_str() {
        Some("shortcut") => CaptureSource::Shortcut,
        Some("tray") => CaptureSource::Tray,
        Some("cli") => CaptureSource::Cli,
        Some("uri") => CaptureSource::Uri,
        Some("new_note") => CaptureSource::NewNote,
        _ => return Err(SavedNoteError::InvalidCapture),
    };
    let notes_start = notes_start(&text)?;
    let mut capture = Capture::new(source, created, timezone.to_owned());
    capture.id = id;
    capture.updated_at = updated;
    capture.author = properties["from"]
        .as_str()
        .and_then(|value| value.strip_prefix("[[")?.strip_suffix("]]"))
        .map(str::to_owned);
    capture.tags = properties["tags"]
        .as_vec()
        .map(|tags| {
            tags.iter()
                .filter_map(Yaml::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    capture.body = text[notes_start..].to_owned();
    capture.meeting = Some(meeting.clone());
    capture.meeting_decision = MeetingDecision::Selected;
    capture.path = Some(PathBuf::from(path));
    capture.first_saved_at = Some(created);
    capture.restored_document = Some(RestoredDocument {
        text,
        notes_start,
        meeting: capture.meeting.clone(),
        author: capture.author.clone(),
    });
    Ok(Some(capture))
}

fn optional_time(value: &Yaml) -> Result<Option<DateTime<FixedOffset>>, SavedNoteError> {
    if value.is_badvalue() || value.is_null() {
        return Ok(None);
    }
    required_time(value).map(Some)
}

fn required_time(value: &Yaml) -> Result<DateTime<FixedOffset>, SavedNoteError> {
    value
        .as_str()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .ok_or(SavedNoteError::InvalidCapture)
}

pub(crate) fn front_matter(text: &str) -> Option<(usize, usize, usize)> {
    let bom = usize::from(text.starts_with('\u{feff}')) * '\u{feff}'.len_utf8();
    let mut lines = text[bom..].split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end() != "---" {
        return None;
    }
    let start = bom + first.len();
    let mut offset = start;
    for line in lines {
        if line.trim_end() == "---" {
            return Some((start, offset, offset + line.len()));
        }
        offset += line.len();
    }
    None
}

/// Finds headings outside fenced code. Notes is the final editable region, so headings
/// typed inside it are preserved as body text rather than treated as generated sections.
pub(crate) fn headings(text: &str) -> Vec<(usize, usize, &str)> {
    let mut headings = Vec::new();
    let mut offset = front_matter(text).map_or(0, |(_, _, body_start)| body_start);
    let mut fence: Option<(char, usize)> = None;
    for line in text[offset..].split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        let indentation = trimmed.len() - trimmed.trim_start_matches(' ').len();
        let candidate = &trimmed[indentation..];
        if indentation <= 3 {
            let character = candidate.chars().next().unwrap_or(' ');
            let length = candidate.chars().take_while(|c| *c == character).count();
            if let Some((marker, count)) = fence {
                if character == marker && length >= count && candidate[length..].trim().is_empty() {
                    fence = None;
                }
            } else if matches!(character, '`' | '~') && length >= 3 {
                fence = Some((character, length));
            } else if candidate.starts_with("# ") || candidate.starts_with("## ") {
                headings.push((offset, offset + line.len(), candidate));
            }
        }
        offset += line.len();
    }
    headings
}

fn notes_start(text: &str) -> Result<usize, SavedNoteError> {
    let (_, _, body_start) = front_matter(text).ok_or(SavedNoteError::InvalidCapture)?;
    let matches = headings(&text[body_start..])
        .into_iter()
        .filter(|(_, _, heading)| *heading == "## Notes")
        .collect::<Vec<_>>();
    let [(_, end, _)] = matches.as_slice() else {
        return Err(SavedNoteError::InvalidCapture);
    };
    let mut start = body_start + *end;
    if text[start..].starts_with("\r\n") {
        start += 2;
    } else if text[start..].starts_with('\n') {
        start += 1;
    }
    Ok(start)
}

const MEETING_KEYS: [&str; 11] = [
    "meeting",
    "meeting_start",
    "meeting_end",
    "organizer",
    "your_rsvp",
    "required_invitees",
    "optional_invitees",
    "resource_invitees",
    "other_invitees",
    "calendar_event_id",
    "meeting_series_id",
];

fn property_blocks(front: &str) -> Vec<(&str, &str)> {
    let mut blocks = Vec::new();
    let mut offset = 0;
    let mut current: Option<(&str, usize)> = None;
    for line in front.split_inclusive('\n') {
        if let Some(key) = property_key(line) {
            if let Some((key, start)) = current {
                blocks.push((key, &front[start..offset]));
            } else if offset > 0 {
                blocks.push(("", &front[..offset]));
            }
            current = Some((key, offset));
        }
        offset += line.len();
    }
    if let Some((key, start)) = current {
        blocks.push((key, &front[start..]));
    } else {
        blocks.push(("", front));
    }
    blocks
}

fn property_key(line: &str) -> Option<&str> {
    if line.starts_with(char::is_whitespace) || line.starts_with('#') {
        return None;
    }
    let mut quote = None;
    let mut escaped = false;
    for (offset, character) in line.char_indices() {
        if escaped {
            escaped = false;
        } else if quote == Some('"') && character == '\\' {
            escaped = true;
        } else if quote == Some(character) {
            quote = None;
        } else if quote.is_none() && matches!(character, '\'' | '"') {
            quote = Some(character);
        } else if quote.is_none() && character == ':' {
            return Some(line[..offset].trim().trim_matches(['\'', '"']));
        }
    }
    None
}

pub(crate) fn render_restored(capture: &Capture, fresh: &str) -> String {
    let document = capture
        .restored_document
        .as_ref()
        .expect("restored document");
    let newline = if document.text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut prefix = document.text[..document.notes_start].to_owned();
    let changed_meeting = document.meeting != capture.meeting;
    let changed_author = document.author != capture.author;
    let (start, end, _) = front_matter(&prefix).expect("validated front matter");
    let (fresh_start, fresh_end, _) = front_matter(fresh).expect("generated front matter");
    let new_properties = property_blocks(&fresh[fresh_start..fresh_end]);
    let mut properties = String::new();
    for (key, block) in property_blocks(&prefix[start..end]) {
        if key == "updated" {
            let replacement = new_properties
                .iter()
                .find(|(key, _)| *key == "updated")
                .unwrap()
                .1;
            properties.push_str(&replacement.replace('\n', newline));
            if let Some((_, remainder)) = block.split_once('\n') {
                properties.push_str(remainder);
            }
        } else if !(changed_meeting && MEETING_KEYS.contains(&key)
            || changed_author && key == "from")
        {
            properties.push_str(block);
        } else {
            for line in block
                .split_inclusive('\n')
                .filter(|line| line.trim_start().starts_with('#'))
            {
                properties.push_str(line);
            }
        }
    }
    if changed_author
        && let Some((_, block)) = new_properties.iter().find(|(key, _)| *key == "from")
    {
        properties.push_str(&block.replace('\n', newline));
    }
    if changed_meeting {
        for (key, block) in &new_properties {
            if MEETING_KEYS.contains(key) {
                properties.push_str(&block.replace('\n', newline));
            }
        }
    }
    prefix.replace_range(start..end, &properties);
    if changed_author {
        let attribution = |text: &str| {
            text.match_indices("\n**")
                .find(|(start, _)| {
                    text[*start..].starts_with("\n**From:**")
                        || text[*start..].starts_with("\n**Captured:**")
                })
                .map(|(start, _)| {
                    (
                        start + 1,
                        text[start + 1..]
                            .find('\n')
                            .map_or(text.len(), |end| start + 1 + end),
                    )
                })
        };
        if let (Some((start, end)), Some((fresh_start, fresh_end))) =
            (attribution(&prefix), attribution(fresh))
        {
            prefix.replace_range(start..end, &fresh[fresh_start..fresh_end]);
        }
    }
    if changed_meeting {
        let sections = headings(&prefix);
        if let Some(index) = sections
            .iter()
            .position(|(_, _, heading)| *heading == "## Meeting details")
        {
            let start = sections[index].0;
            let end = sections
                .get(index + 1)
                .map_or(prefix.len(), |section| section.0);
            prefix.replace_range(start..end, "");
        }
        let sections = headings(fresh);
        if let Some(index) = sections
            .iter()
            .position(|(_, _, heading)| *heading == "## Meeting details")
        {
            let start = sections[index].0;
            let end = sections
                .get(index + 1)
                .map_or(fresh.len(), |section| section.0);
            let insert = headings(&prefix)
                .into_iter()
                .find(|(_, _, heading)| *heading == "## Summary")
                .map(|section| section.0)
                .unwrap_or(prefix.len());
            prefix.insert_str(insert, &fresh[start..end].replace('\n', newline));
        }
    }
    prefix.push_str(&capture.body);
    if !capture.body.is_empty() && !capture.body.ends_with('\n') {
        prefix.push_str(newline);
    }
    prefix
}

pub(crate) fn advance_snapshot(capture: &mut Capture, contents: String) {
    if let Some(document) = capture.restored_document.as_mut() {
        document.notes_start = contents.len()
            - capture.body.len()
            - usize::from(!capture.body.is_empty() && !capture.body.ends_with('\n'))
                * if document.text.contains("\r\n") { 2 } else { 1 };
        document.text = contents;
        document.meeting = capture.meeting.clone();
        document.author = capture.author.clone();
    }
}
