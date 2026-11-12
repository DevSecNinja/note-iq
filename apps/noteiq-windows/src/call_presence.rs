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

/// Whether Teams is using a microphone right now: `Some(true)` in a call, `Some(false)` when
/// every microphone was checked without finding one, and `None` when the check failed, which
/// callers treat as "maybe in a call".
pub fn teams_call_active() -> Option<bool> {
    // SAFETY: COM is initialized on this thread for the duration of the check and balanced
    // by `CoUninitialize`; the interfaces are released before that.
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let (paths, complete) = active_capture_processes();
        if initialized {
            CoUninitialize();
        }
        if paths.iter().any(|path| is_teams_process(path)) {
            Some(true)
        } else {
            complete.then_some(false)
        }
    }
}

/// Processes with an active capture session, and whether every device and session could be
/// checked. One failing device does not hide a session found on another.
unsafe fn active_capture_processes() -> (Vec<String>, bool) {
    let mut paths = Vec::new();
    let mut complete = true;
    // SAFETY: plain COM calls on interfaces this function owns.
    unsafe {
        let devices =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .and_then(|enumerator| {
                    enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)
                });
        let Ok(devices) = devices else {
            return (paths, false);
        };
        let Ok(count) = devices.GetCount() else {
            return (paths, false);
        };
        for device in 0..count {
            let sessions = devices
                .Item(device)
                .and_then(|device| device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None))
                .and_then(|manager| manager.GetSessionEnumerator());
            let Ok(sessions) = sessions else {
                complete = false;
                continue;
            };
            let Ok(count) = sessions.GetCount() else {
                complete = false;
                continue;
            };
            for session in 0..count {
                let active = sessions
                    .GetSession(session)
                    .and_then(|session| session.cast::<IAudioSessionControl2>())
                    .and_then(|control| {
                        Ok((control.GetState()? == AudioSessionStateActive)
                            .then_some(control.GetProcessId()?))
                    });
                match active {
                    // A process NoteIQ may not open, such as an elevated one, is not Teams,
                    // which runs as the user.
                    Ok(Some(pid)) => paths.extend(process_path(pid)),
                    Ok(None) => {}
                    Err(_) => complete = false,
                }
            }
        }
    }
    (paths, complete)
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
        println!("Teams call active: {:?}", teams_call_active());
    }
}
