use chrono::{DateTime, FixedOffset};
use noteiq_core::{
    meeting::{MeetingSelection, select_current_meeting},
    workiq::{
        MEETING_QUERIES, MeetingSearchTool, WorkiqError, discover_meeting_search_tool,
        meeting_search_arguments, parse_meeting_results,
    },
};
use serde_json::{Value, json};

fn at(text: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(text).unwrap()
}

/// Tuesday 3 November 2026, 14:15 local time.
fn now() -> DateTime<FixedOffset> {
    at("2026-11-03T14:15:00+01:00")
}

fn retrieval_fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/workiq/retrieval_today.json")).unwrap()
}

fn retrieval(results: Vec<Value>) -> Value {
    json!({
        "content": [],
        "structuredContent": {
            "application/vnd.ms-workiq.retrieval": {
                "results": results,
                "stoppedReason": "completed"
            }
        },
        "isError": false
    })
}

fn timed_result(time: &str, start_hint: &str) -> Value {
    json!({
        "markdown": format!("- **Timed meeting**\n  - **time:** {time}\n  - **numberOfInvitees:** 0"),
        "id": "h1",
        "resourceId": "AAMkTimedEvent=",
        "entityType": "Meetings",
        "resourceMetadata": {"meeting": {"title": "<title>Timed meeting</title>", "startTime": start_hint}}
    })
}

fn timed_result_with_timing(time: &str, start_hint: &str, until: &str) -> Value {
    json!({
        "markdown": format!(
            "- **Timed meeting**\n  - **time:** {time}\n  - **timeUntilMeeting:** {until}\n  - **numberOfInvitees:** 0"
        ),
        "id": "h1",
        "resourceId": "AAMkTimedEvent=",
        "entityType": "Meetings",
        "resourceMetadata": {"meeting": {"title": "<title>Timed meeting</title>", "startTime": start_hint}}
    })
}

fn parse_single_range(
    time: &str,
    start_hint: &str,
) -> Result<(DateTime<FixedOffset>, DateTime<FixedOffset>), WorkiqError> {
    let meetings = parse_meeting_results(&retrieval(vec![timed_result(time, start_hint)]), now())?;
    assert_eq!(meetings.len(), 1);
    Ok((meetings[0].start.unwrap(), meetings[0].end.unwrap()))
}

#[test]
fn discovers_the_scoped_meeting_retrieval_tool_from_runtime_schema() {
    let schema = json!({
        "result": {
            "tools": [{
                "name": "retrieve",
                "description": "Search indexed M365 data.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "strategy": {"type": ["string", "null"]},
                        "capabilities": {
                            "type": ["array", "null"],
                            "items": {"anyOf": [
                                {"properties": {"name": {"const": "People"}}},
                                {"properties": {"name": {"const": "Meetings"}}}
                            ]}
                        }
                    },
                    "required": ["query"]
                }
            }]
        }
    });

    assert_eq!(
        discover_meeting_search_tool(&schema).unwrap(),
        Some(MeetingSearchTool {
            name: "retrieve".to_owned(),
            query_property: "query".to_owned(),
            meetings_capability: true,
        })
    );
}

#[test]
fn does_not_guess_an_unbounded_or_non_meeting_search_tool() {
    let schema = json!({
        "result": {
            "tools": [{
                "name": "ask",
                "inputSchema": {
                    "type": "object",
                    "properties": {"question": {"type": "string"}},
                    "required": ["question"]
                }
            }]
        }
    });

    assert_eq!(discover_meeting_search_tool(&schema).unwrap(), None);
}

#[test]
fn rejects_malformed_tool_lists() {
    assert!(discover_meeting_search_tool(&json!({"result":{"tools":"unexpected"}})).is_err());
}

#[test]
fn parses_meetings_without_losing_invitees_or_status_flags() {
    let response = json!({
        "result": {
            "meetings": [{
                "eventId": "event-42",
                "subject": "Design review",
                "start": "2026-11-03T13:00:00+01:00",
                "end": "2026-11-03T13:30:00+01:00",
                "organizer": {"email": "organizer@example.test"},
                "attendees": [{"displayName": "Attendee"}],
                "isCancelled": false,
                "isDeclined": false
            }]
        }
    });

    let meetings = parse_meeting_results(&response, now()).unwrap();

    assert_eq!(meetings.len(), 1);
    assert_eq!(meetings[0].id, "event-42");
    assert_eq!(meetings[0].title.as_deref(), Some("Design review"));
    assert_eq!(meetings[0].invitees[0].display_contact(), "Attendee");
    assert_eq!(
        meetings[0].invitees[0].role,
        noteiq_core::model::InviteeRole::Unknown
    );
    assert_eq!(
        meetings[0].organizer.as_deref(),
        Some("organizer@example.test")
    );
}

#[test]
fn rejects_tool_errors_and_incomplete_meeting_records() {
    assert!(matches!(
        parse_meeting_results(&json!({"isError": true}), now()),
        Err(WorkiqError::LookupFailed)
    ));
    assert!(matches!(
        parse_meeting_results(
            &json!({
                "isError": true,
                "content": [{"text": "Authentication required. Please sign in again."}]
            }),
            now()
        ),
        Err(WorkiqError::AuthenticationRequired)
    ));
    assert!(matches!(
        parse_meeting_results(
            &json!({
                "meetings": [{
                    "eventId": "event-42",
                    "subject": "Missing times"
                }]
            }),
            now()
        ),
        Err(WorkiqError::InvalidResponse)
    ));
}

#[test]
fn asks_for_current_upcoming_and_todays_meetings_scoped_to_meetings() {
    let tool = MeetingSearchTool {
        name: "retrieve".to_owned(),
        query_property: "query".to_owned(),
        meetings_capability: true,
    };

    assert_eq!(
        MEETING_QUERIES,
        [
            "My calendar meetings happening now",
            "My calendar meetings starting in the next hour",
            "My calendar meetings today",
        ]
    );
    for query in MEETING_QUERIES {
        let arguments = meeting_search_arguments(&tool, query);

        assert_eq!(arguments["query"], query);
        assert_eq!(arguments["capabilities"], json!([{"name": "Meetings"}]));
        assert_eq!(arguments["strategy"], "grounding");
    }
}

#[test]
fn parses_the_real_workiq_retrieval_format() {
    let meetings = parse_meeting_results(&retrieval_fixture(), now()).unwrap();

    let ids = meetings
        .iter()
        .map(|meeting| meeting.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "AAMkAGFakeEventId01AAA=",
            "AAMkAGFakeEventId02AAA=",
            "AAMkAGFakeEventId03AAA="
        ],
        "the all-day result has no clock time and is skipped"
    );

    let standup = &meetings[0];
    assert_eq!(standup.title.as_deref(), Some("Morning standup"));
    assert_eq!(standup.start, Some(at("2026-11-03T09:00:00+01:00")));
    assert_eq!(standup.end, Some(at("2026-11-03T09:15:00+01:00")));
    assert!(standup.invitees.is_empty());

    let review = &meetings[1];
    assert_eq!(review.title.as_deref(), Some("Project review & planning"));
    assert_eq!(review.start, Some(at("2026-11-03T14:00:00+01:00")));
    assert_eq!(review.end, Some(at("2026-11-03T14:30:00+01:00")));
    assert_eq!(
        review.organizer.as_deref(),
        Some("Organizer Name <organizer@example.test>")
    );
    assert_eq!(
        review.invitees[0].display_contact(),
        "Attendee One <one@example.test>"
    );
    assert_eq!(
        review.invitees[1].display_contact(),
        "Attendee Two <two@example.test>"
    );
    assert_eq!(
        review.invitees[0].role,
        noteiq_core::model::InviteeRole::Required
    );
    assert_eq!(
        review.invitees[1].role,
        noteiq_core::model::InviteeRole::Optional
    );
    assert_eq!(review.your_rsvp, Some(noteiq_core::model::Rsvp::Accepted));
    assert!(!review.cancelled);
    assert!(!review.declined);

    let town_hall = &meetings[2];
    assert_eq!(town_hall.title.as_deref(), Some("Town hall"));
    assert!(town_hall.declined, "RSVP Info 'Declined' marks the meeting");
}

#[test]
fn plain_object_parser_preserves_all_day_flag_across_spring_dst() {
    let start = "2026-03-29T00:00:00+01:00";
    let end = "2026-03-30T00:00:00+02:00";
    let reference = at("2026-03-29T12:00:00+02:00");
    let response = json!({"meetings":[{
        "id":"dst-holiday",
        "subject":"Company holiday",
        "start":start,
        "end":end,
        "isAllDay":true
    }]});

    let meetings = parse_meeting_results(&response, reference).unwrap();
    assert_eq!(meetings.len(), 1);
    assert!(meetings[0].all_day);
    assert_eq!(
        select_current_meeting(&meetings, reference, true),
        MeetingSelection::NoMeeting
    );
}

#[test]
fn selects_the_only_accepted_meeting_that_is_active_now() {
    let meetings = parse_meeting_results(&retrieval_fixture(), now()).unwrap();

    match select_current_meeting(&meetings, now(), true) {
        MeetingSelection::Connected(meeting) => {
            assert_eq!(meeting.id, "AAMkAGFakeEventId02AAA=");
            assert_eq!(meeting.title.as_deref(), Some("Project review & planning"));
        }
        other => panic!("expected the project review to connect, got {other:?}"),
    }
    assert_eq!(
        select_current_meeting(&meetings, at("2026-11-03T16:00:00+01:00"), true),
        MeetingSelection::NoMeeting
    );
}

#[test]
fn reads_titles_from_the_markdown_heading_when_metadata_is_missing() {
    let response = retrieval(vec![json!({
        "markdown": "- **Canceled: Budget \\[Q4\\] &#124; review &amp; sign\\-off**\n  - **time:** today at 2 PM to today at 3 PM\n  - **organizer:** {\"displayName\":\"\",\"emailAddresses\":\"\\[{\\\\u0022address\\\\u0022:\\\\u0022owner\\_name@example.test\\\\u0022}\\]\"}\n  - **invitees:** \\[{\"type\":\"Required\",\"emailAddress\":{\"address\":\"only\\_address@example.test\"}}\\]",
        "entityUrl": "/me/events/AAMkFallbackId%3D",
        "entityType": "Meetings"
    })]);

    let meetings = parse_meeting_results(&response, now()).unwrap();

    assert_eq!(meetings.len(), 1);
    let meeting = &meetings[0];
    assert_eq!(meeting.id, "/me/events/AAMkFallbackId%3D");
    assert_eq!(
        meeting.title.as_deref(),
        Some("Canceled: Budget [Q4] | review & sign-off")
    );
    assert!(meeting.cancelled);
    assert_eq!(
        meeting.organizer.as_deref(),
        Some("owner_name@example.test")
    );
    assert_eq!(
        meeting.invitees[0].display_contact(),
        "only_address@example.test"
    );
}

#[test]
fn ignores_non_meeting_retrieval_results() {
    let mut email = timed_result("today at 2 PM to today at 3 PM", "11/3/2026");
    email["entityType"] = json!("Email");

    assert!(
        parse_meeting_results(&retrieval(vec![email]), now())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn reports_an_invalid_response_when_no_retrieval_result_is_usable() {
    let response = retrieval(vec![
        timed_result("All day", "11/3/2026"),
        timed_result("today at 3 PM to today at 2 PM", "11/3/2026"),
    ]);

    assert!(matches!(
        parse_meeting_results(&response, now()),
        Err(WorkiqError::InvalidResponse)
    ));
}

#[test]
fn reads_relative_meeting_times_as_utc_and_returns_local_timestamps() {
    // Without `timeUntilMeeting` there is nothing to calibrate against, so WorkIQ's observed
    // UTC clock is assumed. `now()` is 13:15 UTC on 3 November.
    let cases = [
        (
            "today at 12 AM to today at 12:30 PM",
            "11/3/2026",
            "2026-11-03T00:00:00Z",
            "2026-11-03T12:30:00Z",
        ),
        (
            "today at 11:30 PM to tomorrow at 12:15 AM",
            "11/3/2026",
            "2026-11-03T23:30:00Z",
            "2026-11-04T00:15:00Z",
        ),
        (
            "yesterday at 13:00 to yesterday at 14:00",
            "11/2/2026",
            "2026-11-02T13:00:00Z",
            "2026-11-02T14:00:00Z",
        ),
        (
            "Monday at 9 AM to Thursday at 5 PM",
            "11/2/2026",
            "2026-11-02T09:00:00Z",
            "2026-11-05T17:00:00Z",
        ),
        (
            "last Friday at 10 AM to today at 10 AM",
            "10/30/2026",
            "2026-10-30T10:00:00Z",
            "2026-11-03T10:00:00Z",
        ),
        (
            "Monday at 9 AM to Monday at 10 AM",
            "11/3/2026",
            "2026-11-02T09:00:00Z",
            "2026-11-02T10:00:00Z",
        ),
    ];

    for (time, hint, start, end) in cases {
        let (parsed_start, parsed_end) = parse_single_range(time, hint).unwrap();
        assert_eq!((parsed_start, parsed_end), (at(start), at(end)), "{time}");
        assert_eq!(parsed_start.offset(), now().offset(), "{time}");
        assert_eq!(parsed_end.offset(), now().offset(), "{time}");
    }
}

#[test]
fn an_in_progress_meeting_written_in_utc_is_placed_at_its_local_time() {
    let response = retrieval(vec![timed_result_with_timing(
        "today at 1 PM to today at 1:30 PM",
        "11/3/2026",
        "event is in progress",
    )]);

    let meetings = parse_meeting_results(&response, now()).unwrap();

    assert_eq!(meetings[0].start, Some(at("2026-11-03T14:00:00+01:00")));
    assert_eq!(meetings[0].end, Some(at("2026-11-03T14:30:00+01:00")));
    assert!(matches!(
        select_current_meeting(&meetings, now(), true),
        MeetingSelection::Connected(_)
    ));
}

#[test]
fn utc_dates_are_used_for_today_and_tomorrow_around_local_midnight() {
    // 00:30 local on 4 November is still 23:30 UTC on 3 November.
    let just_after_midnight = at("2026-11-04T00:30:00+01:00");
    let response = retrieval(vec![timed_result_with_timing(
        "today at 11 PM to tomorrow at 12 AM",
        "11/3/2026",
        "event is in progress",
    )]);

    let meetings = parse_meeting_results(&response, just_after_midnight).unwrap();

    assert_eq!(meetings[0].start, Some(at("2026-11-04T00:00:00+01:00")));
    assert_eq!(meetings[0].end, Some(at("2026-11-04T01:00:00+01:00")));
}

#[test]
fn times_written_in_local_time_are_detected_from_time_until_meeting() {
    let cases = [
        (
            "today at 2 PM to today at 2:30 PM",
            "event is in progress",
            "2026-11-03T14:00:00+01:00",
        ),
        (
            "today at 2:18 PM to today at 3 PM",
            "3 minutes",
            "2026-11-03T14:18:00+01:00",
        ),
        (
            "tomorrow at 6:08 PM to tomorrow at 7 PM",
            "1 day, 3 hours and 53 minutes",
            "2026-11-04T18:08:00+01:00",
        ),
        (
            "today at 1 PM to today at 2 PM",
            "event is in the past",
            "2026-11-03T13:00:00+01:00",
        ),
    ];

    for (time, until, start) in cases {
        let response = retrieval(vec![timed_result_with_timing(time, "11/3/2026", until)]);

        let meetings = parse_meeting_results(&response, now()).unwrap();

        assert_eq!(meetings[0].start, Some(at(start)), "{time} / {until}");
    }
}

#[test]
fn upcoming_meetings_written_in_utc_keep_the_utc_reading() {
    let response = retrieval(vec![timed_result_with_timing(
        "today at 1:30 PM to today at 2 PM",
        "11/3/2026",
        "15 minutes",
    )]);

    let meetings = parse_meeting_results(&response, now()).unwrap();

    assert_eq!(meetings[0].start, Some(at("2026-11-03T14:30:00+01:00")));
}

#[test]
fn unreadable_time_until_meeting_falls_back_to_utc() {
    for until in ["soon", "about 3 minutes", "3 fortnights", ""] {
        let response = retrieval(vec![timed_result_with_timing(
            "today at 2 PM to today at 2:30 PM",
            "11/3/2026",
            until,
        )]);

        let meetings = parse_meeting_results(&response, now()).unwrap();

        assert_eq!(
            meetings[0].start,
            Some(at("2026-11-03T15:00:00+01:00")),
            "{until:?}"
        );
    }
}

#[test]
fn rejects_unrecognised_meeting_times() {
    for time in [
        "All day",
        "today at 13 PM to today at 2 PM",
        "today at 3 PM to today at 2 PM",
        "someday at 9 AM to someday at 10 AM",
        "today 9 AM - 10 AM",
    ] {
        assert!(
            matches!(
                parse_single_range(time, "not a date"),
                Err(WorkiqError::InvalidResponse)
            ),
            "{time}"
        );
    }
}
