//! Tells whether the user is in a Teams call on this PC.
//!
//! Teams keeps its microphone capture session active for the whole call, even while muted, and
//! releases it when the call ends. Windows lists capture sessions per microphone, so NoteIQ
//! checks for an active one owned by Teams. Calls joined in a browser or on another device are
//! not seen.

use std::path::Path;
use windows::{
    Win32::{
        Foundation::CloseHandle,
        Media::Audio::{
            AudioSessionStateActive, DEVICE_STATE_ACTIVE, IAudioSessionControl2,
            IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture,
        },
        System::{
            Com::{
                CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
    },
    core::{Interface, PWSTR},
};

/// Process names of new Teams and classic Teams.
const TEAMS_PROCESSES: [&str; 2] = ["ms-teams.exe", "teams.exe"];

/// Whether Teams is using a microphone right now. Errors count as "not in a call", so a
/// missing or unusual audio setup only means fewer reminders.
pub fn teams_call_active() -> bool {
    // SAFETY: COM is initialized on this thread for the duration of the check and balanced
    // by `CoUninitialize`; the interfaces are released before that.
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let active = active_capture_processes()
            .unwrap_or_default()
            .iter()
            .any(|path| is_teams_process(path));
        if initialized {
            CoUninitialize();
        }
        active
    }
}

unsafe fn active_capture_processes() -> windows::core::Result<Vec<String>> {
    let mut paths = Vec::new();
    // SAFETY: plain COM calls on interfaces this function owns.
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let devices = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
        for device in 0..devices.GetCount()? {
            let manager: IAudioSessionManager2 =
                devices.Item(device)?.Activate(CLSCTX_ALL, None)?;
            let sessions = manager.GetSessionEnumerator()?;
            for session in 0..sessions.GetCount()? {
                let control: IAudioSessionControl2 = sessions.GetSession(session)?.cast()?;
                if control.GetState()? != AudioSessionStateActive {
                    continue;
                }
                if let Some(path) = process_path(control.GetProcessId()?) {
                    paths.push(path);
                }
            }
        }
    }
    Ok(paths)
}

fn process_path(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    // SAFETY: the handle is checked and closed; the buffer outlives the call.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut length = u32::try_from(buffer.len()).unwrap_or_default();
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(process);
        result.ok()?;
        Some(String::from_utf16_lossy(buffer.get(..length as usize)?))
    }
}

fn is_teams_process(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            TEAMS_PROCESSES
                .iter()
                .any(|teams| name.eq_ignore_ascii_case(teams))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_new_and_classic_teams_only() {
        assert!(is_teams_process(
            r"C:\Program Files\WindowsApps\MSTeams_25.1.0_x64__8wekyb3d8bbwe\ms-teams.exe"
        ));
        assert!(is_teams_process(
            r"C:\Users\me\AppData\Local\Microsoft\Teams\current\Teams.exe"
        ));
        assert!(!is_teams_process(
            r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application\msedgewebview2.exe"
        ));
        assert!(!is_teams_process(r"C:\Tools\not-teams.exe"));
    }

    /// Prints whether a Teams call is active. Run by hand during and after a call:
    /// `cargo test -p noteiq-windows --features desktop --lib -- smoke_reports_a_teams_call --ignored --nocapture`
    #[test]
    #[ignore = "depends on a live Teams call"]
    fn smoke_reports_a_teams_call() {
        println!("Teams call active: {}", teams_call_active());
    }
}
