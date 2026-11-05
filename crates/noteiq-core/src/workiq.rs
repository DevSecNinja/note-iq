use crate::{
    diagnostics::FailureClass,
    model::{Invitee, InviteeRole, Meeting, Rsvp, format_contact},
};
use chrono::{
    DateTime, Datelike, Days, FixedOffset, NaiveDate, NaiveTime, TimeDelta, TimeZone, Weekday,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};
use thiserror::Error;

const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
const MAX_MESSAGE_BYTES: u64 = 4 * 1024 * 1024;
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const RETRIEVAL_CONTENT_TYPE: &str = "application/vnd.ms-workiq.retrieval";
/// WorkIQ returns at most about ten results per search, so focused queries for the current and
/// next meetings run alongside the broader day query. All three run concurrently.
pub const MEETING_QUERIES: [&str; 3] = [
    "My calendar meetings happening now",
    "My calendar meetings starting in the next hour",
    "My calendar meetings today",
];
/// Slack when comparing WorkIQ's `timeUntilMeeting` with a parsed start time; WorkIQ evaluates
/// it a few seconds after NoteIQ captured `now`.
const TIMING_TOLERANCE: TimeDelta = TimeDelta::minutes(5);
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// A cold WorkIQ meeting search was observed taking about 18 seconds.
pub const DEFAULT_LOOKUP_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeetingSearchTool {
    pub name: String,
    pub query_property: String,
    pub meetings_capability: bool,
}

#[derive(Clone, Debug, Error)]
pub enum WorkiqError {
    #[error("The Scout WorkIQ executable was not found.")]
    ExecutableMissing,
    #[error("The Scout WorkIQ process could not be started.")]
    StartFailed,
    #[error("The Scout WorkIQ process stopped unexpectedly.")]
    ProcessExited,
    #[error("WorkIQ did not respond before the lookup timed out.")]
    TimedOut,
    #[error("WorkIQ returned an invalid response.")]
    InvalidResponse,
    #[error("WorkIQ needs you to sign in again.")]
    AuthenticationRequired,
    #[error("This WorkIQ version does not expose a compatible meetings search.")]
    UnsupportedCapability,
    #[error("WorkIQ could not complete the meeting lookup.")]
    LookupFailed,
}

impl WorkiqError {
    pub fn failure_class(&self) -> FailureClass {
        match self {
            Self::ExecutableMissing => FailureClass::MissingExecutable,
            Self::StartFailed => FailureClass::StartFailed,
            Self::ProcessExited => FailureClass::ProcessExited,
            Self::TimedOut => FailureClass::TimedOut,
            Self::InvalidResponse => FailureClass::InvalidResponse,
            Self::AuthenticationRequired => FailureClass::AuthenticationRequired,
            Self::UnsupportedCapability => FailureClass::UnsupportedCapability,
            Self::LookupFailed => FailureClass::Unknown,
        }
    }

    pub fn user_message(&self) -> &'static str {
        match self {
            Self::ExecutableMissing => {
                "Scout's WorkIQ component was not found. Notes still work without meeting context."
            }
            Self::StartFailed => {
                "Scout's WorkIQ component could not start. Notes still work without meeting context."
            }
            Self::ProcessExited => {
                "Scout's WorkIQ component stopped unexpectedly. You can retry or keep taking notes."
            }
            Self::TimedOut => "Meeting lookup timed out. You can retry or keep taking notes.",
            Self::InvalidResponse => {
                "WorkIQ returned data NoteIQ could not use. You can retry or keep taking notes."
            }
            Self::AuthenticationRequired => {
                "WorkIQ needs you to sign in again with Scout, then retry meeting lookup."
            }
            Self::UnsupportedCapability => {
                "This WorkIQ version does not support meeting lookup. Notes still work normally."
            }
            Self::LookupFailed => {
                "WorkIQ could not look up meetings. Check connectivity and sign-in, then retry."
            }
        }
    }
}

fn authentication_error(value: &Value) -> bool {
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    [
        "authentication",
        "unauthorized",
        "sign in",
        "token expired",
        "access token",
    ]
    .iter()
    .any(|indicator| message.contains(indicator))
}

pub fn discover_meeting_search_tool(
    response: &Value,
) -> Result<Option<MeetingSearchTool>, WorkiqError> {
    if let Some(error) = response.get("error") {
        return Err(if authentication_error(error) {
            WorkiqError::AuthenticationRequired
        } else {
            WorkiqError::InvalidResponse
        });
    }
    let tools = response
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .ok_or(WorkiqError::InvalidResponse)?;

    for tool in tools {
        let Some(name) = tool.get("name").and_then(Value::as_str) else {
            continue;
        };
        if name != "retrieve" {
            continue;
        }
        let Some(schema) = tool.get("inputSchema") else {
            continue;
        };
        let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
            continue;
        };
        if !properties.contains_key("query")
            || !schema_supports_meetings(properties.get("capabilities"))
        {
            continue;
        }
        let supported = ["query", "capabilities", "strategy"];
        let all_required_supported = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|required| {
                required.iter().all(|property| {
                    property
                        .as_str()
                        .is_some_and(|property| supported.contains(&property))
                })
            })
            .unwrap_or(true);
        if !all_required_supported {
            continue;
        }
        return Ok(Some(MeetingSearchTool {
            name: name.to_owned(),
            query_property: "query".to_owned(),
            meetings_capability: true,
        }));
    }
    Ok(None)
}

fn schema_supports_meetings(schema: Option<&Value>) -> bool {
    schema.is_some_and(|schema| schema.to_string().contains("\"const\":\"Meetings\""))
}

/// Builds the arguments for one of [`MEETING_QUERIES`]. WorkIQ search ranks by relevance, so
/// the current meeting is still selected locally from the merged results.
pub fn meeting_search_arguments(tool: &MeetingSearchTool, query: &str) -> Value {
    json!({
        tool.query_property.clone(): query,
        "capabilities": [{"name": "Meetings"}],
        "strategy": "grounding"
    })
}

fn parse_timestamp(value: &Value) -> Option<DateTime<FixedOffset>> {
    value
        .as_str()
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .or_else(|| {
            value
                .as_object()
                .and_then(|object| object.get("dateTime").or_else(|| object.get("start")))
                .and_then(|nested| nested.as_str())
                .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        })
}

fn string_field<'a>(object: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
}

fn display_field(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        let value = object.get(*key)?;
        value.as_str().map(str::to_owned).or_else(|| {
            value
                .as_object()
                .and_then(|nested| string_field(nested, &["name", "displayName", "email"]))
                .map(str::to_owned)
        })
    })
}

fn parse_rsvp(value: &str) -> Option<Rsvp> {
    match value.trim().to_ascii_lowercase().as_str() {
        "accepted" => Some(Rsvp::Accepted),
        "tentative" | "tentativelyaccepted" => Some(Rsvp::Tentative),
        "declined" => Some(Rsvp::Declined),
        "notresponded" | "not responded" => Some(Rsvp::NotResponded),
        "organizer" | "meeting organizer" => Some(Rsvp::Organizer),
        _ => None,
    }
}

fn parse_invitees(value: Option<&Value>) -> Vec<Invitee> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|invitee| {
            let (name, email) = person_details(invitee);
            if name.is_none() && email.is_none() {
                return None;
            }
            Some(Invitee {
                name,
                email,
                role: match invitee.get("type").and_then(Value::as_str) {
                    Some(role) if role.eq_ignore_ascii_case("required") => InviteeRole::Required,
                    Some(role) if role.eq_ignore_ascii_case("optional") => InviteeRole::Optional,
                    Some(role) if role.eq_ignore_ascii_case("resource") => InviteeRole::Resource,
                    _ => InviteeRole::Unknown,
                },
                rsvp: invitee
                    .pointer("/status/response")
                    .and_then(Value::as_str)
                    .and_then(parse_rsvp)
                    .unwrap_or(Rsvp::Unknown),
                has_attended: invitee.get("hasAttended").and_then(Value::as_bool),
            })
        })
        .collect()
}

/// Applies calendar RSVP details without discarding attendance observations from retrieval.
pub fn apply_meeting_details(meeting: &mut Meeting, details: &Value) {
    if let Some(organizer) = details.get("organizer").and_then(person_contact) {
        meeting.organizer = Some(organizer);
    }
    if let Some(rsvp) = details
        .pointer("/responseStatus/response")
        .and_then(Value::as_str)
        .and_then(parse_rsvp)
    {
        meeting.your_rsvp = Some(rsvp);
        meeting.declined = rsvp == Rsvp::Declined;
    }
    if let Some(attendees) = details.get("attendees").filter(|value| value.is_array()) {
        let mut invitees = parse_invitees(Some(attendees));
        for invitee in &mut invitees {
            let previous =
                meeting
                    .invitees
                    .iter()
                    .find(|previous| match (&invitee.email, &previous.email) {
                        (Some(email), Some(previous)) => email.eq_ignore_ascii_case(previous),
                        (None, None) => invitee.name == previous.name,
                        _ => false,
                    });
            if let Some(previous) = previous {
                invitee.has_attended = invitee.has_attended.or(previous.has_attended);
                if invitee.rsvp == Rsvp::Unknown {
                    invitee.rsvp = previous.rsvp;
                }
            }
        }
        meeting.invitees = invitees;
    }
}

fn supports_event_fetch(tools: &Value) -> bool {
    tools
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| {
            tools.iter().any(|tool| {
                tool.get("name").and_then(Value::as_str) == Some("fetch")
                    && tool
                        .pointer("/inputSchema/properties/entityUrls/type")
                        .and_then(Value::as_str)
                        == Some("array")
                    && tool
                        .pointer("/inputSchema/properties/entityUrls/items/type")
                        .and_then(Value::as_str)
                        == Some("string")
                    && tool
                        .pointer("/inputSchema/required")
                        .and_then(Value::as_array)
                        .is_none_or(|required| {
                            required
                                .iter()
                                .all(|field| field.as_str() == Some("entityUrls"))
                        })
            })
        })
}

fn enrich_meeting_details(process: &mut McpProcess, meetings: &mut [Meeting], timeout: Duration) {
    let urls = meetings
        .iter()
        .filter_map(|meeting| meeting.entity_url.as_deref())
        .filter(|url| url.starts_with("/me/events/") || url.starts_with("/me/calendar/events/"))
        .collect::<Vec<_>>();
    let calls = urls
        .chunks(5)
        .map(|urls| json!({"name": "fetch", "arguments": {"entityUrls": urls}}))
        .collect();
    let responses = process.request_many("tools/call", calls, timeout);
    for response in responses {
        // WorkIQ can flag a whole batch as an error when only one row is throttled.
        let rows = response
            .as_ref()
            .ok()
            .and_then(|response| response.pointer("/result/structuredContent/results"))
            .and_then(Value::as_array);
        let Some(rows) = rows else {
            tracing::warn!(
                failure_class = "meeting_details_unavailable",
                "Calendar RSVP details could not be retrieved"
            );
            continue;
        };
        for row in rows {
            let status_code = row.get("statusCode").and_then(Value::as_u64);
            let Some(details) = row
                .get("data")
                .filter(|_| status_code.is_some_and(|code| (200..300).contains(&code)))
            else {
                tracing::warn!(
                    failure_class = "meeting_details_unavailable",
                    status_code,
                    "Calendar RSVP details could not be retrieved"
                );
                continue;
            };
            if let Some(meeting) = meetings.iter_mut().find(|meeting| {
                details.get("id").and_then(Value::as_str) == Some(meeting.id.as_str())
            }) {
                apply_meeting_details(meeting, details);
            }
        }
    }
}

fn collect_meetings(value: &Value, results: &mut Vec<Meeting>, incomplete: &mut bool) {
    match value {
        Value::Object(object) => {
            let id = string_field(object, &["id", "eventId", "meetingId"]);
            let start = ["start", "startDateTime", "startTime"]
                .iter()
                .find_map(|key| object.get(*key).and_then(parse_timestamp));
            let end = ["end", "endDateTime", "endTime"]
                .iter()
                .find_map(|key| object.get(*key).and_then(parse_timestamp));
            let title = display_field(object, &["subject", "title", "name"]);

            if let Some(id) = id.filter(|id| !id.is_empty()) {
                if let (Some(start), Some(end)) = (start, end) {
                    results.push(Meeting {
                        id: id.to_owned(),
                        title,
                        start: Some(start),
                        end: Some(end),
                        organizer: object.get("organizer").and_then(person_contact),
                        invitees: parse_invitees(object.get("attendees")),
                        your_rsvp: value
                            .pointer("/responseStatus/response")
                            .and_then(Value::as_str)
                            .and_then(parse_rsvp),
                        entity_url: None,
                        cancelled: object
                            .get("isCancelled")
                            .or_else(|| object.get("cancelled"))
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        declined: object
                            .get("isDeclined")
                            .or_else(|| object.get("declined"))
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        all_day: object
                            .get("isAllDay")
                            .or_else(|| object.get("allDay"))
                            .or_else(|| object.get("all_day"))
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    });
                } else if title.is_some() {
                    *incomplete = true;
                }
            }
            for child in object.values() {
                collect_meetings(child, results, incomplete);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_meetings(child, results, incomplete);
            }
        }
        _ => {}
    }
}

pub fn parse_meeting_results(
    response: &Value,
    now: DateTime<FixedOffset>,
) -> Result<Vec<Meeting>, WorkiqError> {
    if response.get("isError").and_then(Value::as_bool) == Some(true) {
        let message = response
            .pointer("/content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|content| content.get("text").and_then(Value::as_str))
            .map(|text| json!({"message": text}))
            .any(|message| authentication_error(&message));
        return Err(if message {
            WorkiqError::AuthenticationRequired
        } else {
            WorkiqError::LookupFailed
        });
    }
    let mut meetings = Vec::new();
    let mut incomplete = false;
    let retrieval_results = response
        .get("structuredContent")
        .and_then(|content| content.get(RETRIEVAL_CONTENT_TYPE))
        .and_then(|retrieval| retrieval.get("results"))
        .and_then(Value::as_array);
    if let Some(results) = retrieval_results {
        let results = results
            .iter()
            .filter(|result| is_meeting_result(result))
            .collect::<Vec<_>>();
        let reference = reference_offset(&results, now);
        let reference_now = now.with_timezone(&reference);
        let mut unparsed = 0_usize;
        for result in results {
            match parse_retrieval_meeting(result, reference_now, *now.offset()) {
                Some(meeting) => meetings.push(meeting),
                None => unparsed += 1,
            }
        }
        if unparsed > 0 {
            tracing::warn!(
                unparsed,
                parsed = meetings.len(),
                "Some WorkIQ meeting results had an unrecognised format"
            );
            incomplete = true;
        }
    } else {
        collect_meetings(response, &mut meetings, &mut incomplete);
        for text in response
            .pointer("/content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|content| content.get("text").and_then(Value::as_str))
        {
            if let Ok(value) = serde_json::from_str::<Value>(text.trim()) {
                collect_meetings(&value, &mut meetings, &mut incomplete);
            }
        }
    }
    if meetings.is_empty() && incomplete {
        return Err(WorkiqError::InvalidResponse);
    }
    meetings.sort_by_key(|left| left.start);
    meetings.dedup_by(|left, right| left.id == right.id);
    Ok(meetings)
}

fn is_meeting_result(result: &Value) -> bool {
    result
        .get("entityType")
        .and_then(Value::as_str)
        .is_none_or(|kind| matches!(kind, "Meetings" | "Meeting" | "Events" | "Event"))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MeetingTiming {
    InProgress,
    Past,
    StartsIn(TimeDelta),
}

/// Reads WorkIQ's timezone-independent `timeUntilMeeting`, for example `event is in progress`,
/// `event is in the past`, `3 minutes` or `1 day, 3 hours and 53 minutes`.
fn parse_time_until(text: &str) -> Option<MeetingTiming> {
    let text = text.trim().to_ascii_lowercase();
    if text.contains("in progress") {
        return Some(MeetingTiming::InProgress);
    }
    if text.contains("in the past") {
        return Some(MeetingTiming::Past);
    }
    let mut total = TimeDelta::zero();
    for part in text.split(',').flat_map(|part| part.split(" and ")) {
        let mut words = part.split_whitespace();
        let (Some(amount), Some(unit), None) = (words.next(), words.next(), words.next()) else {
            return None;
        };
        let amount = amount.parse::<i64>().ok()?;
        total += match unit.trim_end_matches('s') {
            "day" => TimeDelta::try_days(amount)?,
            "hour" => TimeDelta::try_hours(amount)?,
            "minute" => TimeDelta::try_minutes(amount)?,
            "second" => TimeDelta::try_seconds(amount)?,
            _ => return None,
        };
    }
    Some(MeetingTiming::StartsIn(total))
}

fn retrieval_timing(result: &Value) -> Option<(String, Option<&str>, MeetingTiming)> {
    let markdown = result.get("markdown").and_then(Value::as_str)?;
    let (_, mut fields) = markdown_fields(markdown);
    let timing = parse_time_until(fields.get("timeUntilMeeting")?)?;
    let hint = result
        .pointer("/resourceMetadata/meeting/startTime")
        .and_then(Value::as_str);
    Some((fields.remove("time")?, hint, timing))
}

fn timing_matches(result: &Value, reference_now: DateTime<FixedOffset>) -> bool {
    let Some((time, hint, timing)) = retrieval_timing(result) else {
        return false;
    };
    let Some((start, end)) = parse_time_range(&time, hint, reference_now) else {
        return false;
    };
    match timing {
        MeetingTiming::InProgress => {
            start <= reference_now + TIMING_TOLERANCE && reference_now - TIMING_TOLERANCE < end
        }
        MeetingTiming::Past => end <= reference_now + TIMING_TOLERANCE,
        MeetingTiming::StartsIn(delay) => {
            (start - (reference_now + delay)).abs() <= TIMING_TOLERANCE
        }
    }
}

/// WorkIQ writes relative meeting times (`today at 1 PM`) in UTC, not the user's timezone.
/// Each result's `timeUntilMeeting` confirms which reading is right, so a WorkIQ version that
/// switches to local time is still read correctly. Ties keep the observed UTC behavior.
fn reference_offset(results: &[&Value], now: DateTime<FixedOffset>) -> FixedOffset {
    let utc = FixedOffset::east_opt(0).expect("UTC is a valid offset");
    let local = *now.offset();
    if local == utc {
        return utc;
    }
    let score = |offset: FixedOffset| {
        let reference_now = now.with_timezone(&offset);
        results
            .iter()
            .filter(|result| timing_matches(result, reference_now))
            .count()
    };
    let (utc_score, local_score) = (score(utc), score(local));
    let reference = if local_score > utc_score { local } else { utc };
    tracing::debug!(
        utc_score,
        local_score,
        reference_offset_minutes = reference.local_minus_utc() / 60,
        "Calibrated WorkIQ meeting times"
    );
    reference
}

fn parse_retrieval_meeting(
    result: &Value,
    reference_now: DateTime<FixedOffset>,
    local: FixedOffset,
) -> Option<Meeting> {
    let object = result.as_object()?;
    let id = string_field(object, &["resourceId", "entityUrl", "id"])
        .map(str::trim)
        .filter(|id| !id.is_empty())?;
    let metadata = result.pointer("/resourceMetadata/meeting");
    let markdown = object
        .get("markdown")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let (heading, fields) = markdown_fields(markdown);
    let title = metadata
        .and_then(|meeting| meeting.get("title"))
        .and_then(Value::as_str)
        .and_then(clean_display_text)
        .or_else(|| {
            heading
                .as_deref()
                .and_then(|heading| clean_display_text(&unescape_markdown(heading)))
        });
    let start_date_hint = metadata
        .and_then(|meeting| meeting.get("startTime"))
        .and_then(Value::as_str);
    let (start, end) = parse_time_range(fields.get("time")?, start_date_hint, reference_now)?;
    let (start, end) = (start.with_timezone(&local), end.with_timezone(&local));
    let organizer = fields
        .get("organizer")
        .and_then(|value| parse_embedded_json(value))
        .and_then(|organizer| person_contact(&organizer));
    let invitees = fields
        .get("invitees")
        .and_then(|value| parse_embedded_json(value))
        .map(|invitees| parse_invitees(Some(&invitees)))
        .unwrap_or_default();
    let your_rsvp = fields
        .get("additionalMetadata")
        .and_then(|value| parse_embedded_json(value))
        .and_then(|metadata| metadata.as_array().cloned())
        .into_iter()
        .flatten()
        .find_map(|item| {
            if item.get("key").and_then(Value::as_str) == Some("RSVP Info") {
                item.get("value")
                    .and_then(Value::as_str)
                    .and_then(|value| value.strip_prefix("Your response: "))
                    .and_then(parse_rsvp)
            } else {
                None
            }
        });
    let declined = your_rsvp == Some(Rsvp::Declined);
    let cancelled = title.as_deref().is_some_and(|title| {
        let title = title.to_ascii_lowercase();
        title.starts_with("canceled:") || title.starts_with("cancelled:")
    });
    let all_day = object
        .get("isAllDay")
        .or_else(|| object.get("allDay"))
        .or_else(|| object.get("all_day"))
        .or_else(|| metadata.and_then(|meeting| meeting.get("isAllDay")))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    Some(Meeting {
        id: id.to_owned(),
        title,
        start: Some(start),
        end: Some(end),
        organizer,
        invitees,
        your_rsvp,
        entity_url: object
            .get("entityUrl")
            .and_then(Value::as_str)
            .map(str::to_owned),
        cancelled,
        declined,
        all_day,
    })
}

/// Reads WorkIQ's `- **field:** value` meeting summary lines. The first bold line without a
/// field name is the meeting heading.
fn markdown_fields(markdown: &str) -> (Option<String>, HashMap<String, String>) {
    let mut heading = None;
    let mut fields = HashMap::new();
    for line in markdown.lines() {
        let Some(line) = line.trim_start().strip_prefix("- **") else {
            continue;
        };
        if let Some((name, value)) = line.split_once(":**")
            && !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric())
        {
            fields
                .entry(name.to_owned())
                .or_insert_with(|| value.trim().to_owned());
            continue;
        }
        if heading.is_none() {
            heading = line.strip_suffix("**").map(str::to_owned);
        }
    }
    (heading, fields)
}

fn unescape_markdown(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\\'
            && let Some(next) = characters
                .peek()
                .copied()
                .filter(char::is_ascii_punctuation)
        {
            output.push(next);
            characters.next();
            continue;
        }
        output.push(character);
    }
    output
}

fn parse_embedded_json(value: &str) -> Option<Value> {
    serde_json::from_str(&unescape_markdown(value.trim())).ok()
}

fn strip_tags(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut in_tag = false;
    for character in text.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output
}

fn decode_html_entities(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find('&') {
        output.push_str(&rest[..index]);
        let candidate = &rest[index..];
        let decoded = candidate
            .find(';')
            .filter(|end| *end <= 10)
            .and_then(|end| {
                let entity = &candidate[1..end];
                let character = match entity {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    _ => entity.strip_prefix('#').and_then(|number| {
                        let code = match number.strip_prefix(['x', 'X']) {
                            Some(hex) => u32::from_str_radix(hex, 16).ok(),
                            None => number.parse().ok(),
                        };
                        code.and_then(char::from_u32)
                    }),
                };
                character.map(|character| (character, end + 1))
            });
        match decoded {
            Some((character, length)) => {
                output.push(character);
                rest = &candidate[length..];
            }
            None => {
                output.push('&');
                rest = &candidate[1..];
            }
        }
    }
    output.push_str(rest);
    output
}

fn clean_display_text(text: &str) -> Option<String> {
    let cleaned = decode_html_entities(&strip_tags(text));
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    (!cleaned.is_empty()).then_some(cleaned)
}

fn person_contact(value: &Value) -> Option<String> {
    let (name, email) = person_details(value);
    format_contact(name.as_deref(), email.as_deref())
}

fn person_details(value: &Value) -> (Option<String>, Option<String>) {
    if let Some(text) = value.as_str() {
        let text = text.trim();
        return ((!text.is_empty()).then(|| text.to_owned()), None);
    }
    let Some(object) = value.as_object() else {
        return (None, None);
    };
    let nested = object
        .get("emailAddress")
        .map(person_details)
        .unwrap_or_default();
    let name = ["displayName", "name"]
        .iter()
        .filter_map(|key| object.get(*key).and_then(Value::as_str))
        .find_map(clean_display_text)
        .or(nested.0);
    let addresses = match object.get("emailAddresses") {
        Some(Value::String(text)) => serde_json::from_str::<Value>(text).ok(),
        other => other.cloned(),
    };
    let email = ["address", "email"]
        .iter()
        .filter_map(|key| object.get(*key).and_then(Value::as_str))
        .find_map(clean_display_text)
        .or(nested.1)
        .or_else(|| {
            addresses
                .as_ref()?
                .as_array()?
                .iter()
                .filter_map(|address| address.get("address").and_then(Value::as_str))
                .find_map(clean_display_text)
        });
    (name, email)
}

fn parse_clock(text: &str) -> Option<NaiveTime> {
    let text = text.trim().to_ascii_uppercase();
    let (clock, afternoon) = if let Some(clock) = text.strip_suffix("AM") {
        (clock.trim_end(), Some(false))
    } else if let Some(clock) = text.strip_suffix("PM") {
        (clock.trim_end(), Some(true))
    } else {
        (text.as_str(), None)
    };
    let (hour, minute) = match clock.split_once(':') {
        Some((hour, minute)) => (hour.parse::<u32>().ok()?, minute.parse::<u32>().ok()?),
        None => (clock.parse::<u32>().ok()?, 0),
    };
    let hour = match afternoon {
        Some(afternoon) if (1..=12).contains(&hour) => hour % 12 + if afternoon { 12 } else { 0 },
        Some(_) => return None,
        None => hour,
    };
    NaiveTime::from_hms_opt(hour, minute, 0)
}

fn relative_day(word: &str, today: NaiveDate) -> Option<NaiveDate> {
    match word.trim().to_ascii_lowercase().as_str() {
        "today" => Some(today),
        "yesterday" => today.pred_opt(),
        "tomorrow" => today.succ_opt(),
        _ => None,
    }
}

fn weekday_of(word: &str) -> Option<Weekday> {
    word.split_whitespace().last()?.parse().ok()
}

fn parse_month_day_year(text: &str) -> Option<NaiveDate> {
    let mut parts = text.trim().split('/').map(|part| part.parse::<u32>().ok());
    let (month, day, year) = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() {
        return None;
    }
    NaiveDate::from_ymd_opt(i32::try_from(year).ok()?, month, day)
}

/// Converts WorkIQ's relative meeting time (for example `today at 9 AM to today at 9:30 AM`
/// or `last Friday at 10 AM to today at 10 AM`) into absolute timestamps in `now`'s offset,
/// which must be the offset WorkIQ wrote the times in. `startTime` metadata (`M/D/YYYY`)
/// anchors weekday-relative starts.
fn parse_time_range(
    text: &str,
    start_date_hint: Option<&str>,
    now: DateTime<FixedOffset>,
) -> Option<(DateTime<FixedOffset>, DateTime<FixedOffset>)> {
    let today = now.date_naive();
    let (start_text, end_text) = text.trim().split_once(" to ")?;
    let (start_day, start_clock) = start_text.trim().rsplit_once(" at ")?;
    let (end_day, end_clock) = end_text.trim().rsplit_once(" at ")?;

    let start_date = relative_day(start_day, today).or_else(|| {
        let hint = start_date_hint.and_then(parse_month_day_year)?;
        match weekday_of(start_day) {
            Some(weekday) if hint.weekday() != weekday => [hint.pred_opt(), hint.succ_opt()]
                .into_iter()
                .flatten()
                .find(|date| date.weekday() == weekday),
            _ => Some(hint),
        }
    })?;
    let end_date = if end_day.trim().eq_ignore_ascii_case(start_day.trim()) {
        start_date
    } else {
        relative_day(end_day, today).or_else(|| {
            let weekday = weekday_of(end_day)?;
            (0..7)
                .filter_map(|offset| start_date.checked_add_days(Days::new(offset)))
                .find(|date| date.weekday() == weekday)
        })?
    };

    let offset = now.offset();
    let start = offset
        .from_local_datetime(&start_date.and_time(parse_clock(start_clock)?))
        .single()?;
    let end = offset
        .from_local_datetime(&end_date.and_time(parse_clock(end_clock)?))
        .single()?;
    (end >= start).then_some((start, end))
}

struct McpProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    responses: Receiver<Result<Value, WorkiqError>>,
    next_id: u64,
    timeout: Duration,
}

impl McpProcess {
    fn start(executable: &Path, timeout: Duration) -> Result<Self, WorkiqError> {
        if !executable.is_file() {
            return Err(WorkiqError::ExecutableMissing);
        }
        let mut child = Command::new(executable)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| WorkiqError::StartFailed)?;
        let stdout = child.stdout.take().ok_or(WorkiqError::StartFailed)?;
        if let Some(mut stderr) = child.stderr.take() {
            thread::spawn(move || {
                let mut sink = std::io::sink();
                let _ = std::io::copy(&mut stderr, &mut sink);
            });
        }
        let (sender, responses) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                let bytes = match reader
                    .by_ref()
                    .take(MAX_MESSAGE_BYTES + 1)
                    .read_until(b'\n', &mut line)
                {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        let _ = sender.send(Err(WorkiqError::InvalidResponse));
                        break;
                    }
                };
                if bytes == 0 {
                    break;
                }
                if bytes as u64 > MAX_MESSAGE_BYTES {
                    let _ = sender.send(Err(WorkiqError::InvalidResponse));
                    break;
                }
                match serde_json::from_slice::<Value>(&line) {
                    Ok(value) => {
                        if sender.send(Ok(value)).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        let _ = sender.send(Err(WorkiqError::InvalidResponse));
                        break;
                    }
                }
            }
        });
        let stdin = child.stdin.take().ok_or(WorkiqError::StartFailed)?;
        Ok(Self {
            child,
            stdin: Some(stdin),
            responses,
            next_id: 1,
            timeout,
        })
    }

    fn send(&mut self, message: &Value) -> Result<(), WorkiqError> {
        let mut line = serde_json::to_vec(message).map_err(|_| WorkiqError::InvalidResponse)?;
        line.push(b'\n');
        let stdin = self.stdin.as_mut().ok_or(WorkiqError::ProcessExited)?;
        stdin
            .write_all(&line)
            .and_then(|()| stdin.flush())
            .map_err(|_| WorkiqError::ProcessExited)
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, WorkiqError> {
        self.request_with_timeout(method, params, self.timeout)
    }

    fn request_with_timeout(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, WorkiqError> {
        self.request_many(method, vec![params], timeout)
            .pop()
            .unwrap_or(Err(WorkiqError::InvalidResponse))
    }

    /// Sends every request before waiting so WorkIQ can work on them concurrently. Results
    /// keep the order of `params`; all requests share one deadline.
    fn request_many(
        &mut self,
        method: &str,
        params: Vec<Value>,
        timeout: Duration,
    ) -> Vec<Result<Value, WorkiqError>> {
        let mut results = vec![None; params.len()];
        let mut pending = Vec::new();
        for (index, params) in params.into_iter().enumerate() {
            let id = self.next_id;
            self.next_id += 1;
            match self.send(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params
            })) {
                Ok(()) => pending.push((id, index)),
                Err(error) => results[index] = Some(Err(error)),
            }
        }
        let deadline = Instant::now() + timeout;
        while !pending.is_empty() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let failure = if remaining.is_zero() {
                Some(WorkiqError::TimedOut)
            } else {
                match self.responses.recv_timeout(remaining) {
                    Ok(Ok(response)) => {
                        let id = response.get("id").and_then(Value::as_u64);
                        if let Some(position) =
                            pending.iter().position(|(sent, _)| Some(*sent) == id)
                        {
                            let (_, index) = pending.swap_remove(position);
                            results[index] = Some(response_result(response));
                        }
                        None
                    }
                    Ok(Err(error)) => Some(error),
                    Err(RecvTimeoutError::Timeout) => Some(WorkiqError::TimedOut),
                    Err(RecvTimeoutError::Disconnected) => Some(WorkiqError::ProcessExited),
                }
            };
            if let Some(error) = failure {
                for (_, index) in pending.drain(..) {
                    results[index] = Some(Err(error.clone()));
                }
            }
        }
        results
            .into_iter()
            .map(|result| result.unwrap_or(Err(WorkiqError::InvalidResponse)))
            .collect()
    }

    fn notify(&mut self, method: &str) -> Result<(), WorkiqError> {
        self.send(&json!({"jsonrpc":"2.0","method":method}))
    }
}

impl Drop for McpProcess {
    fn drop(&mut self) {
        self.stdin.take();
        let deadline = Instant::now() + SHUTDOWN_GRACE;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(25));
                }
                _ => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Clone, Debug)]
pub struct WorkiqClient {
    executable: PathBuf,
    timeout: Duration,
    lookup_timeout: Duration,
}

impl WorkiqClient {
    pub fn new(executable: PathBuf) -> Self {
        Self {
            executable,
            timeout: DEFAULT_REQUEST_TIMEOUT,
            lookup_timeout: DEFAULT_LOOKUP_TIMEOUT,
        }
    }

    /// Applies one timeout to the handshake and to the meeting search.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self.lookup_timeout = timeout;
        self
    }

    pub fn discover_meetings(
        &self,
        now: DateTime<FixedOffset>,
    ) -> Result<Vec<Meeting>, WorkiqError> {
        let started = Instant::now();
        let mut process = McpProcess::start(&self.executable, self.timeout)?;
        let initialized = process.request(
            "initialize",
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "NoteIQ", "version": env!("CARGO_PKG_VERSION")}
            }),
        )?;
        if initialized.pointer("/result/protocolVersion").is_none() {
            return Err(WorkiqError::InvalidResponse);
        }
        log_phase("initialize", started);
        process.notify("notifications/initialized")?;
        let tools = process.request("tools/list", json!({}))?;
        let tool =
            discover_meeting_search_tool(&tools)?.ok_or(WorkiqError::UnsupportedCapability)?;
        log_phase("tools_list", started);
        let calls = MEETING_QUERIES
            .iter()
            .map(|query| {
                json!({"name": tool.name, "arguments": meeting_search_arguments(&tool, query)})
            })
            .collect();
        let responses = process.request_many("tools/call", calls, self.lookup_timeout);
        log_phase("meeting_search", started);
        let mut meetings = Vec::new();
        let mut seen = HashSet::new();
        let mut failure: Option<WorkiqError> = None;
        let mut succeeded = 0_usize;
        for (query, response) in MEETING_QUERIES.iter().zip(responses) {
            let parsed = response.and_then(|response| {
                let result = response.get("result").ok_or(WorkiqError::InvalidResponse)?;
                parse_meeting_results(result, now)
            });
            match parsed {
                Ok(found) => {
                    succeeded += 1;
                    tracing::debug!(
                        query,
                        meetings = found.len(),
                        "WorkIQ meeting query completed"
                    );
                    meetings.extend(
                        found
                            .into_iter()
                            .filter(|meeting| seen.insert(meeting.id.clone())),
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        query,
                        failure_class = ?error.failure_class(),
                        "WorkIQ meeting query failed"
                    );
                    if !matches!(failure, Some(WorkiqError::AuthenticationRequired)) {
                        failure = Some(error);
                    }
                }
            }
        }
        if succeeded == 0 {
            return Err(failure.unwrap_or(WorkiqError::InvalidResponse));
        }
        if supports_event_fetch(&tools) {
            enrich_meeting_details(&mut process, &mut meetings, self.timeout);
        }
        // WorkIQ may take the full shutdown grace period to exit; don't hold results back.
        thread::spawn(move || drop(process));
        meetings.sort_by_key(|meeting| meeting.start);
        tracing::info!(
            meetings = meetings.len(),
            queries_succeeded = succeeded,
            elapsed_ms = elapsed_ms(started),
            "WorkIQ meeting lookup completed"
        );
        Ok(meetings)
    }
}

fn response_result(response: Value) -> Result<Value, WorkiqError> {
    match response.get("error") {
        Some(error) if authentication_error(error) => Err(WorkiqError::AuthenticationRequired),
        Some(_) => Err(WorkiqError::InvalidResponse),
        None => Ok(response),
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn log_phase(phase: &'static str, started: Instant) {
    tracing::debug!(
        phase,
        elapsed_ms = elapsed_ms(started),
        "WorkIQ phase completed"
    );
}
