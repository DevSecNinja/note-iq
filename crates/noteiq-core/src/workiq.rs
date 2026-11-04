use crate::{diagnostics::FailureClass, model::Meeting};
use chrono::{DateTime, Datelike, Days, FixedOffset, NaiveDate, NaiveTime, TimeZone, Weekday};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
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
const MEETING_QUERY: &str = "My calendar meetings today";
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// A cold WorkIQ meeting search was observed taking about 18 seconds.
pub const DEFAULT_LOOKUP_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeetingSearchTool {
    pub name: String,
    pub query_property: String,
    pub meetings_capability: bool,
}

#[derive(Debug, Error)]
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

/// Asks for today's meetings rather than "meetings active now": WorkIQ search ranks by
/// relevance and may return upcoming meetings, so the current one is selected locally.
pub fn meeting_search_arguments(tool: &MeetingSearchTool) -> Value {
    json!({
        tool.query_property.clone(): MEETING_QUERY,
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

fn parse_attendees(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|attendee| {
            attendee.as_str().map(str::to_owned).or_else(|| {
                attendee
                    .as_object()
                    .and_then(|object| display_field(object, &["name", "displayName", "email"]))
            })
        })
        .collect()
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
                        organizer: object
                            .get("organizer")
                            .and_then(|value| value.as_object())
                            .and_then(|organizer| {
                                display_field(organizer, &["name", "displayName", "email"])
                            }),
                        attendees: parse_attendees(object.get("attendees")),
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
        let mut unparsed = 0_usize;
        for result in results.iter().filter(|result| is_meeting_result(result)) {
            match parse_retrieval_meeting(result, now) {
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

fn parse_retrieval_meeting(result: &Value, now: DateTime<FixedOffset>) -> Option<Meeting> {
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
    let (start, end) = parse_time_range(fields.get("time")?, start_date_hint, now)?;
    let organizer = fields
        .get("organizer")
        .and_then(|value| parse_embedded_json(value))
        .and_then(|organizer| person_name(&organizer));
    let attendees = fields
        .get("invitees")
        .and_then(|value| parse_embedded_json(value))
        .and_then(|invitees| invitees.as_array().cloned())
        .into_iter()
        .flatten()
        .filter_map(|invitee| person_name(&invitee))
        .collect();
    let declined = fields
        .get("additionalMetadata")
        .and_then(|value| parse_embedded_json(value))
        .and_then(|metadata| metadata.as_array().cloned())
        .into_iter()
        .flatten()
        .any(|item| {
            item.get("key").and_then(Value::as_str) == Some("RSVP Info")
                && item
                    .get("value")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.to_ascii_lowercase().contains("declined"))
        });
    let cancelled = title.as_deref().is_some_and(|title| {
        let title = title.to_ascii_lowercase();
        title.starts_with("canceled:") || title.starts_with("cancelled:")
    });

    Some(Meeting {
        id: id.to_owned(),
        title,
        start: Some(start),
        end: Some(end),
        organizer,
        attendees,
        cancelled,
        declined,
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

fn person_name(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    if let Some(name) = ["displayName", "name"]
        .iter()
        .filter_map(|key| object.get(*key).and_then(Value::as_str))
        .find_map(clean_display_text)
    {
        return Some(name);
    }
    if let Some(name) = object.get("emailAddress").and_then(person_name) {
        return Some(name);
    }
    if let Some(address) = object
        .get("address")
        .and_then(Value::as_str)
        .and_then(clean_display_text)
    {
        return Some(address);
    }
    let addresses = match object.get("emailAddresses")? {
        Value::String(text) => serde_json::from_str::<Value>(text).ok()?,
        other => other.clone(),
    };
    addresses
        .as_array()?
        .iter()
        .filter_map(|address| address.get("address").and_then(Value::as_str))
        .find_map(clean_display_text)
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

/// Converts WorkIQ's local, relative meeting time (for example
/// `today at 9 AM to today at 9:30 AM` or `last Friday at 10 AM to today at 10 AM`) into
/// absolute timestamps. `startTime` metadata (`M/D/YYYY`) anchors weekday-relative starts.
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
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))?;
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(WorkiqError::TimedOut);
            }
            let response = match self.responses.recv_timeout(remaining) {
                Ok(response) => response?,
                Err(RecvTimeoutError::Timeout) => return Err(WorkiqError::TimedOut),
                Err(RecvTimeoutError::Disconnected) => return Err(WorkiqError::ProcessExited),
            };
            if response.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = response.get("error") {
                    return Err(if authentication_error(error) {
                        WorkiqError::AuthenticationRequired
                    } else {
                        WorkiqError::InvalidResponse
                    });
                }
                return Ok(response);
            }
        }
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
        let arguments = meeting_search_arguments(&tool);
        let result = process.request_with_timeout(
            "tools/call",
            json!({"name": tool.name, "arguments": arguments}),
            self.lookup_timeout,
        )?;
        log_phase("meeting_search", started);
        // WorkIQ may take the full shutdown grace period to exit; don't hold results back.
        thread::spawn(move || drop(process));
        let result = result.get("result").ok_or(WorkiqError::InvalidResponse)?;
        let meetings = parse_meeting_results(result, now)?;
        tracing::info!(
            meetings = meetings.len(),
            elapsed_ms = elapsed_ms(started),
            "WorkIQ meeting lookup completed"
        );
        Ok(meetings)
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
