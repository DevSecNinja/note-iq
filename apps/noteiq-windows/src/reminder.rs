use crate::toast;
use noteiq_core::{destination::Destination, model::Meeting, saved_note::find_meeting_note};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
    },
    thread,
};

pub struct ReminderJob {
    pub meeting: Meeting,
    pub key: String,
    pub notes_root: PathBuf,
    /// Cleared when reminders are turned off; checked again right before the toast appears.
    pub enabled: Arc<AtomicBool>,
}

/// Whether a reminder was handled. `Delivered` includes a meeting that turned out to have a
/// note, so it is not checked again. `Cancelled` means reminders were turned off first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReminderOutcome {
    Delivered,
    Failed,
    Cancelled,
}

/// Shows meeting reminders off the UI thread, since searching the notes folder and calling
/// into Windows can take a moment. A meeting that already has a saved note is skipped. Each
/// job's outcome is passed to `report` with its key.
pub fn spawn_notifier(
    report: impl Fn(String, ReminderOutcome) + Send + 'static,
) -> Sender<ReminderJob> {
    let (sender, receiver) = mpsc::channel::<ReminderJob>();
    thread::spawn(move || {
        let mut registered = false;
        for job in receiver {
            if !job.enabled.load(Ordering::Relaxed) {
                report(job.key, ReminderOutcome::Cancelled);
                continue;
            }
            match find_meeting_note(&Destination::new(job.notes_root), &job.meeting) {
                Ok(Some(_)) => {
                    tracing::info!("Skipped a meeting reminder because the meeting has a note");
                    report(job.key, ReminderOutcome::Delivered);
                    continue;
                }
                Ok(None) => {}
                Err(_) => tracing::warn!(failure_class = "reminder_note_lookup_failed"),
            }
            if !registered {
                registered = true;
                if let Err(message) = toast::register_app_id() {
                    tracing::warn!(failure_class = "notification_registration_failed", message);
                }
            }
            // The notes folder lookup can take a while: reminders may have been turned off.
            if !job.enabled.load(Ordering::Relaxed) {
                report(job.key, ReminderOutcome::Cancelled);
                continue;
            }
            let outcome = match toast::show_meeting_reminder(&job.meeting, &job.key) {
                Ok(()) => {
                    tracing::info!("Showed a meeting reminder");
                    ReminderOutcome::Delivered
                }
                Err(error) => {
                    tracing::warn!(
                        failure_class = "reminder_notification_failed",
                        code = error.code().0,
                        "Could not show a meeting reminder"
                    );
                    ReminderOutcome::Failed
                }
            };
            report(job.key, outcome);
        }
    });
    sender
}
