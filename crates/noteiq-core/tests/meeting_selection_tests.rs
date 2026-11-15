mod common;

use chrono::DateTime;
use common::{instant, meeting};
use noteiq_core::meeting::{
    MeetingSelection, choice_label, list_entry, meeting_choices, meetings_in_progress,
    select_current_meeting,
};
use noteiq_core::model::Meeting;

fn at(clock: &str) -> chrono::DateTime<chrono::FixedOffset> {
    chrono::DateTime::parse_from_rfc3339(&format!("2026-11-03T{clock}:00+01:00")).unwrap()
}

fn timed(id: &str, start: &str, end: &str) -> Meeting {
    let mut timed = meeting();
    timed.id = id.to_owned();
    timed.start = Some(at(start));
    timed.end = Some(at(end));
    timed
}

fn all_day(id: &str) -> Meeting {
    let mut event = meeting();
    event.id = id.to_owned();
    event.title = Some("Company holiday".to_owned());
    event.start = Some(at("00:00"));
    event.end = Some(at("00:00") + chrono::TimeDelta::days(1));
    event.all_day = true;
    event
}

fn second_meeting() -> Meeting {
    let mut other = meeting();
    other.id = "event-43".to_owned();
    other.title = Some("Budget review".to_owned());
    other
}

#[test]
fn auto_connects_only_when_exactly_one_current_meeting_is_eligible() {
    let selected = select_current_meeting(&[meeting()], instant(), true);
    assert!(matches!(selected, MeetingSelection::Connected(_)));

    let overlap = select_current_meeting(&[meeting(), second_meeting()], instant(), true);
    assert!(matches!(overlap, MeetingSelection::NeedsChoice(events) if events.len() == 2));
}

#[test]
fn distinguishes_no_match_and_disabled_auto_connect() {
    assert_eq!(
        select_current_meeting(&[], instant(), true),
        MeetingSelection::NoMeeting
    );
    assert!(matches!(
        select_current_meeting(&[meeting()], instant(), false),
        MeetingSelection::NeedsChoice(events) if events.len() == 1
    ));
}

#[test]
fn ignores_cancelled_declined_and_out_of_window_events() {
    let mut cancelled = meeting();
    cancelled.cancelled = true;
    let mut declined = meeting();
    declined.declined = true;
    let long_ended = timed("event-old", "13:00", "13:55");
    let much_later = timed("event-later", "14:31", "15:00");
    let mut reversed = meeting();
    reversed.end = reversed.start;

    let candidates = [cancelled, declined, long_ended, much_later, reversed];
    assert_eq!(
        select_current_meeting(&candidates, instant(), true),
        MeetingSelection::NoMeeting
    );
    assert!(meeting_choices(&candidates, instant()).is_empty());
}

#[test]
fn offers_meetings_that_start_or_ended_within_fifteen_minutes() {
    let starting = timed("event-next", "14:30", "15:00");
    let ended = timed("event-prev", "13:30", "14:00");

    match select_current_meeting(&[ended.clone(), starting.clone()], instant(), true) {
        MeetingSelection::NeedsChoice(events) => assert_eq!(events, [starting, ended]),
        other => panic!("expected a choice, got {other:?}"),
    }
}

#[test]
fn connects_the_single_in_progress_meeting_even_with_nearby_ones() {
    let starting = timed("event-next", "14:20", "15:00");

    assert_eq!(
        select_current_meeting(&[starting, meeting()], instant(), true),
        MeetingSelection::Connected(meeting())
    );
}

#[test]
fn counts_only_distinct_timed_meetings_in_progress() {
    let starting = timed("event-next", "14:20", "15:00");
    let ended = timed("event-prev", "13:30", "14:00");
    let mut cancelled = timed("event-cancelled", "14:00", "14:30");
    cancelled.cancelled = true;
    let candidates = [
        meeting(),
        meeting(),
        second_meeting(),
        starting,
        ended,
        all_day("event-all-day"),
        cancelled,
    ];

    assert_eq!(meetings_in_progress(&candidates, instant()), 2);
    assert_eq!(meetings_in_progress(&[], instant()), 0);
}

#[test]
fn orders_choices_by_relevance_then_closeness() {
    let early = timed("event-early", "13:00", "15:00");
    let late = timed("event-late", "14:10", "14:45");
    let soon = timed("event-soon", "14:25", "15:00");
    let sooner = timed("event-sooner", "14:20", "15:00");
    let ended = timed("event-ended", "13:30", "14:05");
    let all_day = all_day("event-all-day");

    let choices = meeting_choices(
        &[all_day, ended, soon, early, sooner, late.clone(), late],
        instant(),
    );

    let ids = choices
        .iter()
        .map(|meeting| meeting.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "event-late",
            "event-early",
            "event-sooner",
            "event-soon",
            "event-ended",
            "event-all-day"
        ]
    );
}

#[test]
fn all_day_events_alone_never_prompt_but_are_offered_on_request() {
    let all_day = all_day("event-all-day");

    assert_eq!(
        select_current_meeting(std::slice::from_ref(&all_day), instant(), true),
        MeetingSelection::NoMeeting
    );
    let choices = meeting_choices(std::slice::from_ref(&all_day), instant());
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0], all_day);
    assert!(matches!(
        select_current_meeting(&[all_day, meeting()], instant(), true),
        MeetingSelection::Connected(selected) if selected.id == "event-42"
    ));
}

#[test]
fn explicit_all_day_events_are_not_auto_connected_across_spring_dst() {
    let start = DateTime::parse_from_rfc3339("2026-03-29T00:00:00+01:00").unwrap();
    let end = DateTime::parse_from_rfc3339("2026-03-30T00:00:00+02:00").unwrap();
    let mut event = meeting();
    event.start = Some(start);
    event.end = Some(end);
    event.all_day = true;

    assert_eq!(
        select_current_meeting(&[event.clone()], at("12:00"), true),
        MeetingSelection::NoMeeting
    );
    assert!(choice_label(&event).ends_with("all day"));
}

#[test]
fn list_entries_show_time_title_and_details_for_each_state() {
    let mut current = timed("now", "14:00", "14:30");
    current.organizer = Some("Alex Morgan <alex@contoso.com>".to_owned());
    current.invitees = vec![common::invitee("A"), common::invitee("B")];
    let entry = list_entry(&current, instant());
    assert_eq!(
        (
            entry.time.as_str(),
            entry.title.as_str(),
            entry.details.as_str()
        ),
        (
            "14:00",
            "Project review",
            "ends 14:30 · Alex Morgan · 2 people"
        )
    );
    assert!(entry.now && !entry.past);
    assert!(!entry.transcribed, "an unknown transcript is not shown");
    current.has_transcript = Some(true);
    assert!(list_entry(&current, instant()).transcribed);

    let ended = list_entry(&timed("ended", "09:00", "10:00"), instant());
    assert_eq!(ended.details, "Ended 10:00 · Organizer · 1 person");
    assert!(ended.past && !ended.now);

    let mut later = timed("later", "15:35", "16:05");
    later.your_rsvp = Some(noteiq_core::model::Rsvp::Tentative);
    later.organizer = None;
    later.invitees.clear();
    let entry = list_entry(&later, instant());
    assert_eq!(entry.details, "In 1 h 20 min · 30 min · Tentative");
    assert!(!entry.now && !entry.past);

    let mut tomorrow = timed("tomorrow", "09:00", "10:00");
    tomorrow.start = tomorrow
        .start
        .map(|start| start + chrono::TimeDelta::days(1));
    tomorrow.end = tomorrow.end.map(|end| end + chrono::TimeDelta::days(1));
    assert!(
        list_entry(&tomorrow, instant())
            .details
            .starts_with("Tomorrow · 1 h")
    );
    let mut friday = tomorrow.clone();
    friday.start = friday.start.map(|start| start + chrono::TimeDelta::days(2));
    friday.end = friday.end.map(|end| end + chrono::TimeDelta::days(2));
    assert!(
        list_entry(&friday, instant())
            .details
            .starts_with("Fri 6 Nov · 1 h")
    );
}

#[test]
fn list_entries_handle_all_day_untitled_and_other_offsets() {
    let mut event = all_day("holiday");
    event.organizer = None;
    event.invitees.clear();
    let entry = list_entry(&event, instant());
    assert_eq!(
        (entry.time.as_str(), entry.details.as_str()),
        ("All day", "")
    );
    assert!(entry.now);

    let mut untitled = meeting();
    untitled.title = Some("  ".to_owned());
    // 13:00 UTC is 14:00 in the note's +01:00 offset.
    untitled.start = Some(DateTime::parse_from_rfc3339("2026-11-03T13:00:00+00:00").unwrap());
    let entry = list_entry(&untitled, instant());
    assert_eq!(
        (entry.time.as_str(), entry.title.as_str()),
        ("14:00", "Untitled meeting")
    );
}

#[test]
fn choice_labels_show_local_times_or_all_day() {
    assert_eq!(
        choice_label(&meeting()),
        "Project review · 03 Nov · 14:00–14:30"
    );
    assert_eq!(
        choice_label(&all_day("event-all-day")),
        "Company holiday · 03 Nov · all day"
    );
    let mut untitled = meeting();
    untitled.title = None;
    untitled.end = None;
    assert_eq!(choice_label(&untitled), "Untitled meeting");
}

#[test]
fn browsing_includes_older_meetings_without_changing_auto_connect() {
    let morning = timed("morning", "09:00", "10:00");
    let earlier = timed("earlier", "12:00", "13:00");
    let later = timed("later", "16:00", "17:00");
    let mut cancelled = morning.clone();
    cancelled.id = "cancelled".into();
    cancelled.cancelled = true;
    let candidates = [
        morning.clone(),
        later,
        earlier,
        meeting(),
        morning,
        cancelled,
    ];
    let choices = noteiq_core::meeting::browse_meetings(&candidates, instant());
    assert_eq!(
        choices
            .iter()
            .map(|meeting| meeting.id.as_str())
            .collect::<Vec<_>>(),
        ["event-42", "earlier", "morning", "later"]
    );
    assert_eq!(
        select_current_meeting(&candidates, instant(), true),
        MeetingSelection::Connected(meeting())
    );
}
