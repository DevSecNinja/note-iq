use crate::{
    autosave::AutosaveSchedule,
    destination::{Destination, DestinationError, finish_capture, save_capture},
    model::{Capture, Meeting},
};
use chrono::{DateTime, FixedOffset};
use std::time::Instant;

pub struct CaptureController {
    pub capture: Capture,
    autosave: AutosaveSchedule,
}

impl CaptureController {
    pub fn new(capture: Capture) -> Self {
        Self {
            capture,
            autosave: AutosaveSchedule::default(),
        }
    }

    pub fn edit(&mut self, body: impl Into<String>, now: Instant) {
        self.capture.set_body(body);
        self.autosave.changed(now);
    }

    /// Attaches a meeting the user chose. Saving waits for the usual typing pause, so moving
    /// through the picker does not rewrite or rename the file at every step.
    pub fn connect_meeting(&mut self, meeting: Meeting, now: Instant) {
        self.capture.connect(meeting);
        self.metadata_changed(now);
    }

    pub fn disconnect_meeting(&mut self, now: Instant) {
        self.capture.disconnect();
        self.metadata_changed(now);
    }

    /// Schedules saving a meeting or author change. An unsaved empty draft stays unsaved.
    pub fn metadata_changed(&mut self, now: Instant) {
        if self.capture.path.is_some() || !self.capture.is_empty() {
            self.autosave.changed(now);
        }
    }

    pub fn autosave_due(&self, now: Instant) -> bool {
        self.autosave.is_due(now)
    }

    pub fn is_dirty(&self) -> bool {
        self.autosave.is_dirty()
    }

    pub fn save(
        &mut self,
        destination: &Destination,
        now: DateTime<FixedOffset>,
    ) -> Result<Option<std::path::PathBuf>, DestinationError> {
        let result = match save_capture(&mut self.capture, destination, now) {
            Ok(result) => result,
            Err(error) => {
                self.autosave.failed(Instant::now());
                return Err(error);
            }
        };
        self.autosave.saved();
        Ok(result)
    }

    pub fn finish(&mut self) -> Result<(), DestinationError> {
        finish_capture(&mut self.capture)
    }
}
