use serde::Serialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use thiserror::Error;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt};
use uuid::Uuid;

const LOG_RETENTION: Duration = Duration::from_secs(14 * 24 * 60 * 60);

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticOperation {
    NoteSave,
    MeetingLookup,
    DestinationSelection,
    NoteOpen,
    ApplicationStart,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    MissingExecutable,
    StartFailed,
    ProcessExited,
    TimedOut,
    InvalidResponse,
    UnsupportedCapability,
    AuthenticationRequired,
    DestinationUnavailable,
    ExternalConflict,
    PermissionDenied,
    Unknown,
}

#[derive(Clone, Debug, Serialize)]
pub struct DiagnosticEvent {
    pub correlation_id: Uuid,
    pub operation: DiagnosticOperation,
    pub failure_class: FailureClass,
    pub elapsed_ms: u64,
}

impl DiagnosticEvent {
    pub fn new(
        operation: DiagnosticOperation,
        failure_class: FailureClass,
        elapsed_ms: u64,
    ) -> Self {
        Self {
            correlation_id: Uuid::new_v4(),
            operation,
            failure_class,
            elapsed_ms,
        }
    }
}

#[derive(Debug, Error)]
pub enum DiagnosticsError {
    #[error("The log directory is unavailable: {0}")]
    Directory(#[from] io::Error),
    #[error("The log file could not be opened.")]
    Appender(#[from] tracing_appender::rolling::InitError),
    #[error("The log subscriber could not be initialized.")]
    Subscriber,
}

pub fn default_log_directory() -> Option<PathBuf> {
    directories::ProjectDirs::from("io", "DevSecNinja", "NoteIQ")
        .map(|directories| directories.data_local_dir().join("logs"))
}

pub fn initialize_logging(log_directory: &Path) -> Result<WorkerGuard, DiagnosticsError> {
    fs::create_dir_all(log_directory)?;
    prune_old_logs(log_directory)?;
    let appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("noteiq.log")
        .build(log_directory)?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(writer)
        .with_ansi(false)
        .try_init()
        .map_err(|_| DiagnosticsError::Subscriber)?;
    Ok(guard)
}

fn prune_old_logs(log_directory: &Path) -> Result<(), io::Error> {
    let now = SystemTime::now();
    for entry in fs::read_dir(log_directory)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if !file_name.starts_with("noteiq.log") || !entry.file_type()?.is_file() {
            continue;
        }
        let modified = entry.metadata()?.modified()?;
        if now.duration_since(modified).unwrap_or_default() > LOG_RETENTION {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
