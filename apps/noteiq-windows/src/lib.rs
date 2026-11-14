#![cfg(all(windows, feature = "desktop"))]

#[cfg(windows)]
mod app;
#[cfg(windows)]
#[allow(unsafe_code)]
mod author;
#[cfg(windows)]
#[allow(unsafe_code)]
mod call_presence;
#[cfg(windows)]
mod error_ui;
#[cfg(windows)]
#[allow(unsafe_code)]
mod foreground;
#[cfg(windows)]
mod hotkey;
#[cfg(windows)]
#[allow(unsafe_code)]
mod instance;
#[cfg(windows)]
mod keyboard;
#[cfg(windows)]
mod launch;
#[cfg(windows)]
mod meeting_ui;
#[cfg(windows)]
mod open_note;
#[cfg(windows)]
mod reminder;
#[cfg(windows)]
mod settings_ui;
#[cfg(windows)]
mod toast;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod ui;
#[cfg(windows)]
#[allow(unsafe_code)]
mod ui_generated;
#[cfg(windows)]
mod uri;
#[cfg(windows)]
mod workiq_windows;

#[cfg(windows)]
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    app::run()
}
