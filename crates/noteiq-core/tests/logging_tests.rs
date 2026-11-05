//! Separate test binary: logging installs a process-wide tracing subscriber.

use noteiq_core::diagnostics::{DiagnosticsError, initialize_logging};
use std::{
    fs::{self, File},
    time::{Duration, SystemTime},
};

#[test]
fn prunes_expired_logs_and_writes_new_entries() {
    let directory = tempfile::tempdir().unwrap();
    let logs = directory.path().join("logs");
    fs::create_dir_all(&logs).unwrap();
    let expired = logs.join("noteiq.log.2026-01-01");
    let recent = logs.join("noteiq.log.keep");
    let unrelated = logs.join("other.log");
    for path in [&expired, &recent, &unrelated] {
        fs::write(path, "old entry\n").unwrap();
    }
    let long_ago = SystemTime::now() - Duration::from_secs(15 * 24 * 60 * 60);
    for path in [&expired, &unrelated] {
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
    }

    let current_log = logs.join(format!(
        "noteiq.log.{}",
        chrono::Utc::now().format("%Y-%m-%d")
    ));
    fs::create_dir(&current_log).unwrap();
    assert!(matches!(
        initialize_logging(&logs),
        Err(DiagnosticsError::Appender(_))
    ));
    fs::remove_dir(&current_log).unwrap();

    let guard = initialize_logging(&logs).unwrap();
    tracing::warn!(
        failure_class = "timed_out",
        "meeting lookup diagnostic marker"
    );
    drop(guard);

    assert!(!expired.exists(), "logs older than 14 days are pruned");
    assert!(recent.exists(), "recent logs are kept");
    assert!(
        unrelated.exists(),
        "files NoteIQ did not create are untouched"
    );
    let written = fs::read_dir(&logs)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path() != recent)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("noteiq.log")
        })
        .map(|entry| fs::read_to_string(entry.path()).unwrap())
        .collect::<String>();
    assert!(written.contains("meeting lookup diagnostic marker"));
    assert!(written.contains("timed_out"));

    assert!(matches!(
        initialize_logging(&logs),
        Err(DiagnosticsError::Subscriber)
    ));
}
