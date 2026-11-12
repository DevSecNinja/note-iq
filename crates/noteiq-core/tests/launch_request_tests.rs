use noteiq_core::{
    launch::{
        ForwardRequest, LaunchRequest, forward_message, forward_request_message,
        parse_forward_message, parse_forward_request, parse_launch_request, reminder_uri,
    },
    model::CaptureSource,
};

#[test]
fn accepts_only_the_explicit_new_note_command_and_uri() {
    assert_eq!(
        parse_launch_request(&["noteiq".to_owned(), "new".to_owned()]).unwrap(),
        LaunchRequest::NewNote
    );
    assert_eq!(
        LaunchRequest::from_uri("noteiq://new").unwrap(),
        LaunchRequest::NewNote
    );
}

#[test]
fn accepts_the_uri_canonicalized_by_the_windows_shell() {
    for uri in ["noteiq://new/", "NOTEIQ://NEW/"] {
        assert_eq!(
            LaunchRequest::from_uri(uri).unwrap(),
            LaunchRequest::NewNote
        );
    }
}

#[test]
fn rejects_untrusted_arguments_and_uri_paths() {
    assert!(
        parse_launch_request(&[
            "noteiq".to_owned(),
            "new".to_owned(),
            "--path".to_owned(),
            "x".to_owned()
        ])
        .is_err()
    );
    for uri in [
        "noteiq://new/../../file.md",
        "noteiq://new?path=note.md",
        "noteiq://new/?path=note.md",
        "noteiq://new/#fragment",
        "noteiq://new//",
        "noteiq://new/file.md",
    ] {
        assert!(LaunchRequest::from_uri(uri).is_err(), "{uri}");
    }
}

#[test]
fn forwarded_launches_keep_their_source() {
    for source in [CaptureSource::Cli, CaptureSource::Uri, CaptureSource::Tray] {
        assert_eq!(
            parse_forward_message(&format!("{}\n", forward_message(source))),
            Some(source)
        );
    }
}

#[test]
fn ignores_unexpected_forwarded_messages() {
    for message in [
        "",
        "show",
        "new",
        "show shortcut",
        "show new_note",
        "show cli --path x",
        "SHOW CLI",
        "open C:\\notes\\x.md",
    ] {
        assert_eq!(parse_forward_message(message), None, "{message:?}");
    }
}

#[test]
fn reminder_links_carry_only_a_well_formed_key() {
    let reminder = "0123456789abcdef";
    assert_eq!(
        LaunchRequest::from_uri(&reminder_uri(reminder)).unwrap(),
        LaunchRequest::MeetingReminder(reminder.to_owned())
    );
    assert_eq!(
        LaunchRequest::from_uri("NOTEIQ://REMINDER/0123456789ABCDEF/").unwrap(),
        LaunchRequest::MeetingReminder(reminder.to_owned())
    );
    for uri in [
        "noteiq://reminder/",
        "noteiq://reminder/0123456789abcde",
        "noteiq://reminder/0123456789abcdefa",
        "noteiq://reminder/0123456789abcdeg",
        "noteiq://reminder/0123456789abcdef?path=x",
        "noteiq://reminder/../0123456789abcdef",
    ] {
        assert!(LaunchRequest::from_uri(uri).is_err(), "{uri}");
    }
}

#[test]
fn forwarded_reminders_keep_their_key() {
    let reminder = ForwardRequest::MeetingReminder("0123456789abcdef".to_owned());
    assert_eq!(
        parse_forward_request(&format!("{}\n", forward_request_message(&reminder))),
        Some(reminder)
    );
    assert_eq!(
        parse_forward_request("show cli"),
        Some(ForwardRequest::Show(CaptureSource::Cli))
    );
    for message in [
        "reminder",
        "reminder ",
        "reminder 0123456789ABCDEF",
        "reminder 0123456789abcdef extra",
        "reminder ../../file.md",
    ] {
        assert_eq!(parse_forward_request(message), None, "{message:?}");
    }
}
