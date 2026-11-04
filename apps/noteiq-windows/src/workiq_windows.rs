use noteiq_core::settings::Settings;
use std::{env, path::PathBuf};

pub fn discover_executable(settings: &Settings) -> Option<PathBuf> {
    settings
        .workiq_executable
        .as_ref()
        .filter(|path| path.is_file())
        .cloned()
        .or_else(|| {
            env::var_os("LOCALAPPDATA").map(PathBuf::from).map(|local| {
                local.join(
                    "Programs\\Microsoft Scout\\resources\\app.asar.unpacked\\node_modules\\@microsoft\\workiq\\bin\\win-arm64\\workiq.exe",
                )
            })
        })
        .filter(|path| path.is_file())
}
