use crate::{instance, launch, ui};
use noteiq_core::{
    diagnostics::{default_log_directory, initialize_logging},
    model::CaptureSource,
    settings::Settings,
};
use std::{env, error::Error, path::PathBuf};

pub fn run() -> Result<(), Box<dyn Error>> {
    let arguments = env::args().collect::<Vec<_>>();
    let noteiq_core::launch::LaunchRequest::NewNote = launch::parse_arguments(&arguments)?;
    let source = if arguments
        .get(1)
        .is_some_and(|argument| argument.starts_with("noteiq:"))
    {
        CaptureSource::Uri
    } else if arguments.len() > 1 {
        CaptureSource::Cli
    } else {
        CaptureSource::Tray
    };

    match instance::start_or_forward(source)? {
        instance::InstanceRole::Forwarded => return Ok(()),
        role @ instance::InstanceRole::Primary(_) => {
            let profile = env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .or_else(|| env::var_os("HOME").map(PathBuf::from))
                .unwrap_or_default();
            let project = directories::ProjectDirs::from("io", "DevSecNinja", "NoteIQ");
            let settings_path = project
                .as_ref()
                .map(|project| project.config_dir().join("settings.toml"))
                .unwrap_or_else(|| profile.join(".noteiq").join("settings.toml"));

            let (settings, startup_message) = match Settings::load(&settings_path, &profile) {
                Ok(settings) => (settings, None),
                Err(error) => {
                    let _ = error;
                    (
                        Settings::default_for(&profile),
                        Some("Settings could not be loaded. NoteIQ is using defaults.".to_owned()),
                    )
                }
            };

            let log_directory =
                default_log_directory().unwrap_or_else(|| profile.join(".noteiq").join("logs"));
            let _log_guard = initialize_logging(&log_directory).ok();

            ui::run(settings, settings_path, source, role, startup_message)?;
        }
    }
    Ok(())
}
