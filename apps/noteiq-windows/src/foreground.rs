//! Windows only lets a process take the foreground while it holds activation rights: after a
//! hotkey or tray click, or when the launcher that started it passes them on. Both helpers are
//! best-effort; without rights Windows flashes the taskbar button instead.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        ASFW_ANY, AllowSetForegroundWindow, IsIconic, SW_RESTORE, SetForegroundWindow, ShowWindow,
    },
};

pub fn bring_to_front(window: &slint::Window) {
    let slint_handle = window.window_handle();
    let Ok(handle) = slint_handle.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let hwnd = handle.hwnd.get() as HWND;
    // SAFETY: `hwnd` belongs to the live Slint window owned by this UI thread. These calls only
    // change activation state, take no other pointers, and their failure is harmless.
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd);
    }
}

/// Called by a short-lived launcher process (`noteiq new`, `noteiq://new`) before it hands the
/// request to the running instance, so that instance may bring its window forward.
pub fn allow_running_instance_to_activate() {
    // SAFETY: plain value argument; failure only means Windows flashes the taskbar instead.
    unsafe {
        AllowSetForegroundWindow(ASFW_ANY);
    }
}
