//! Finds the signed-in person's display name for a note's `from` property, the way the vault's
//! own captures link to the person who wrote them.

use windows_sys::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};
use winreg::{RegKey, enums::HKEY_CURRENT_USER};

const MAX_NAME_UTF16: u32 = 1024;

/// Returns the Windows account's display name, falling back to the Office user name.
pub fn display_name() -> Option<String> {
    windows_display_name()
        .and_then(normalize)
        .or_else(|| office_user_name().and_then(normalize))
}

fn windows_display_name() -> Option<String> {
    let mut len = 0u32;
    // SAFETY: a null buffer with a zero length only asks for the required size, which Windows
    // writes to `len`, a valid local. The call is expected to fail with ERROR_MORE_DATA.
    unsafe {
        GetUserNameExW(NameDisplay, std::ptr::null_mut(), &mut len);
    }
    if len == 0 || len > MAX_NAME_UTF16 {
        return None;
    }
    let mut buffer = vec![0u16; len as usize];
    // SAFETY: `buffer` holds exactly `len` UTF-16 units and outlives the call; Windows writes at
    // most `len` units, including the terminating null, and updates `len` to the name's length.
    let found = unsafe { GetUserNameExW(NameDisplay, buffer.as_mut_ptr(), &mut len) };
    if !found {
        return None;
    }
    buffer.get(..len as usize).map(String::from_utf16_lossy)
}

fn office_user_name() -> Option<String> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Office\Common\UserInfo")
        .ok()?
        .get_value::<String, _>("UserName")
        .ok()
}

fn normalize(name: String) -> Option<String> {
    let name = name.trim().trim_end_matches('\0').trim();
    (!name.is_empty()).then(|| name.to_owned())
}
