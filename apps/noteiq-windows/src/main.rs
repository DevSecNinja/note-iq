#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    noteiq_windows::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("NoteIQ's desktop interface is supported on Windows.");
}
