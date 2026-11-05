use noteiq_core::note_filename::{DEFAULT_NOTE_FILENAME, NoteFilename};
use noteiq_core::settings::{DestinationKind, Settings, discover_onedrive_notes};
use std::collections::HashMap;
use tempfile::tempdir;

#[test]
fn malformed_settings_are_backed_up_before_defaults_can_be_saved() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("settings.toml");
    let original = "destination_root = 'my-custom-vault'\nhotkey = [broken";
    std::fs::write(&path, original).unwrap();
    let (settings, backup) = Settings::load_with_recovery(&path, temp.path()).unwrap();
    let backup = backup.expect("recovery must preserve the unreadable configuration");
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    settings.save(&path).unwrap();
    assert_eq!(std::fs::read_to_string(backup).unwrap(), original);
    assert_eq!(Settings::load(&path, temp.path()).unwrap(), settings);
}

#[test]
fn settings_recovery_does_not_replace_previous_backups() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("settings.toml");
    std::fs::write(&path, "broken = [").unwrap();
    let (_, first) = Settings::load_with_recovery(&path, temp.path()).unwrap();
    let (_, second) = Settings::load_with_recovery(&path, temp.path()).unwrap();
    assert_ne!(first, second);
    assert!(first.unwrap().exists());
    assert!(second.unwrap().exists());
}

#[test]
fn settings_recovery_does_not_treat_io_errors_as_a_default_configuration() {
    let temp = tempdir().unwrap();
    assert!(Settings::load_with_recovery(temp.path(), temp.path()).is_err());
    let path = temp.path().join("settings.toml");
    let (settings, backup) = Settings::load_with_recovery(&path, temp.path()).unwrap();
    assert!(backup.is_none());
    assert!(!path.exists());
    settings.save(&path).unwrap();
    let (loaded, backup) = Settings::load_with_recovery(&path, temp.path()).unwrap();
    assert_eq!(settings, loaded);
    assert!(backup.is_none());
}

#[test]
fn defaults_to_the_requested_vault_relative_to_the_user_profile() {
    let profile = tempfile::tempdir().unwrap();
    let profile = profile.path();
    let settings = Settings::default_for(profile);

    assert_eq!(settings.destination_kind, DestinationKind::Vault);
    assert_eq!(
        settings.destination_root,
        profile
            .join("OneDrive - Microsoft")
            .join("projects")
            .join("personal-assistants")
            .join("notes")
    );
    assert!(settings.auto_connect_meeting);
    assert_eq!(settings.hotkey, "Win+Shift+N");
    assert!(settings.hotkey_enabled);
}

#[test]
fn discovers_existing_notes_folders_without_choosing_or_merging_accounts() {
    let temp = tempdir().unwrap();
    let corporate = temp.path().join("work");
    let personal = temp.path().join("personal");
    std::fs::create_dir_all(corporate.join("Notes")).unwrap();
    std::fs::create_dir_all(personal.join("Notes")).unwrap();
    let mut environment = HashMap::new();
    environment.insert(
        "OneDriveCommercial".to_owned(),
        corporate.to_string_lossy().into(),
    );
    environment.insert(
        "OneDriveConsumer".to_owned(),
        personal.to_string_lossy().into(),
    );

    let candidates = discover_onedrive_notes(&environment);

    assert_eq!(
        candidates,
        vec![corporate.join("Notes"), personal.join("Notes")]
    );
}

#[test]
fn settings_round_trip_through_the_user_configuration_file() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config").join("settings.toml");
    let profile = temp.path().join("profile");
    let mut settings = Settings::default_for(&profile);
    settings.auto_connect_meeting = false;
    settings.hotkey = "Ctrl+Alt+N".to_owned();
    settings.hotkey_enabled = false;
    settings.save(&path).unwrap();

    let loaded = Settings::load(&path, &profile).unwrap();

    assert_eq!(loaded, settings);
}

#[test]
fn the_note_author_follows_windows_unless_settings_name_one() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("settings.toml");
    let profile = temp.path().join("profile");
    assert_eq!(Settings::default_for(&profile).author_name, None);

    let mut settings = Settings::default_for(&profile);
    settings.author_name = Some("Alex Example".to_owned());
    settings.save(&path).unwrap();
    let loaded = Settings::load(&path, &profile).unwrap();

    assert_eq!(loaded.author_name.as_deref(), Some("Alex Example"));
    assert_eq!(loaded, settings);
}

#[test]
fn settings_saved_before_the_author_override_existed_still_load() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("settings.toml");
    let profile = temp.path().join("profile");
    let vault = temp.path().join("vault");
    std::fs::write(
        &path,
        format!(
            "destination_kind = \"vault\"\ndestination_root = '{}'\nauto_connect_meeting = false\nhotkey = \"Ctrl+Alt+N\"\nhotkey_enabled = true\n",
            vault.display()
        ),
    )
    .unwrap();

    let loaded = Settings::load(&path, &profile).unwrap();

    assert_eq!(loaded.author_name, None);
    assert_eq!(loaded.note_filename, DEFAULT_NOTE_FILENAME);
    assert_eq!(loaded.destination_root, vault);
    assert!(!loaded.auto_connect_meeting);
    assert_eq!(loaded.hotkey, "Ctrl+Alt+N");
}

#[test]
fn the_note_filename_setting_round_trips_through_the_configuration_file() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("settings.toml");
    let profile = temp.path().join("profile");
    let mut settings = Settings::default_for(&profile);
    assert_eq!(settings.note_filename, DEFAULT_NOTE_FILENAME);
    assert_eq!(settings.note_filename_template(), NoteFilename::default());

    settings.note_filename = "{date}-{title}".to_owned();
    settings.save(&path).unwrap();
    let loaded = Settings::load(&path, &profile).unwrap();

    assert_eq!(loaded, settings);
    assert_eq!(
        loaded.note_filename_template(),
        NoteFilename::parse("{date}-{title}").unwrap()
    );
}

#[test]
fn a_hand_edited_filename_that_is_invalid_falls_back_to_the_default() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("settings.toml");
    let profile = temp.path().join("profile");
    let mut settings = Settings::default_for(&profile);
    settings.note_filename = "notes/{customer}".to_owned();
    settings.save(&path).unwrap();

    let loaded = Settings::load(&path, &profile).unwrap();

    assert_eq!(
        loaded.note_filename, "notes/{customer}",
        "the file is left as written"
    );
    assert_eq!(loaded.note_filename_template(), NoteFilename::default());
}
