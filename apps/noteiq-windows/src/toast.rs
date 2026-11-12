//! Native Windows notifications for meeting reminders.
//!
//! NoteIQ is not packaged, so it registers its app ID under
//! `HKCU\Software\Classes\AppUserModelId` to name and brand its notifications without a
//! Start menu shortcut. Clicking a notification opens a `noteiq://reminder/<key>` link, which
//! reaches the running NoteIQ through the existing protocol and single-instance handling, both
//! from the pop-up and from Notification Center.

use noteiq_core::{launch::reminder_uri, meeting_reminder::reminder_text, model::Meeting};
use std::{fs, path::PathBuf};
use windows::{
    Data::Xml::Dom::XmlDocument,
    UI::Notifications::{ToastNotification, ToastNotificationManager},
    core::HSTRING,
};
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, KEY_WRITE},
};

pub const APP_ID: &str = "DevSecNinja.NoteIQ";
const GROUP: &str = "meeting-reminders";
const ICON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/noteiq-64.png"));

/// Names NoteIQ as the sender of its notifications. Safe to repeat at every start.
pub fn register_app_id() -> Result<(), String> {
    let failure = |_| "NoteIQ could not register its notifications with Windows.".to_owned();
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags("Software\\Classes", KEY_WRITE)
        .and_then(|classes| classes.create_subkey(format!("AppUserModelId\\{APP_ID}")))
        .map_err(failure)?;
    key.set_value("DisplayName", &"NoteIQ").map_err(failure)?;
    if let Some(icon) = write_icon() {
        key.set_value("IconUri", &icon.display().to_string())
            .map_err(failure)?;
    }
    Ok(())
}

fn write_icon() -> Option<PathBuf> {
    let project = directories::ProjectDirs::from("io", "DevSecNinja", "NoteIQ")?;
    let path = project.data_local_dir().join("noteiq-notification.png");
    if fs::read(&path).ok().as_deref() != Some(ICON) {
        fs::create_dir_all(path.parent()?).ok()?;
        fs::write(&path, ICON).ok()?;
    }
    Some(path)
}

/// Shows "Write a quick note for …?" with Write note and Dismiss buttons.
pub fn show_meeting_reminder(meeting: &Meeting, key: &str) -> windows::core::Result<()> {
    let document = XmlDocument::new()?;
    let offset = *chrono::Local::now().offset();
    document.LoadXml(&HSTRING::from(reminder_xml(meeting, key, offset)))?;
    let toast = ToastNotification::CreateToastNotification(&document)?;
    toast.SetTag(&HSTRING::from(key))?;
    toast.SetGroup(&HSTRING::from(GROUP))?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))?.Show(&toast)
}

fn reminder_xml(meeting: &Meeting, key: &str, offset: chrono::FixedOffset) -> String {
    let text = reminder_text(meeting, offset);
    let link = escape(&reminder_uri(key));
    let details = if text.details.is_empty() {
        String::new()
    } else {
        format!("<text>{}</text>", escape(&text.details))
    };
    format!(
        concat!(
            r#"<toast launch="{link}" activationType="protocol">"#,
            r#"<visual><binding template="ToastGeneric">"#,
            "<text>{title}</text>{details}<text>{body}</text>",
            "</binding></visual>",
            "<actions>",
            r#"<action content="Write note" activationType="protocol" arguments="{link}"/>"#,
            r#"<action content="Dismiss" activationType="system" arguments="dismiss"/>"#,
            "</actions></toast>"
        ),
        link = link,
        title = escape(&text.title),
        details = details,
        body = escape(&text.body),
    )
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            // XML 1.0 cannot carry most control characters, even escaped.
            character if character.is_control() && !matches!(character, '\t' | '\n') => {}
            character => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn meeting(title: &str) -> Meeting {
        Meeting {
            id: "event-1".into(),
            title: Some(title.into()),
            start: Some(DateTime::parse_from_rfc3339("2026-11-04T14:00:00+01:00").unwrap()),
            end: Some(DateTime::parse_from_rfc3339("2026-11-04T14:30:00+01:00").unwrap()),
            organizer: Some("Alex Morgan <alex@example.test>".into()),
            invitees: Vec::new(),
            your_rsvp: None,
            entity_url: None,
            cancelled: false,
            declined: false,
            all_day: false,
            has_transcript: Some(false),
        }
    }

    /// Shows a real notification. Run by hand to check notifications on this machine:
    /// `cargo test -p noteiq-windows --features desktop --lib -- smoke_shows_a_real_toast --ignored --nocapture`
    #[test]
    #[ignore = "shows a real Windows notification"]
    fn smoke_shows_a_real_toast() {
        register_app_id().unwrap();
        show_meeting_reminder(&meeting("Q4 roadmap review"), "0123456789abcdef").unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let history = ToastNotificationManager::History()
            .unwrap()
            .GetHistoryWithId(&HSTRING::from(APP_ID))
            .unwrap();
        println!("history size: {}", history.Size().unwrap());
        assert!(history.Size().unwrap() > 0);
    }

    #[test]
    fn reminder_is_valid_xml_that_opens_the_meeting_link() {
        let xml = reminder_xml(
            &meeting("Q&A <draft> \"review\"\u{7}"),
            "0123456789abcdef",
            chrono::FixedOffset::east_opt(3600).unwrap(),
        );
        let document = XmlDocument::new().unwrap();
        document.LoadXml(&HSTRING::from(xml.as_str())).unwrap();
        let texts = document
            .GetElementsByTagName(&HSTRING::from("text"))
            .unwrap();
        assert_eq!(
            texts.Item(0).unwrap().InnerText().unwrap().to_string(),
            "Write a quick note for Q&A <draft> \"review\"?"
        );
        assert_eq!(
            texts.Item(1).unwrap().InnerText().unwrap().to_string(),
            "14:00\u{2013}14:30 \u{b7} Alex Morgan"
        );
        assert!(xml.contains(r#"launch="noteiq://reminder/0123456789abcdef""#));
        assert!(xml.contains(r#"arguments="noteiq://reminder/0123456789abcdef""#));
        assert!(xml.contains(r#"activationType="system" arguments="dismiss""#));
    }
}
