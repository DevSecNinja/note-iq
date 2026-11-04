use noteiq_core::settings::Settings;
use std::{
    env,
    path::{Path, PathBuf},
};

const SCOUT_WORKIQ_BIN: &str = "Programs\\Microsoft Scout\\resources\\app.asar.unpacked\\node_modules\\@microsoft\\workiq\\bin";

/// WorkIQ names its native folders `win-<arch>`. The build's own architecture comes first; the
/// other folder is a fallback because Windows on Arm runs both, and Scout ships only its own.
#[cfg(target_arch = "aarch64")]
const PLATFORM_FOLDERS: [&str; 2] = ["win-arm64", "win-x64"];
#[cfg(not(target_arch = "aarch64"))]
const PLATFORM_FOLDERS: [&str; 2] = ["win-x64", "win-arm64"];

pub fn discover_executable(settings: &Settings) -> Option<PathBuf> {
    let local_app_data = env::var_os("LOCALAPPDATA").map(PathBuf::from);
    discover_in(settings, local_app_data.as_deref())
}

fn discover_in(settings: &Settings, local_app_data: Option<&Path>) -> Option<PathBuf> {
    settings
        .workiq_executable
        .as_ref()
        .filter(|path| path.is_file())
        .cloned()
        .or_else(|| {
            let bin = local_app_data?.join(SCOUT_WORKIQ_BIN);
            PLATFORM_FOLDERS
                .iter()
                .map(|folder| bin.join(folder).join("workiq.exe"))
                .find(|path| path.is_file())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn settings() -> Settings {
        Settings::default_for(Path::new("C:\\Users\\example"))
    }

    fn install(local_app_data: &Path, folder: &str) -> PathBuf {
        let dir = local_app_data.join(SCOUT_WORKIQ_BIN).join(folder);
        fs::create_dir_all(&dir).unwrap();
        let executable = dir.join("workiq.exe");
        fs::write(&executable, b"").unwrap();
        executable
    }

    #[test]
    fn native_folder_matches_the_build_architecture() {
        let expected = if cfg!(target_arch = "aarch64") {
            "win-arm64"
        } else {
            "win-x64"
        };
        assert_eq!(PLATFORM_FOLDERS[0], expected);
        assert_ne!(PLATFORM_FOLDERS[0], PLATFORM_FOLDERS[1]);
    }

    #[test]
    fn prefers_the_build_architecture_when_both_are_installed() {
        let root = tempfile::tempdir().unwrap();
        let native = install(root.path(), PLATFORM_FOLDERS[0]);
        install(root.path(), PLATFORM_FOLDERS[1]);
        assert_eq!(discover_in(&settings(), Some(root.path())), Some(native));
    }

    #[test]
    fn falls_back_to_the_other_architecture() {
        let root = tempfile::tempdir().unwrap();
        let other = install(root.path(), PLATFORM_FOLDERS[1]);
        assert_eq!(discover_in(&settings(), Some(root.path())), Some(other));
    }

    #[test]
    fn configured_executable_wins_over_scout() {
        let root = tempfile::tempdir().unwrap();
        install(root.path(), PLATFORM_FOLDERS[0]);
        let custom = root.path().join("custom-workiq.exe");
        fs::write(&custom, b"").unwrap();
        let settings = Settings {
            workiq_executable: Some(custom.clone()),
            ..settings()
        };
        assert_eq!(discover_in(&settings, Some(root.path())), Some(custom));
    }

    #[test]
    fn missing_configured_executable_falls_back_to_scout() {
        let root = tempfile::tempdir().unwrap();
        let native = install(root.path(), PLATFORM_FOLDERS[0]);
        let settings = Settings {
            workiq_executable: Some(root.path().join("missing.exe")),
            ..settings()
        };
        assert_eq!(discover_in(&settings, Some(root.path())), Some(native));
    }

    #[test]
    fn reports_nothing_without_scout() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(SCOUT_WORKIQ_BIN).join(PLATFORM_FOLDERS[0])).unwrap();
        assert_eq!(discover_in(&settings(), Some(root.path())), None);
        assert_eq!(discover_in(&settings(), None), None);
    }
}
