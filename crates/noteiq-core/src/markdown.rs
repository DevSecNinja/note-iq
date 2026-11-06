//! Renders Obsidian properties, meeting context, optional refinement sections,
//! and the typed text as ordinary editable Markdown.

use crate::model::{Capture, Invitee, InviteeRole, Meeting, Rsvp, format_contact};
use chrono::{DateTime, FixedOffset, Offset, SecondsFormat, TimeZone};
use chrono_tz::Tz;
use thiserror::Error;

/// Layout version written as `noteiq_schema`; version 4 uses an editable Notes section.
pub const NOTE_SCHEMA: u32 = 4;

const BASE_TAGS: [&str; 2] = ["capture", "note"];
const FALLBACK_TITLE: &str = "Quick note";
const MAX_TITLE_CHARS: usize = 80;
const MARKDOWN_SPECIAL: &str = "\\`*_[]<>#|~$%=";
const WIKILINK_FORBIDDEN: &str = "[]|#^\\/:*?\"<>";
const REFINE_HINT: &str = "<!-- Captured with NoteIQ. Ask the assistant to \"refine this note\" to add a Summary and\n     Follow-ups. Notes below are editable Markdown. -->\n";

struct InviteeGroup {
    role: InviteeRole,
    key: &'static str,
    heading: &'static str,
    always_listed: bool,
}

const INVITEE_GROUPS: [InviteeGroup; 4] = [
    InviteeGroup {
        role: InviteeRole::Required,
        key: "required_invitees",
        heading: "Required invitees",
        always_listed: true,
    },
    InviteeGroup {
        role: InviteeRole::Optional,
        key: "optional_invitees",
        heading: "Optional invitees",
        always_listed: true,
    },
    InviteeGroup {
        role: InviteeRole::Resource,
        key: "resource_invitees",
        heading: "Resources",
        always_listed: false,
    },
    InviteeGroup {
        role: InviteeRole::Unknown,
        key: "other_invitees",
        heading: "Other invitees",
        always_listed: false,
    },
];

#[derive(Debug, Error)]
pub enum MarkdownError {
    #[error("Could not encode Markdown metadata: {0}")]
    Encoding(#[from] serde_json::Error),
}

/// Shows meeting times on the capture's wall clock. A named zone gives each instant the offset
/// in force at that moment, so a meeting that spans a daylight-saving change keeps its real end
/// time; the capture's fixed offset is the fallback when the zone is unknown or disagrees with
/// the clock the capture was taken on (for example, automatic DST adjustment turned off).
#[derive(Clone, Copy)]
enum LocalClock {
    Zone(Tz),
    Fixed(FixedOffset),
}

impl LocalClock {
    fn for_capture(capture: &Capture) -> Self {
        let fixed = *capture.created_at.offset();
        match capture.timezone.parse::<Tz>() {
            Ok(zone)
                if zone
                    .offset_from_utc_datetime(&capture.created_at.naive_utc())
                    .fix()
                    == fixed =>
            {
                Self::Zone(zone)
            }
            _ => Self::Fixed(fixed),
        }
    }

    fn at(self, time: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
        match self {
            Self::Zone(zone) => time.with_timezone(&zone).fixed_offset(),
            Self::Fixed(offset) => time.with_timezone(&offset),
        }
    }
}

pub fn render_markdown(
    capture: &Capture,
    updated_at: DateTime<FixedOffset>,
) -> Result<String, MarkdownError> {
    let clock = LocalClock::for_capture(capture);
    let author = capture.author.as_deref().and_then(wikilink_target);
    let mut output = String::with_capacity(2048 + capture.body.len());

    output.push_str("---\nkind: capture\nsource: manual\n");
    if let Some(author) = &author {
        output.push_str(&format!(
            "from: {}\n",
            yaml_string(&format!("[[{author}]]"))?
        ));
    }
    output.push_str(&format!(
        "date: {}\n",
        capture.created_at.format("%Y-%m-%d")
    ));
    output.push_str(&format!(
        "captured: {}\n",
        precise_timestamp(capture.created_at)
    ));
    output.push_str("processing: raw\nai_refined: false\nstatus: inbox\n");
    write_tags(&mut output, &capture.tags)?;
    if let Some(meeting) = &capture.meeting {
        write_meeting_properties(&mut output, meeting, clock)?;
    }
    output.push_str(&format!("id: {}\n", yaml_string(&capture.id.to_string())?));
    output.push_str(&format!("updated: {}\n", precise_timestamp(updated_at)));
    output.push_str(&format!("timezone: {}\n", yaml_string(&capture.timezone)?));
    output.push_str(&format!("captured_via: {}\n", capture.source.as_str()));
    output.push_str(&format!("noteiq_schema: {NOTE_SCHEMA}\n"));
    output.push_str("---\n\n");

    output.push_str(&format!("# {}\n\n", escape_inline(&note_title(capture))));
    let captured = capture.created_at.format("%Y-%m-%d %H:%M");
    match &author {
        Some(author) => output.push_str(&format!(
            "**From:** [[{author}]] · **Captured:** {captured}\n\n"
        )),
        None => output.push_str(&format!("**Captured:** {captured}\n\n")),
    }
    output.push_str(REFINE_HINT);
    output.push('\n');
    if let Some(meeting) = &capture.meeting {
        write_meeting_details(&mut output, meeting, clock);
    }
    output.push_str("## Summary\n\n## Follow-ups\n\n---\n\n## Notes\n\n");
    output.push_str(&capture.body);
    if !capture.body.is_empty() && !capture.body.ends_with('\n') {
        output.push('\n');
    }
    Ok(if capture.restored_document.is_some() {
        crate::saved_note::render_restored(capture, &output)
    } else {
        output
    })
}

fn yaml_string(value: &str) -> Result<String, MarkdownError> {
    Ok(serde_json::to_string(value)?)
}

fn precise_timestamp(value: DateTime<FixedOffset>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, false)
}

/// Trims a value and drops it when nothing printable remains.
fn present(value: Option<&str>) -> Option<&str> {
    value
        .map(str::trim)
        .filter(|value| value.chars().any(|c| !c.is_whitespace() && !c.is_control()))
}

/// Collapses line breaks, control characters and repeated whitespace into single spaces.
fn single_line(text: &str) -> String {
    text.split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn wikilink_target(name: &str) -> Option<String> {
    let cleaned = name
        .chars()
        .map(|c| {
            if WIKILINK_FORBIDDEN.contains(c) {
                ' '
            } else {
                c
            }
        })
        .collect::<String>();
    let cleaned = single_line(&cleaned);
    cleaned
        .chars()
        .any(char::is_alphanumeric)
        .then_some(cleaned)
}

fn write_tags(output: &mut String, tags: &[String]) -> Result<(), MarkdownError> {
    let mut seen = BASE_TAGS.map(str::to_owned).to_vec();
    let mut rendered = seen.clone();
    for tag in tags
        .iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
    {
        let folded = tag.to_lowercase();
        if seen.contains(&folded) {
            continue;
        }
        seen.push(folded);
        rendered.push(yaml_string(tag)?);
    }
    output.push_str(&format!("tags: [{}]\n", rendered.join(", ")));
    Ok(())
}

fn invitee_contact(invitee: &Invitee) -> Option<String> {
    format_contact(
        present(invitee.name.as_deref()),
        present(invitee.email.as_deref()),
    )
}

fn write_meeting_properties(
    output: &mut String,
    meeting: &Meeting,
    clock: LocalClock,
) -> Result<(), MarkdownError> {
    if let Some(title) = present(meeting.title.as_deref()) {
        output.push_str(&format!("meeting: {}\n", yaml_string(title)?));
    }
    for (key, time) in [
        ("meeting_start", meeting.start),
        ("meeting_end", meeting.end),
    ] {
        if let Some(time) = time {
            output.push_str(&format!(
                "{key}: {}\n",
                clock.at(time).to_rfc3339_opts(SecondsFormat::Secs, false)
            ));
        }
    }
    if let Some(organizer) = present(meeting.organizer.as_deref()) {
        output.push_str(&format!("organizer: {}\n", yaml_string(organizer)?));
    }
    if let Some(rsvp) = meeting.your_rsvp.filter(|rsvp| *rsvp != Rsvp::Unknown) {
        output.push_str(&format!("your_rsvp: {}\n", rsvp.as_str()));
    }
    for group in &INVITEE_GROUPS {
        let contacts = meeting
            .invitees
            .iter()
            .filter(|invitee| invitee.role == group.role)
            .filter_map(invitee_contact)
            .collect::<Vec<_>>();
        if contacts.is_empty() {
            if group.always_listed {
                output.push_str(&format!("{}: []\n", group.key));
            }
            continue;
        }
        output.push_str(&format!("{}:\n", group.key));
        for contact in contacts {
            output.push_str(&format!("  - {}\n", yaml_string(&contact)?));
        }
    }
    if let Some(id) = present(Some(meeting.id.as_str())) {
        output.push_str(&format!("calendar_event_id: {}\n", yaml_string(id)?));
    }
    Ok(())
}

fn write_meeting_details(output: &mut String, meeting: &Meeting, clock: LocalClock) {
    let mut lines = Vec::new();
    if let Some(when) = meeting_when(meeting, clock) {
        lines.push(format!("- **When:** {when}"));
    }
    if let Some(organizer) = present(meeting.organizer.as_deref()) {
        lines.push(format!(
            "- **Organizer:** {}",
            escape_inline(contact_name(organizer))
        ));
    }
    if let Some(rsvp) = meeting.your_rsvp.and_then(rsvp_label) {
        lines.push(format!("- **Your RSVP:** {rsvp}"));
    }
    for group in &INVITEE_GROUPS {
        let people = meeting
            .invitees
            .iter()
            .filter(|invitee| invitee.role == group.role)
            .filter_map(invitee_line)
            .collect::<Vec<_>>();
        if people.is_empty() {
            continue;
        }
        lines.push(format!("- **{}:**", group.heading));
        lines.extend(people.into_iter().map(|person| format!("  - {person}")));
    }
    if lines.is_empty() {
        return;
    }
    output.push_str("## Meeting details\n\n");
    for line in lines {
        output.push_str(&line);
        output.push('\n');
    }
    output.push('\n');
}

fn meeting_when(meeting: &Meeting, clock: LocalClock) -> Option<String> {
    let start = clock.at(meeting.start?);
    let Some(end) = meeting
        .end
        .map(|end| clock.at(end))
        .filter(|end| *end > start)
    else {
        return Some(start.format("%Y-%m-%d %H:%M").to_string());
    };
    let range = if start.date_naive() == end.date_naive() {
        format!("{}–{}", start.format("%Y-%m-%d %H:%M"), end.format("%H:%M"))
    } else {
        format!(
            "{} – {}",
            start.format("%Y-%m-%d %H:%M"),
            end.format("%Y-%m-%d %H:%M")
        )
    };
    Some(match duration_label((end - start).num_minutes()) {
        Some(duration) => format!("{range} ({duration})"),
        None => range,
    })
}

fn duration_label(minutes: i64) -> Option<String> {
    match (minutes / 60, minutes % 60) {
        (0, 0) => None,
        (0, minutes) => Some(format!("{minutes} min")),
        (hours, 0) => Some(format!("{hours} h")),
        (hours, minutes) => Some(format!("{hours} h {minutes} min")),
    }
}

fn rsvp_label(rsvp: Rsvp) -> Option<&'static str> {
    match rsvp {
        Rsvp::Accepted => Some("Accepted"),
        Rsvp::Tentative => Some("Tentative"),
        Rsvp::Declined => Some("Declined"),
        Rsvp::NotResponded => Some("No response"),
        Rsvp::Organizer => Some("Organizer"),
        Rsvp::Unknown => None,
    }
}

/// Shows a stored `Name <email>` contact by its name only.
fn contact_name(contact: &str) -> &str {
    contact
        .strip_suffix('>')
        .and_then(|rest| rest.rfind(" <"))
        .map(|index| contact[..index].trim())
        .filter(|name| !name.is_empty())
        .unwrap_or(contact)
}

fn invitee_line(invitee: &Invitee) -> Option<String> {
    let name = present(invitee.name.as_deref()).or_else(|| present(invitee.email.as_deref()))?;
    let mut details = Vec::new();
    if let Some(rsvp) = rsvp_label(invitee.rsvp) {
        details.push(rsvp.to_lowercase());
    }
    match invitee.has_attended {
        Some(true) => details.push("attended".to_owned()),
        Some(false) => details.push("did not attend".to_owned()),
        None => {}
    }
    let name = escape_inline(name);
    Some(if details.is_empty() {
        name
    } else {
        format!("{name} — {}", details.join(", "))
    })
}

/// Escapes calendar text and titles so they render literally in a heading or list item.
/// An `&` is escaped only where it would start an HTML entity, so "R&D" stays readable.
fn escape_inline(text: &str) -> String {
    let text = single_line(text);
    let mut escaped = String::with_capacity(text.len() + 8);
    for (index, character) in text.char_indices() {
        if MARKDOWN_SPECIAL.contains(character)
            || (character == '&' && starts_entity(&text[index + 1..]))
        {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    let digits = escaped.len()
        - escaped
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .len();
    if escaped.starts_with(['-', '+']) {
        escaped.insert(0, '\\');
    } else if digits > 0 && escaped[digits..].starts_with(['.', ')']) {
        escaped.insert(digits, '\\');
    }
    escaped
}

/// Reports whether the text after an `&` completes an entity such as `&amp;`, `&#38;` or `&#x26;`.
fn starts_entity(rest: &str) -> bool {
    let Some((name, _)) = rest.split_once(';') else {
        return false;
    };
    match name.strip_prefix('#') {
        Some(number) => {
            let (digits, radix) = match number.strip_prefix(['x', 'X']) {
                Some(hex) => (hex, 16),
                None => (number, 10),
            };
            (1..=7).contains(&digits.len()) && digits.chars().all(|c| c.is_digit(radix))
        }
        None => {
            (1..=32).contains(&name.len())
                && name.starts_with(|c: char| c.is_ascii_alphabetic())
                && name.chars().all(|c| c.is_ascii_alphanumeric())
        }
    }
}

fn note_title(capture: &Capture) -> String {
    capture
        .meeting
        .as_ref()
        .and_then(|meeting| meeting.title.as_deref())
        .map(single_line)
        .filter(|title| title.chars().any(char::is_alphanumeric))
        .or_else(|| body_title(&capture.body))
        .unwrap_or_else(|| FALLBACK_TITLE.to_owned())
}

fn body_title(body: &str) -> Option<String> {
    body.lines()
        .map(|line| single_line(strip_block_markers(line)))
        .find(|line| line.chars().any(char::is_alphanumeric))
        .map(|line| truncate_title(&line))
}

fn truncate_title(title: &str) -> String {
    if title.chars().count() <= MAX_TITLE_CHARS {
        return title.to_owned();
    }
    let kept = title.chars().take(MAX_TITLE_CHARS - 1).collect::<String>();
    format!("{}…", kept.trim_end())
}

/// Removes heading, quote, list and task markers so a line can serve as the note title.
fn strip_block_markers(line: &str) -> &str {
    let mut line = line.trim_start();
    loop {
        let before = line.len();
        line = line.trim_start_matches('#').trim_start();
        if let Some(rest) = line.strip_prefix('>') {
            line = rest.trim_start();
        }
        if let Some(rest) = list_item_text(line) {
            line = rest;
        }
        if let Some(rest) = ["[ ]", "[x]", "[X]"]
            .into_iter()
            .find_map(|task| marker_rest(line.strip_prefix(task)?))
        {
            line = rest;
        }
        if line.len() == before {
            return line;
        }
    }
}

fn list_item_text(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix(['-', '*', '+']) {
        return marker_rest(rest);
    }
    let digits = line.len() - line.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if !(1..=9).contains(&digits) {
        return None;
    }
    marker_rest(line[digits..].strip_prefix(['.', ')'])?)
}

/// Accepts a marker only when whitespace or the end of the line follows it.
fn marker_rest(rest: &str) -> Option<&str> {
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim_start())
}
