#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    match noteiq_windows::run() {
        Ok(()) => Ok(()),
        Err(error) => {
            show_startup_error(&error.to_string());
            Err(error)
        }
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn show_startup_error(message: &str) {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr};

    let message = OsStr::new(message)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let title = OsStr::new("NoteIQ could not start")
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: both UTF-16 strings are NUL-terminated and remain alive for the call.
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(
            ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            windows_sys::Win32::UI::WindowsAndMessaging::MB_OK
                | windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
        );
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("NoteIQ's desktop interface is supported on Windows.");
}
