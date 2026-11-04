use noteiq_core::{
    diagnostics::{DiagnosticEvent, DiagnosticOperation, FailureClass},
    workiq::WorkiqError,
};
use std::collections::BTreeSet;

#[test]
fn structured_events_cannot_contain_note_or_meeting_payloads() {
    let event = DiagnosticEvent::new(
        DiagnosticOperation::MeetingLookup,
        FailureClass::TimedOut,
        2_500,
    );
    let rendered = serde_json::to_string(&event).unwrap();

    assert!(rendered.contains("meeting_lookup"));
    assert!(rendered.contains("timed_out"));
    assert!(!rendered.contains("meeting title"));
    assert!(!rendered.contains("note body"));
}

#[test]
fn structured_events_only_carry_identifiers_classes_and_timings() {
    let event = DiagnosticEvent::new(
        DiagnosticOperation::NoteSave,
        FailureClass::PermissionDenied,
        12,
    );
    let rendered = serde_json::to_value(&event).unwrap();

    let keys = rendered
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        keys,
        BTreeSet::from(["correlation_id", "elapsed_ms", "failure_class", "operation"])
    );
    assert_eq!(rendered["operation"], "note_save");
    assert_eq!(rendered["failure_class"], "permission_denied");
    assert_eq!(rendered["elapsed_ms"], 12);
}

#[test]
fn every_event_gets_a_fresh_correlation_id() {
    let first = DiagnosticEvent::new(
        DiagnosticOperation::ApplicationStart,
        FailureClass::Unknown,
        0,
    );
    let second = DiagnosticEvent::new(
        DiagnosticOperation::ApplicationStart,
        FailureClass::Unknown,
        0,
    );

    assert_ne!(first.correlation_id, second.correlation_id);
}

#[test]
fn every_workiq_failure_has_a_class_and_a_friendly_message() {
    let cases = [
        (
            WorkiqError::ExecutableMissing,
            FailureClass::MissingExecutable,
        ),
        (WorkiqError::StartFailed, FailureClass::StartFailed),
        (WorkiqError::ProcessExited, FailureClass::ProcessExited),
        (WorkiqError::TimedOut, FailureClass::TimedOut),
        (WorkiqError::InvalidResponse, FailureClass::InvalidResponse),
        (
            WorkiqError::AuthenticationRequired,
            FailureClass::AuthenticationRequired,
        ),
        (
            WorkiqError::UnsupportedCapability,
            FailureClass::UnsupportedCapability,
        ),
        (WorkiqError::LookupFailed, FailureClass::Unknown),
    ];

    for (error, class) in cases {
        assert_eq!(error.failure_class(), class, "{error:?}");
        let message = error.user_message();
        let lowercase = message.to_lowercase();
        assert!(message.ends_with('.'), "{error:?}: {message}");
        assert!(
            ["notes", "retry", "sign in"]
                .iter()
                .any(|hint| lowercase.contains(hint)),
            "{error:?} should tell the user what still works or what to do: {message}"
        );
        for jargon in ["MCP", "JSON", "stdout", "exit code", "panic"] {
            assert!(!message.contains(jargon), "{error:?}: {message}");
        }
    }
}
