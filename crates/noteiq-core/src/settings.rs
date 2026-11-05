use crate::note_filename::{DEFAULT_NOTE_FILENAME, NoteFilename};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use thiserror::Error;

const VAULT_SUFFIX: [&str; 4] = ["projects", "personal-assistants", "notes", ""];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestinationKind {
    Vault,
    OneDrive,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub destination_kind: DestinationKind,
    pub destination_root: PathBuf,
    pub auto_connect_meeting: bool,
    pub hotkey: String,
    pub hotkey_enabled: bool,
    pub workiq_executable: Option<PathBuf>,
    /// Overrides the Windows display name used for a note's `from` property.
    pub author_name: Option<String>,
    /// Filename template for new notes, such as `{date}_{time}_{title}`.
    pub note_filename: String,
}

impl Settings {
    pub fn default_for(user_profile: &Path) -> Self {
        let mut vault = user_profile.join("OneDrive - Microsoft");
        for component in VAULT_SUFFIX.into_iter().filter(|part| !part.is_empty()) {
            vault.push(component);
        }
        Self {
            destination_kind: DestinationKind::Vault,
            destination_root: vault,
            auto_connect_meeting: true,
            hotkey: "Win+Shift+N".to_owned(),
            hotkey_enabled: true,
            workiq_executable: None,
            author_name: None,
            note_filename: DEFAULT_NOTE_FILENAME.to_owned(),
        }
    }

    /// The filename template for new notes. A hand-edited template that no longer validates
    /// falls back to the default rather than blocking saves.
    pub fn note_filename_template(&self) -> NoteFilename {
        NoteFilename::parse(&self.note_filename).unwrap_or_else(|error| {
            tracing::warn!(
                failure_class = "invalid_note_filename",
                error_kind = error.kind(),
                "Using the default note filename because the saved template is invalid"
            );
            NoteFilename::default()
        })
    }

    pub fn load(path: &Path, user_profile: &Path) -> Result<Self, SettingsError> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default_for(user_profile));
            }
            Err(error) => return Err(error.into()),
        };
        let settings = toml::from_str(&contents)?;
        Ok(settings)
    }

    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        let parent = path.parent().ok_or(SettingsError::InvalidPath)?;
        fs::create_dir_all(parent)?;
        let contents = toml::to_string_pretty(self)?;
        let mut temporary = NamedTempFile::new_in(parent)?;
        std::io::Write::write_all(&mut temporary, contents.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(path)
            .map_err(|error| SettingsError::Io(error.error))?;
        Ok(())
    }

    /// Recover syntax errors only after preserving the original file. I/O errors remain
    /// failures: an unreadable file must never be replaced with defaults.
    pub fn load_with_recovery(
        path: &Path,
        user_profile: &Path,
    ) -> Result<(Self, Option<PathBuf>), SettingsError> {
        match Self::load(path, user_profile) {
            Ok(settings) => Ok((settings, None)),
            Err(SettingsError::Parse(_)) => {
                let parent = path.parent().ok_or(SettingsError::InvalidPath)?;
                let contents = fs::read(path)?;
                let mut backup = tempfile::Builder::new()
                    .prefix("settings-")
                    .suffix(".toml.bak")
                    .tempfile_in(parent)?;
                std::io::Write::write_all(&mut backup, &contents)?;
                backup.as_file().sync_all()?;
                let (_, backup_path) = backup.keep().map_err(|error| error.error)?;
                Ok((Self::default_for(user_profile), Some(backup_path)))
            }
            Err(error) => {
                tracing::warn!(failure_class = "settings_load_failed");
                Err(error)
            }
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        let profile = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_default();
        Self::default_for(&profile)
    }
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("The settings file path is invalid.")]
    InvalidPath,
    #[error("Settings file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Settings file could not be parsed: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Settings file could not be serialized: {0}")]
    Serialize(#[from] toml::ser::Error),
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum HotkeyError {
    #[error("Enter a shortcut such as Win+Shift+N.")]
    Empty,
    #[error("Add a key after the modifiers, such as Win+Shift+N.")]
    MissingKey,
    #[error("Add Win, Ctrl or Alt so the shortcut doesn't block normal typing.")]
    MissingModifier,
    #[error("\"{0}\" isn't a modifier. Use Win, Ctrl, Alt or Shift.")]
    UnknownModifier(String),
}

fn canonical_modifier(token: &str) -> Option<&'static str> {
    match token.to_ascii_lowercase().as_str() {
        "win" | "windows" | "meta" | "super" | "cmd" | "command" => Some("Meta"),
        "ctrl" | "control" => Some("Ctrl"),
        "alt" | "option" => Some("Alt"),
        "shift" => Some("Shift"),
        _ => None,
    }
}

fn is_function_key(token: &str) -> bool {
    let upper = token.to_ascii_uppercase();
    upper
        .strip_prefix('F')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (1..=24).contains(&number))
}

/// Converts a user-facing shortcut such as `Win+Shift+N` into the syntax accepted by
/// `global-hotkey`, rejecting shortcuts that would capture ordinary typing.
pub fn normalize_hotkey(setting: &str) -> Result<String, HotkeyError> {
    let setting = setting.trim();
    if setting.is_empty() {
        return Err(HotkeyError::Empty);
    }
    let tokens = setting.split('+').map(str::trim).collect::<Vec<_>>();
    let (key, modifier_tokens) = tokens.split_last().ok_or(HotkeyError::Empty)?;
    if key.is_empty() || canonical_modifier(key).is_some() {
        return Err(HotkeyError::MissingKey);
    }
    let mut modifiers: Vec<&'static str> = Vec::new();
    for token in modifier_tokens {
        if token.is_empty() {
            return Err(HotkeyError::MissingKey);
        }
        let modifier = canonical_modifier(token)
            .ok_or_else(|| HotkeyError::UnknownModifier((*token).to_owned()))?;
        if !modifiers.contains(&modifier) {
            modifiers.push(modifier);
        }
    }
    if !modifiers.iter().any(|modifier| *modifier != "Shift") && !is_function_key(key) {
        return Err(HotkeyError::MissingModifier);
    }
    let key = if key.chars().count() == 1 {
        key.to_ascii_uppercase()
    } else {
        (*key).to_owned()
    };
    if modifiers.is_empty() {
        return Ok(key);
    }
    Ok(format!("{}+{key}", modifiers.join("+")))
}

pub fn discover_onedrive_notes(environment: &HashMap<String, String>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for key in ["OneDriveCommercial", "OneDrive", "OneDriveConsumer"] {
        let Some(root) = environment.get(key).filter(|root| !root.trim().is_empty()) else {
            continue;
        };
        let notes = PathBuf::from(root).join("Notes");
        if notes.is_dir() && !candidates.contains(&notes) {
            candidates.push(notes);
        }
    }
    candidates
}

pub fn discover_onedrive_notes_from_environment() -> Vec<PathBuf> {
    discover_onedrive_notes(&std::env::vars().collect())
}
