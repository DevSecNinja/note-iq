use noteiq_core::{
    diagnostics::{DiagnosticEvent, DiagnosticOperation},
    workiq::WorkiqError,
};
use std::time::Instant;

pub fn meeting_failure(error: &WorkiqError, started: Instant) -> (String, DiagnosticEvent) {
    (
        error.user_message().to_owned(),
        DiagnosticEvent::new(
            DiagnosticOperation::MeetingLookup,
            error.failure_class(),
            started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        ),
    )
}
