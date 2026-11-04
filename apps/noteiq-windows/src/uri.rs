use std::env;
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, KEY_WRITE},
};

pub fn register_protocol() -> Result<(), String> {
    let executable = env::current_exe().map_err(|_| "NoteIQ could not locate its executable.")?;
    let classes = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags("Software\\Classes", KEY_WRITE)
        .map_err(|_| "NoteIQ could not register its Command Palette link.")?;
    let (protocol, _) = classes
        .create_subkey("noteiq")
        .map_err(|_| "NoteIQ could not register its Command Palette link.")?;
    protocol
        .set_value("", &"URL:NoteIQ Capture")
        .map_err(|_| "NoteIQ could not register its Command Palette link.")?;
    protocol
        .set_value("URL Protocol", &"")
        .map_err(|_| "NoteIQ could not register its Command Palette link.")?;
    let (command, _) = protocol
        .create_subkey("shell\\open\\command")
        .map_err(|_| "NoteIQ could not register its Command Palette link.")?;
    let value = format!("\"{}\" \"%1\"", executable.display());
    command
        .set_value("", &value)
        .map_err(|_| "NoteIQ could not register its Command Palette link.")?;
    Ok(())
}
