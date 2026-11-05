use noteiq_core::settings::{HotkeyError, Settings, normalize_hotkey};
use tempfile::tempdir;

#[test]
fn the_default_shortcut_is_win_shift_n_and_registers_as_meta_shift_n() {
    let profile = tempdir().unwrap();
    let settings = Settings::default_for(profile.path());

    assert_eq!(settings.hotkey, "Win+Shift+N");
    assert!(settings.hotkey_enabled);
    assert_eq!(normalize_hotkey(&settings.hotkey).unwrap(), "Meta+Shift+N");
}

#[test]
fn common_modifier_spellings_are_accepted_case_insensitively() {
    let cases = [
        ("win+shift+n", "Meta+Shift+N"),
        (" Ctrl + Alt + N ", "Ctrl+Alt+N"),
        ("Control+Option+Space", "Ctrl+Alt+Space"),
        ("Windows+KeyN", "Meta+KeyN"),
        ("Super+Shift+F2", "Meta+Shift+F2"),
        ("Ctrl+Ctrl+N", "Ctrl+N"),
        ("Shift+F9", "Shift+F9"),
        ("F9", "F9"),
    ];

    for (input, expected) in cases {
        assert_eq!(normalize_hotkey(input).as_deref(), Ok(expected), "{input}");
    }
}

#[test]
fn incomplete_or_typing_shortcuts_are_rejected_with_actionable_messages() {
    let cases = [
        ("", HotkeyError::Empty),
        ("   ", HotkeyError::Empty),
        ("N", HotkeyError::MissingModifier),
        ("Shift+N", HotkeyError::MissingModifier),
        ("Shift+Shift+N", HotkeyError::MissingModifier),
        ("Shift+Space", HotkeyError::MissingModifier),
        ("Win+Shift", HotkeyError::MissingKey),
        ("Win+", HotkeyError::MissingKey),
        ("Win++N", HotkeyError::MissingKey),
        ("Hyper+N", HotkeyError::UnknownModifier("Hyper".to_owned())),
    ];

    for (input, expected) in cases {
        let error = normalize_hotkey(input).unwrap_err();
        assert_eq!(error, expected, "{input:?}");
        let message = error.to_string();
        assert!(
            message.contains("Win") || message.contains("such as"),
            "{input:?} gives guidance: {message}"
        );
    }
}

#[test]
fn custom_and_disabled_shortcuts_persist_across_restarts() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("settings.toml");
    let mut settings = Settings::default_for(temp.path());
    settings.hotkey = "Ctrl+Alt+Q".to_owned();
    settings.hotkey_enabled = false;
    settings.save(&path).unwrap();

    let loaded = Settings::load(&path, temp.path()).unwrap();

    assert_eq!(loaded.hotkey, "Ctrl+Alt+Q");
    assert!(!loaded.hotkey_enabled);
}

#[test]
fn a_partial_settings_file_keeps_defaults_for_missing_hotkey_fields() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("settings.toml");
    std::fs::write(&path, "hotkey = \"Ctrl+Alt+Q\"\n").unwrap();

    let loaded = Settings::load(&path, temp.path()).unwrap();

    assert_eq!(loaded.hotkey, "Ctrl+Alt+Q");
    assert!(loaded.hotkey_enabled);
    assert!(loaded.auto_connect_meeting);
}
