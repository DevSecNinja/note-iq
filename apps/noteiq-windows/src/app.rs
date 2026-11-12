use crate::{instance, launch, ui};
use noteiq_core::{
    diagnostics::{default_log_directory, initialize_logging},
    launch::{ForwardRequest, LaunchRequest},
    model::CaptureSource,
    settings::Settings,
};
use std::{env, error::Error, path::PathBuf};

pub fn run() -> Result<(), Box<dyn Error>> {
    let arguments = env::args().collect::<Vec<_>>();
    let request = launch::parse_arguments(&arguments)?;
    let source = if arguments
        .get(1)
        .is_some_and(|argument| launch::is_noteiq_uri(argument))
    {
        CaptureSource::Uri
    } else if arguments.len() > 1 {
        CaptureSource::Cli
    } else {
        CaptureSource::Tray
    };

    let forward = match request {
        LaunchRequest::NewNote => ForwardRequest::Show(source),
        LaunchRequest::MeetingReminder(key) => ForwardRequest::MeetingReminder(key),
    };

    match instance::start_or_forward(forward)? {
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

            let log_directory =
                default_log_directory().unwrap_or_else(|| profile.join(".noteiq").join("logs"));
            let _log_guard = initialize_logging(&log_directory).ok();
            let (settings, backup) = Settings::load_with_recovery(&settings_path, &profile)?;
            let startup_message = backup.map(|path| {
                tracing::warn!(failure_class = "settings_parse_failed_backed_up");
                format!(
                    "Settings could not be read. The original is preserved at {}. Using defaults.",
                    path.display()
                )
            });

            ui::run(settings, settings_path, source, role, startup_message)?;
        }
    }
    Ok(())
}
