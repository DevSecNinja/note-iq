//! Built-in and user-defined note templates for meeting notes.
//!
//! A template is Markdown with optional `{title}`, `{date}`, `{time}`, `{organizer}` and
//! `{attendees}` placeholders. User templates are stored as `<name>.md` files in a `templates`
//! folder next to the settings file, so they are easy to edit and sync.

use crate::markdown::contact_name;
use crate::model::{Capture, InviteeRole, Meeting, Rsvp};
use crate::note_filename::is_reserved_windows_name;
use chrono::{DateTime, FixedOffset};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use thiserror::Error;

/// The template used when none is configured or the configured one is unavailable.
pub const DEFAULT_TEMPLATE_ID: &str = "general";
/// The folder, next to the settings file, that holds user templates.
pub const TEMPLATES_FOLDER: &str = "templates";
/// The placeholders a template may use.
pub const PLACEHOLDERS: [&str; 5] = ["title", "date", "time", "organizer", "attendees"];
pub const MAX_NAME_LENGTH: usize = 60;
pub const MAX_BODY_LENGTH: usize = 10_000;
/// Characters Windows or Obsidian links cannot use in a file name.
const INVALID_CHARACTERS: [char; 11] = ['<', '>', ':', '"', '|', '?', '*', '#', '^', '[', ']'];
const EXTENSION: &str = "md";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeetingTemplate {
    /// `general` and the other built-in ids, or the file name (without `.md`) of a user template.
    pub id: String,
    pub name: String,
    pub body: String,
    /// Built-in templates are read-only; duplicate one to change it.
    pub built_in: bool,
}

impl MeetingTemplate {
    /// The template text with placeholders filled in from `meeting`. Without a meeting, `{date}`
    /// and `{time}` use `now` and the other placeholders are left empty. Unknown placeholders
    /// in hand-edited files are kept as typed.
    pub fn render(&self, meeting: Option<&Meeting>, now: DateTime<FixedOffset>) -> String {
        render_template(&self.body, meeting, now)
    }
}

const GENERAL_BODY: &str =
    "### Agenda\n\n- \n\n### Notes\n\n- \n\n### Decisions\n\n- \n\n### Action items\n\n- [ ] \n";
const ONE_ON_ONE_BODY: &str =
    "### Updates\n\n- \n\n### Discussion\n\n- \n\n### Feedback\n\n- \n\n### Follow-ups\n\n- [ ] \n";
const CUSTOMER_CALL_BODY: &str = "### Attendees\n\n{attendees}\n\n### Context\n\n- \n\n### Asks\n\n- \n\n### Next steps\n\n- [ ] \n";
const STAND_UP_BODY: &str = "### Yesterday\n\n- \n\n### Today\n\n- \n\n### Blockers\n\n- \n";

const BUILT_INS: [(&str, &str, &str); 4] = [
    (DEFAULT_TEMPLATE_ID, "General meeting", GENERAL_BODY),
    ("one-on-one", "1:1", ONE_ON_ONE_BODY),
    ("customer-call", "Customer call", CUSTOMER_CALL_BODY),
    ("stand-up", "Stand-up", STAND_UP_BODY),
];

/// The read-only templates that ship with NoteIQ, General meeting first.
pub fn built_in_templates() -> Vec<MeetingTemplate> {
    BUILT_INS
        .iter()
        .map(|(id, name, body)| MeetingTemplate {
            id: (*id).to_owned(),
            name: (*name).to_owned(),
            body: (*body).to_owned(),
            built_in: true,
        })
        .collect()
}

/// The General meeting template, used whenever another template is unavailable.
pub fn general_template() -> MeetingTemplate {
    built_in_templates()
        .into_iter()
        .next()
        .expect("General meeting is built in")
}

fn built_in(id: &str) -> Option<MeetingTemplate> {
    built_in_templates()
        .into_iter()
        .find(|template| template.id == id)
}

fn is_built_in_name(name: &str) -> bool {
    BUILT_INS.iter().any(|(id, built_in_name, _)| {
        name.eq_ignore_ascii_case(id) || name.eq_ignore_ascii_case(built_in_name)
    })
}

#[derive(Debug, Error)]
pub enum MeetingTemplateError {
    #[error("Enter a template name.")]
    EmptyName,
    #[error("Keep the template name to {max} characters or fewer.", max = MAX_NAME_LENGTH)]
    NameTooLong,
    #[error("Enter a name, not a folder path.")]
    PathSeparator,
    #[error("Remove the {0} character from the name. Windows can't use it in file names.")]
    InvalidCharacter(char),
    #[error("Remove tabs and other control characters from the name.")]
    ControlCharacter,
    #[error("Template names can't start or end with a dot.")]
    EdgeDot,
    #[error("\"{0}\" is reserved by Windows. Choose another name.")]
    ReservedName(String),
    #[error("\"{0}\" is a built-in template name. Choose another name.")]
    BuiltInName(String),
    #[error("A template named \"{0}\" already exists.")]
    DuplicateName(String),
    #[error("Built-in templates can't be changed. Duplicate it to make your own.")]
    ReadOnly,
    #[error("The template no longer exists.")]
    NotFound,
    #[error("Add some text to the template.")]
    EmptyBody,
    #[error("Keep the template to {max} characters or fewer.", max = MAX_BODY_LENGTH)]
    BodyTooLong,
    #[error(
        "Unknown placeholder: {{{0}}}. Use {{title}}, {{date}}, {{time}}, {{organizer}}, or {{attendees}}."
    )]
    UnknownPlaceholder(String),
    #[error(
        "Use ### or smaller headings, such as ### Agenda. \"{0}\" would clash with the note's own sections."
    )]
    TopLevelHeading(String),
    #[error("The template file could not be saved: {0}")]
    Io(#[from] std::io::Error),
}

impl MeetingTemplateError {
    /// A stable, content-free name for diagnostics.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::EmptyName => "empty_name",
            Self::NameTooLong => "name_too_long",
            Self::PathSeparator => "path_separator",
            Self::InvalidCharacter(_) => "invalid_character",
            Self::ControlCharacter => "control_character",
            Self::EdgeDot => "edge_dot",
            Self::ReservedName(_) => "reserved_name",
            Self::BuiltInName(_) => "built_in_name",
            Self::DuplicateName(_) => "duplicate_name",
            Self::ReadOnly => "read_only",
            Self::NotFound => "not_found",
            Self::EmptyBody => "empty_body",
            Self::BodyTooLong => "body_too_long",
            Self::UnknownPlaceholder(_) => "unknown_placeholder",
            Self::TopLevelHeading(_) => "top_level_heading",
            Self::Io(_) => "io",
        }
    }
}

/// Checks a user template name, which is also its file name. Returns the trimmed name.
pub fn validate_name(name: &str) -> Result<String, MeetingTemplateError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(MeetingTemplateError::EmptyName);
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        return Err(MeetingTemplateError::NameTooLong);
    }
    for character in name.chars() {
        match character {
            '/' | '\\' => return Err(MeetingTemplateError::PathSeparator),
            character if character.is_control() => {
                return Err(MeetingTemplateError::ControlCharacter);
            }
            character if INVALID_CHARACTERS.contains(&character) => {
                return Err(MeetingTemplateError::InvalidCharacter(character));
            }
            _ => {}
        }
    }
    if name.starts_with('.') || name.ends_with('.') {
        return Err(MeetingTemplateError::EdgeDot);
    }
    if is_reserved_windows_name(name) {
        return Err(MeetingTemplateError::ReservedName(name.to_owned()));
    }
    if is_built_in_name(name) {
        return Err(MeetingTemplateError::BuiltInName(name.to_owned()));
    }
    Ok(name.to_owned())
}

/// Checks template text before it is saved: unknown placeholders are rejected so typos such
/// as `{titel}` are caught while editing, and `#` or `##` headings are rejected because a
/// saved note reads them as its own sections, such as `## Notes`.
pub fn validate_body(body: &str) -> Result<(), MeetingTemplateError> {
    if body.trim().is_empty() {
        return Err(MeetingTemplateError::EmptyBody);
    }
    if body.chars().count() > MAX_BODY_LENGTH {
        return Err(MeetingTemplateError::BodyTooLong);
    }
    if let Some(&(start, end, _)) = top_level_headings(body).first() {
        return Err(MeetingTemplateError::TopLevelHeading(
            body[start..end].trim().to_owned(),
        ));
    }
    match segments(body)
        .into_iter()
        .find_map(|segment| match segment {
            Segment::Unknown(name) => Some(name),
            _ => None,
        }) {
        Some(text) => Err(MeetingTemplateError::UnknownPlaceholder(
            text.trim_matches(['{', '}']).trim().to_owned(),
        )),
        None => Ok(()),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Placeholder {
    Title,
    Date,
    Time,
    Organizer,
    Attendees,
}

enum Segment<'a> {
    Text(&'a str),
    Placeholder(Placeholder),
    /// A `{name}` that is not a placeholder; kept as typed when rendering.
    Unknown(&'a str),
}

/// Splits template text into literal text and placeholders. Only `{word}` is treated as a
/// placeholder (ignoring case and surrounding spaces); other braces, such as `{}` or code,
/// are literal text.
fn segments(body: &str) -> Vec<Segment<'_>> {
    let mut segments = Vec::new();
    let mut literal_start = 0;
    let mut position = 0;
    while let Some(offset) = body[position..].find('{') {
        let open = position + offset;
        let Some(length) = body[open + 1..].find(['{', '}', '\n']) else {
            break;
        };
        let close = open + 1 + length;
        if !body[close..].starts_with('}') {
            position = close;
            continue;
        }
        let name = body[open + 1..close].trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            position = close + 1;
            continue;
        }
        if literal_start < open {
            segments.push(Segment::Text(&body[literal_start..open]));
        }
        segments.push(match name.to_ascii_lowercase().as_str() {
            "title" => Segment::Placeholder(Placeholder::Title),
            "date" => Segment::Placeholder(Placeholder::Date),
            "time" => Segment::Placeholder(Placeholder::Time),
            "organizer" => Segment::Placeholder(Placeholder::Organizer),
            "attendees" => Segment::Placeholder(Placeholder::Attendees),
            _ => Segment::Unknown(&body[open..=close]),
        });
        position = close + 1;
        literal_start = position;
    }
    if literal_start < body.len() {
        segments.push(Segment::Text(&body[literal_start..]));
    }
    segments
}

/// Renders template text for `meeting`; see [`MeetingTemplate::render`].
pub fn render_template(
    body: &str,
    meeting: Option<&Meeting>,
    now: DateTime<FixedOffset>,
) -> String {
    let at = meeting
        .and_then(|meeting| meeting.start)
        .map_or(now, |start| start.with_timezone(now.offset()));
    let mut output = String::with_capacity(body.len());
    for segment in segments(body) {
        match segment {
            Segment::Text(text) => output.push_str(text),
            Segment::Unknown(text) => output.push_str(text),
            Segment::Placeholder(Placeholder::Date) => {
                output.push_str(&at.format("%Y-%m-%d").to_string());
            }
            Segment::Placeholder(Placeholder::Time) => {
                output.push_str(&at.format("%H:%M").to_string());
            }
            Segment::Placeholder(Placeholder::Title) => {
                if let Some(title) = meeting.and_then(|meeting| meeting.title.as_deref()) {
                    output.push_str(title.trim());
                }
            }
            Segment::Placeholder(Placeholder::Organizer) => {
                if let Some(organizer) = meeting.and_then(|meeting| meeting.organizer.as_deref()) {
                    output.push_str(contact_name(organizer.trim()));
                }
            }
            Segment::Placeholder(Placeholder::Attendees) => {
                if let Some(meeting) = meeting {
                    output.push_str(&attendees(meeting));
                }
            }
        }
    }
    demote_top_level_headings(&output)
}

/// `#` and `##` headings outside fenced code, which saved notes treat as their own sections,
/// as `(line start, line end, offset of the first #)`. Mirrors the saved-note parser.
fn top_level_headings(text: &str) -> Vec<(usize, usize, usize)> {
    let mut headings = Vec::new();
    let mut offset = 0;
    let mut fence: Option<(char, usize)> = None;
    for line in text.split_inclusive('\n') {
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
                headings.push((offset, offset + line.len(), offset + indentation));
            }
        }
        offset += line.len();
    }
    headings
}

/// Turns `#` and `##` headings into `###`, so a hand-edited template or a meeting name can
/// never add a section that stops the saved note from being reopened.
fn demote_top_level_headings(text: &str) -> String {
    let mut output = text.to_owned();
    for (_, _, hashes) in top_level_headings(text).into_iter().rev() {
        let count = output[hashes..].chars().take_while(|c| *c == '#').count();
        output.replace_range(hashes..hashes + count, "###");
    }
    output
}

/// The people invited to `meeting`, excluding rooms and the organizer, by name.
fn attendees(meeting: &Meeting) -> String {
    meeting
        .invitees
        .iter()
        .filter(|invitee| invitee.role != InviteeRole::Resource && invitee.rsvp != Rsvp::Organizer)
        .filter_map(|invitee| {
            [invitee.name.as_deref(), invitee.email.as_deref()]
                .into_iter()
                .flatten()
                .map(str::trim)
                .find(|value| !value.is_empty())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// User templates on disk, plus the built-ins.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateStore {
    folder: PathBuf,
}

impl TemplateStore {
    pub fn new(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
        }
    }

    /// The `templates` folder next to `settings_path`.
    pub fn beside_settings(settings_path: &Path) -> Self {
        let parent = settings_path.parent().unwrap_or_else(|| Path::new(""));
        Self::new(parent.join(TEMPLATES_FOLDER))
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// The user's templates, sorted by name. A missing folder means none; files with names
    /// that are not valid template names, or that clash with a built-in, are skipped.
    pub fn user_templates(&self) -> Result<Vec<MeetingTemplate>, MeetingTemplateError> {
        let entries = match fs::read_dir(&self.folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut templates = Vec::new();
        for entry in entries {
            let path = entry?.path();
            let is_markdown = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case(EXTENSION));
            if !is_markdown || !path.is_file() {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let name = match validate_name(stem) {
                Ok(name) if name == stem => name,
                Ok(_) => continue,
                Err(error) => {
                    tracing::warn!(
                        failure_class = "meeting_template_skipped",
                        error_kind = error.kind(),
                        "Skipping a template file with an unusable name"
                    );
                    continue;
                }
            };
            match read_body(&path) {
                Ok(body) => templates.push(MeetingTemplate {
                    id: name.clone(),
                    name,
                    body,
                    built_in: false,
                }),
                Err(error) => tracing::warn!(
                    failure_class = "meeting_template_unreadable",
                    error_kind = ?error.kind(),
                    "Skipping a template file that could not be read"
                ),
            }
        }
        templates.sort_by_key(|template| template.name.to_lowercase());
        Ok(templates)
    }

    /// Built-in templates, then the user's. User templates that cannot be read are left out
    /// rather than hiding the built-ins.
    pub fn list(&self) -> Vec<MeetingTemplate> {
        let mut templates = built_in_templates();
        match self.user_templates() {
            Ok(user) => templates.extend(user),
            Err(error) => tracing::warn!(
                failure_class = "meeting_templates_unreadable",
                error_kind = error.kind(),
                "Could not list user templates"
            ),
        }
        templates
    }

    /// The template with `id`, if it exists.
    pub fn get(&self, id: &str) -> Result<Option<MeetingTemplate>, MeetingTemplateError> {
        if let Some(template) = built_in(id) {
            return Ok(Some(template));
        }
        let Some(path) = self.path_for(id) else {
            return Ok(None);
        };
        match read_body(&path) {
            Ok(body) => Ok(Some(MeetingTemplate {
                id: id.to_owned(),
                name: id.to_owned(),
                body,
                built_in: false,
            })),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// The template with `id`, falling back to General meeting when it is missing or cannot
    /// be read, so a broken default never blocks taking notes.
    pub fn resolve(&self, id: &str) -> MeetingTemplate {
        match self.get(id) {
            Ok(Some(template)) => template,
            Ok(None) => {
                tracing::warn!(
                    failure_class = "invalid_meeting_template",
                    error_kind = "not_found",
                    "Using the General meeting template because the default template is missing"
                );
                general_template()
            }
            Err(error) => {
                tracing::warn!(
                    failure_class = "invalid_meeting_template",
                    error_kind = error.kind(),
                    "Using the General meeting template because the default template is unreadable"
                );
                general_template()
            }
        }
    }

    /// Creates a user template. The name must be unused, ignoring case.
    pub fn create(&self, name: &str, body: &str) -> Result<MeetingTemplate, MeetingTemplateError> {
        let name = validate_name(name)?;
        let body = normalize(body);
        validate_body(&body)?;
        self.ensure_unused(&name, None)?;
        fs::create_dir_all(&self.folder)?;
        let path = self.folder.join(file_name(&name));
        let mut temporary = NamedTempFile::new_in(&self.folder)?;
        temporary.write_all(body.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(&path).map_err(|error| {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                MeetingTemplateError::DuplicateName(name.clone())
            } else {
                MeetingTemplateError::Io(error.error)
            }
        })?;
        Ok(MeetingTemplate {
            id: name.clone(),
            name,
            body,
            built_in: false,
        })
    }

    /// Saves new text for the user template `id`, renaming it when `name` differs.
    pub fn update(
        &self,
        id: &str,
        name: &str,
        body: &str,
    ) -> Result<MeetingTemplate, MeetingTemplateError> {
        if built_in(id).is_some() {
            return Err(MeetingTemplateError::ReadOnly);
        }
        let old_path = self.path_for(id).ok_or(MeetingTemplateError::NotFound)?;
        if !old_path.is_file() {
            return Err(MeetingTemplateError::NotFound);
        }
        let name = validate_name(name)?;
        let body = normalize(body);
        validate_body(&body)?;
        self.ensure_unused(&name, Some(id))?;

        let mut temporary = NamedTempFile::new_in(&self.folder)?;
        temporary.write_all(body.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&old_path)
            .map_err(|error| MeetingTemplateError::Io(error.error))?;
        if name != id {
            fs::rename(&old_path, self.folder.join(file_name(&name)))?;
        }
        Ok(MeetingTemplate {
            id: name.clone(),
            name,
            body,
            built_in: false,
        })
    }

    /// Deletes the user template `id`.
    pub fn delete(&self, id: &str) -> Result<(), MeetingTemplateError> {
        if built_in(id).is_some() {
            return Err(MeetingTemplateError::ReadOnly);
        }
        let path = self.path_for(id).ok_or(MeetingTemplateError::NotFound)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(MeetingTemplateError::NotFound)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// The file for a user template id; `None` for ids that are not valid names, so an id can
    /// never reach outside the templates folder.
    fn path_for(&self, id: &str) -> Option<PathBuf> {
        match validate_name(id) {
            Ok(name) if name == id => Some(self.folder.join(file_name(id))),
            _ => None,
        }
    }

    /// Rejects `name` when another template already uses it, ignoring case. `renaming` is the
    /// template being renamed, which may keep its own name in a different case.
    fn ensure_unused(
        &self,
        name: &str,
        renaming: Option<&str>,
    ) -> Result<(), MeetingTemplateError> {
        if renaming.is_some_and(|id| id.to_lowercase() == name.to_lowercase()) {
            return Ok(());
        }
        let taken = self
            .user_templates()?
            .iter()
            .any(|template| template.name.to_lowercase() == name.to_lowercase())
            || self.folder.join(file_name(name)).exists();
        if taken {
            Err(MeetingTemplateError::DuplicateName(name.to_owned()))
        } else {
            Ok(())
        }
    }
}

fn file_name(name: &str) -> String {
    format!("{name}.{EXTENSION}")
}

fn normalize(body: &str) -> String {
    body.replace("\r\n", "\n")
}

fn read_body(path: &Path) -> std::io::Result<String> {
    Ok(normalize(&fs::read_to_string(path)?))
}

/// The template to apply to new meeting notes, or `None` while templates are turned off.
pub fn default_template(
    settings: &crate::settings::Settings,
    store: &TemplateStore,
) -> Option<MeetingTemplate> {
    settings
        .meeting_templates_enabled
        .then(|| store.resolve(&settings.default_meeting_template))
}

fn meeting_key(capture: &Capture) -> Option<(String, Option<DateTime<FixedOffset>>)> {
    capture
        .meeting
        .as_ref()
        .map(|meeting| (meeting.id.clone(), meeting.start))
}

/// Whether the capture's meeting changed since the template was last considered, so
/// [`sync_capture_template`] may have work to do.
pub fn needs_template_sync(capture: &Capture) -> bool {
    meeting_key(capture) != capture.template_meeting
}

/// Keeps a meeting template in step with the capture's meeting, and returns whether the body
/// changed. When the capture is connected to a different meeting and its body is empty, or is
/// still exactly the template inserted earlier, `template` is inserted for the new meeting.
/// Text the user typed is never replaced. Disconnecting removes an untouched template.
/// Pass `None` while templates are turned off.
pub fn sync_capture_template(
    capture: &mut Capture,
    template: Option<&MeetingTemplate>,
    now: DateTime<FixedOffset>,
) -> bool {
    if !needs_template_sync(capture) {
        return false;
    }
    match (capture.meeting.as_ref(), template) {
        (Some(_), Some(template)) => {
            let blank = capture.body.trim().is_empty() || capture.has_untouched_template();
            capture.template_meeting = meeting_key(capture);
            if !blank {
                return false;
            }
            let rendered = template.render(capture.meeting.as_ref(), now);
            let changed = capture.body != rendered;
            capture.body = rendered.clone();
            capture.template_text = Some(rendered);
            changed
        }
        // Templates are off: an untouched template still names the previous meeting, so it is
        // removed. A later connection, or turning templates on, can still apply one.
        (Some(_), None) => {
            let untouched = capture.has_untouched_template();
            capture.template_text = None;
            if untouched {
                capture.body.clear();
            }
            untouched
        }
        (None, _) => {
            capture.template_meeting = None;
            let untouched = capture.has_untouched_template();
            capture.template_text = None;
            if untouched {
                capture.body.clear();
            }
            untouched
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemplateInsertion {
    /// The note was empty, or still held an untouched template, so the template replaced it.
    Replaced,
    /// The note had text, so the template was added after it.
    Appended,
}

/// Inserts a template the user chose for this note. User text is kept: the template is
/// appended after it.
pub fn insert_template(
    capture: &mut Capture,
    template: &MeetingTemplate,
    now: DateTime<FixedOffset>,
) -> TemplateInsertion {
    let rendered = template.render(capture.meeting.as_ref(), now);
    capture.template_meeting = meeting_key(capture);
    if capture.is_empty() {
        capture.body = rendered.clone();
        capture.template_text = Some(rendered);
        TemplateInsertion::Replaced
    } else {
        let mut body = capture.body.trim_end().to_owned();
        body.push_str("\n\n");
        body.push_str(&rendered);
        capture.body = body;
        capture.template_text = None;
        TemplateInsertion::Appended
    }
}
