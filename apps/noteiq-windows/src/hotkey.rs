use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use noteiq_core::settings::normalize_hotkey;
use std::str::FromStr;

pub struct HotkeyController {
    manager: GlobalHotKeyManager,
    registered: Option<HotKey>,
    setting: String,
    enabled: bool,
}

/// Checks that a shortcut can be registered, without registering it.
pub fn validate(setting: &str) -> Result<(), String> {
    parse(setting).map(|_| ())
}

fn parse(setting: &str) -> Result<HotKey, String> {
    let normalized = normalize_hotkey(setting).map_err(|error| error.to_string())?;
    HotKey::from_str(&normalized).map_err(|_| {
        "NoteIQ doesn't recognize that key. Use a letter, digit or F-key, such as Win+Shift+N."
            .to_owned()
    })
}

impl HotkeyController {
    /// Creates the controller without registering anything, so a shortcut that is already
    /// taken at startup can still be replaced from Settings.
    pub fn new() -> Result<Self, String> {
        let manager = GlobalHotKeyManager::new()
            .map_err(|_| "Windows could not initialize global keyboard shortcuts.")?;
        Ok(Self {
            manager,
            registered: None,
            setting: String::new(),
            enabled: false,
        })
    }

    pub fn update(&mut self, setting: &str, enabled: bool) -> Result<(), String> {
        if self.setting == setting && self.enabled == enabled {
            return Ok(());
        }
        let parsed = if enabled && !setting.trim().is_empty() {
            Some(parse(setting)?)
        } else {
            None
        };
        let previous = self.registered.take();
        if let Some(previous) = previous
            && self.manager.unregister(previous).is_err()
        {
            self.registered = Some(previous);
            return Err("The previous NoteIQ shortcut could not be released.".to_owned());
        }
        let Some(parsed) = parsed else {
            self.setting = setting.to_owned();
            self.enabled = enabled;
            return Ok(());
        };
        if self.manager.register(parsed).is_err() {
            if let Some(previous) = previous
                && self.manager.register(previous).is_ok()
            {
                self.registered = Some(previous);
            }
            return Err(format!(
                "{} is already used by another app. Choose a different shortcut in Settings.",
                setting.trim()
            ));
        }
        self.setting = setting.to_owned();
        self.enabled = enabled;
        self.registered = Some(parsed);
        Ok(())
    }

    pub fn was_pressed(&self) -> bool {
        GlobalHotKeyEvent::receiver().try_iter().any(|event| {
            event.state == HotKeyState::Pressed
                && Some(event.id) == self.registered.map(|key| key.id)
        })
    }
}
