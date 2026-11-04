use noteiq_core::settings::{DestinationKind, Settings, discover_onedrive_notes};
use std::collections::HashMap;
use tempfile::tempdir;

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
