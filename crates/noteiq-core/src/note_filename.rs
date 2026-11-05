//! Note filename templates such as `{date}_{time}_{title}`.

use chrono::{DateTime, FixedOffset};
use thiserror::Error;

pub const DEFAULT_NOTE_FILENAME: &str = "{date}_{time}_{title}";
pub const DATE_TITLE_NOTE_FILENAME: &str = "{date}-{title}";
/// Longest name before the collision suffix and `.md`, leaving room for the vault path.
pub const MAX_STEM_LENGTH: usize = 120;
/// Title used in the Settings example.
pub const EXAMPLE_TITLE: &str = "project-review";
/// Longest generated title; matches the slug limit used for note filenames.
const SAMPLE_TITLE_LENGTH: usize = 48;
/// Characters Windows or Obsidian links cannot use in a note name.
const INVALID_CHARACTERS: [char; 11] = ['<', '>', ':', '"', '|', '?', '*', '#', '^', '[', ']'];

#[derive(Clone, Debug, Eq, PartialEq)]
enum Part {
    Literal(String),
    Date,
    Time,
    Title,
}

/// A validated filename template. `{date}`, `{time}` and `{title}` are replaced when a note is
/// first saved; `.md` and any collision suffix are added automatically.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteFilename {
    parts: Vec<Part>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum NoteFilenameError {
    #[error("Enter a filename, such as {{date}}_{{time}}_{{title}}.")]
    Empty,
    #[error("Unknown placeholder: {{{0}}}. Use {{date}}, {{time}}, or {{title}}.")]
    UnknownPlaceholder(String),
    #[error("Close each placeholder with a matching brace, such as {{title}}.")]
    UnbalancedBraces,
    #[error("Enter a filename, not a folder path.")]
    PathSeparator,
    #[error("Remove the {0} character. Windows and Obsidian can't use it in filenames.")]
    InvalidCharacter(char),
    #[error("Remove tabs and other control characters from the filename.")]
    ControlCharacter,
    #[error("Filenames can't start or end with a dot.")]
    EdgeDot,
    #[error("\"{0}\" is reserved by Windows. Add more text to the filename.")]
    ReservedName(String),
    #[error("Keep the filename to {max} characters or fewer, including the title.", max = MAX_STEM_LENGTH)]
    TooLong,
}

impl NoteFilenameError {
    /// A stable, content-free name for diagnostics.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::UnknownPlaceholder(_) => "unknown_placeholder",
            Self::UnbalancedBraces => "unbalanced_braces",
            Self::PathSeparator => "path_separator",
            Self::InvalidCharacter(_) => "invalid_character",
            Self::ControlCharacter => "control_character",
            Self::EdgeDot => "edge_dot",
            Self::ReservedName(_) => "reserved_name",
            Self::TooLong => "too_long",
        }
    }
}

impl Default for NoteFilename {
    fn default() -> Self {
        Self {
            parts: vec![
                Part::Date,
                Part::Literal("_".to_owned()),
                Part::Time,
                Part::Literal("_".to_owned()),
                Part::Title,
            ],
        }
    }
}

impl NoteFilename {
    /// Parses a template. Placeholders ignore case and surrounding spaces; a trailing `.md` is
    /// dropped because the extension is always added.
    pub fn parse(template: &str) -> Result<Self, NoteFilenameError> {
        let template = template.trim();
        let template = strip_markdown_extension(template).trim();
        if template.is_empty() {
            return Err(NoteFilenameError::Empty);
        }

        let mut parts = Vec::new();
        let mut literal = String::new();
        let mut characters = template.chars();
        while let Some(character) = characters.next() {
            match character {
                '{' => {
                    let mut name = String::new();
                    loop {
                        match characters.next() {
                            Some('}') => break,
                            Some('{') | None => return Err(NoteFilenameError::UnbalancedBraces),
                            Some(character) => name.push(character),
                        }
                    }
                    let name = name.trim();
                    let part = match name.to_lowercase().as_str() {
                        "date" => Part::Date,
                        "time" => Part::Time,
                        "title" => Part::Title,
                        _ => return Err(NoteFilenameError::UnknownPlaceholder(name.to_owned())),
                    };
                    if !literal.is_empty() {
                        parts.push(Part::Literal(std::mem::take(&mut literal)));
                    }
                    parts.push(part);
                }
                '}' => return Err(NoteFilenameError::UnbalancedBraces),
                '/' | '\\' => return Err(NoteFilenameError::PathSeparator),
                character if character.is_control() => {
                    return Err(NoteFilenameError::ControlCharacter);
                }
                character if INVALID_CHARACTERS.contains(&character) => {
                    return Err(NoteFilenameError::InvalidCharacter(character));
                }
                character => literal.push(character),
            }
        }
        if !literal.is_empty() {
            parts.push(Part::Literal(literal));
        }
        if template.starts_with('.') || template.ends_with('.') {
            return Err(NoteFilenameError::EdgeDot);
        }

        let filename = Self { parts };
        let sample_time = DateTime::UNIX_EPOCH.fixed_offset();
        let longest = filename.stem(sample_time, &"x".repeat(SAMPLE_TITLE_LENGTH));
        if longest.chars().count() > MAX_STEM_LENGTH {
            return Err(NoteFilenameError::TooLong);
        }
        let sample = filename.stem(sample_time, "note");
        if is_reserved_windows_name(&sample) {
            let base = sample.split('.').next().unwrap_or(&sample).trim_end();
            return Err(NoteFilenameError::ReservedName(base.to_owned()));
        }
        Ok(filename)
    }

    /// The canonical template text stored in settings.
    pub fn template(&self) -> String {
        self.parts
            .iter()
            .map(|part| match part {
                Part::Literal(text) => text.as_str(),
                Part::Date => "{date}",
                Part::Time => "{time}",
                Part::Title => "{title}",
            })
            .collect()
    }

    /// Whether the name changes with the meeting title or note text.
    pub fn uses_title(&self) -> bool {
        self.parts.contains(&Part::Title)
    }

    /// The complete filename, such as `2026-11-04_1415_project-review-2.md`. Collision number
    /// 1 has no suffix.
    pub fn render(&self, at: DateTime<FixedOffset>, title: &str, collision_number: u32) -> String {
        let mut name = self.stem(at, title);
        if is_reserved_windows_name(&name) {
            name.insert_str(0, "note-");
        }
        if collision_number > 1 {
            name.push('-');
            name.push_str(&collision_number.to_string());
        }
        name.push_str(".md");
        name
    }

    /// The filename Settings shows as an example.
    pub fn example(&self, at: DateTime<FixedOffset>) -> String {
        self.render(at, EXAMPLE_TITLE, 1)
    }

    fn stem(&self, at: DateTime<FixedOffset>, title: &str) -> String {
        let mut stem = String::new();
        for part in &self.parts {
            match part {
                Part::Literal(text) => stem.push_str(text),
                Part::Date => stem.push_str(&at.format("%Y-%m-%d").to_string()),
                Part::Time => stem.push_str(&at.format("%H%M").to_string()),
                Part::Title => stem.push_str(title),
            }
        }
        stem
    }
}

fn strip_markdown_extension(template: &str) -> &str {
    let Some(split) = template.len().checked_sub(3) else {
        return template;
    };
    match (template.get(..split), template.get(split..)) {
        (Some(stem), Some(extension)) if extension.eq_ignore_ascii_case(".md") => stem,
        _ => template,
    }
}

/// Windows reserves these device names, with or without an extension, in every folder.
pub(crate) fn is_reserved_windows_name(name: &str) -> bool {
    let base = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end_matches(' ')
        .to_uppercase();
    if matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    let Some(number) = base
        .strip_prefix("COM")
        .or_else(|| base.strip_prefix("LPT"))
    else {
        return false;
    };
    let mut characters = number.chars();
    matches!(
        (characters.next(), characters.next()),
        (Some('0'..='9' | '¹' | '²' | '³'), None)
    )
}
