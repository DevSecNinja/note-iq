//! While NoteIQ has a package identity (needed for on-device AI), Windows groups its window
//! under the package and draws the taskbar button from the package logo, which shows as a blank
//! tile for a package with external location. Giving the window its own app ID makes the
//! taskbar use the window's icon again, as it does without the registration.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::{
    Win32::{
        Foundation::{HWND, PROPERTYKEY},
        Storage::EnhancedStorage::{
            PKEY_AppUserModel_ID, PKEY_AppUserModel_RelaunchCommand,
            PKEY_AppUserModel_RelaunchDisplayNameResource, PKEY_AppUserModel_RelaunchIconResource,
        },
        System::{
            Com::StructuredStorage::{PROPVARIANT, PropVariantClear},
            Variant::VT_LPWSTR,
        },
        UI::Shell::{
            PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow},
            SHStrDupW,
        },
    },
    core::HSTRING,
};

/// Not registered anywhere, so the taskbar draws the window's own icon. The notification app
/// ID can't be reused: the taskbar would look up its registration instead.
const WINDOW_APP_ID: &str = "DevSecNinja.NoteIQ.Capture";

/// Best-effort: on failure the taskbar keeps the package's blank tile.
pub fn use_own_taskbar_identity(window: &impl HasWindowHandle) {
    if windows::ApplicationModel::Package::Current().is_err() {
        return;
    }
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let hwnd = HWND(handle.hwnd.get() as *mut core::ffi::c_void);
    let executable = std::env::current_exe().unwrap_or_default();
    let command = format!("\"{}\"", executable.display());
    let icon = format!("{},0", executable.display());
    // Pinning the button relaunches the executable, not the package.
    let properties: [(&PROPERTYKEY, &str); 4] = [
        (&PKEY_AppUserModel_RelaunchCommand, &command),
        (&PKEY_AppUserModel_RelaunchDisplayNameResource, "NoteIQ"),
        (&PKEY_AppUserModel_RelaunchIconResource, &icon),
        (&PKEY_AppUserModel_ID, WINDOW_APP_ID),
    ];
    // SAFETY: `hwnd` belongs to the live winit window on this UI thread. Each string is copied
    // into COM memory by `SHStrDupW`, which `PropVariantClear` frees after `SetValue` copies it.
    let result = unsafe {
        SHGetPropertyStoreForWindow::<IPropertyStore>(hwnd).and_then(|store| {
            for (key, value) in properties {
                let mut variant = PROPVARIANT::default();
                let inner = &mut *variant.Anonymous.Anonymous;
                inner.Anonymous.pwszVal = SHStrDupW(&HSTRING::from(value))?;
                inner.vt = VT_LPWSTR;
                let set = store.SetValue(key, &variant);
                PropVariantClear(&mut variant)?;
                set?;
            }
            store.Commit()
        })
    };
    if let Err(error) = result {
        tracing::warn!(code = ?error.code(), "Could not give the window its own taskbar identity");
    }
}
