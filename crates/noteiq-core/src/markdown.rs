use crate::model::{Capture, Meeting};
use chrono::{DateTime, FixedOffset};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MarkdownError {
    #[error("Could not encode Markdown metadata: {0}")]
    Encoding(#[from] serde_json::Error),
}

fn yaml_string(value: &str) -> Result<String, MarkdownError> {
    Ok(serde_json::to_string(value)?)
}

fn write_optional(
    output: &mut String,
    key: &str,
    value: Option<&str>,
) -> Result<(), MarkdownError> {
    if let Some(value) = value {
        output.push_str(&format!("{key}: {}\n", yaml_string(value)?));
    }
    Ok(())
}

fn write_meeting(output: &mut String, meeting: &Meeting) -> Result<(), MarkdownError> {
    output.push_str("meeting:\n");
    output.push_str(&format!("  id: {}\n", yaml_string(&meeting.id)?));
    write_optional(output, "  title", meeting.title.as_deref())?;
    if let Some(start) = meeting.start {
        output.push_str(&format!("  start: {}\n", yaml_string(&start.to_rfc3339())?));
    }
    if let Some(end) = meeting.end {
        output.push_str(&format!("  end: {}\n", yaml_string(&end.to_rfc3339())?));
    }
    write_optional(output, "  organizer", meeting.organizer.as_deref())?;
    if meeting.attendees.is_empty() {
        output.push_str("  attendees: []\n");
    } else {
        output.push_str("  attendees:\n");
        for attendee in &meeting.attendees {
            output.push_str(&format!("    - {}\n", yaml_string(attendee)?));
        }
    }
    Ok(())
}

pub fn render_markdown(
    capture: &Capture,
    updated_at: DateTime<FixedOffset>,
) -> Result<String, MarkdownError> {
    let mut output = String::new();
    output.push_str("---\n");
    output.push_str("schema: 1\n");
    output.push_str(&format!("id: {}\n", yaml_string(&capture.id.to_string())?));
    output.push_str(&format!(
        "created_at: {}\n",
        yaml_string(&capture.created_at.to_rfc3339())?
    ));
    output.push_str(&format!(
        "updated_at: {}\n",
        yaml_string(&updated_at.to_rfc3339())?
    ));
    output.push_str(&format!("timezone: {}\n", yaml_string(&capture.timezone)?));
    output.push_str(&format!("source: {}\n", capture.source.as_str()));
    if capture.tags.is_empty() {
        output.push_str("tags: []\n");
    } else {
        output.push_str("tags:\n");
        for tag in &capture.tags {
            output.push_str(&format!("  - {}\n", yaml_string(tag)?));
        }
    }
    if let Some(meeting) = &capture.meeting {
        write_meeting(&mut output, meeting)?;
    }
    output.push_str("---\n");
    output.push_str(&capture.body);
    if !capture.body.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}
