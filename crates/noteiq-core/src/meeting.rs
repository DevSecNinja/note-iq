use crate::model::{Capture, Meeting, MeetingDecision};
use chrono::{DateTime, FixedOffset};
use std::collections::HashSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MeetingSelection {
    NoMeeting,
    Connected(Meeting),
    NeedsChoice(Vec<Meeting>),
}

pub fn select_current_meeting(
    candidates: &[Meeting],
    now: DateTime<FixedOffset>,
    auto_connect: bool,
) -> MeetingSelection {
    let mut seen = HashSet::new();
    let active = candidates
        .iter()
        .filter(|meeting| {
            if meeting.cancelled || meeting.declined {
                return false;
            }
            let (Some(start), Some(end)) = (meeting.start, meeting.end) else {
                return false;
            };
            start <= now && now < end && end > start && seen.insert(meeting.id.clone())
        })
        .cloned()
        .collect::<Vec<_>>();

    match active.as_slice() {
        [] => MeetingSelection::NoMeeting,
        [meeting] if auto_connect => MeetingSelection::Connected(meeting.clone()),
        _ => MeetingSelection::NeedsChoice(active),
    }
}

pub fn apply_automatic_selection(capture: &mut Capture, selection: MeetingSelection) {
    if capture.meeting_decision == MeetingDecision::Selected
        || capture.meeting_decision == MeetingDecision::Disconnected
    {
        return;
    }
    match selection {
        MeetingSelection::Connected(meeting) => capture.auto_connect(meeting),
        MeetingSelection::NoMeeting | MeetingSelection::NeedsChoice(_) => {
            capture.meeting = None;
            capture.meeting_decision = MeetingDecision::Pending;
        }
    }
}
