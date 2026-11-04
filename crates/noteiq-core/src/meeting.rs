use crate::model::{Capture, Meeting, MeetingDecision};
use chrono::{DateTime, FixedOffset, TimeDelta};
use std::collections::HashSet;

/// Meetings that start or ended within this window are offered alongside current ones.
pub const CHOICE_WINDOW: TimeDelta = TimeDelta::minutes(15);
/// All-day and multi-day events span at least this long and are never connected automatically.
const ALL_DAY: TimeDelta = TimeDelta::hours(24);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MeetingSelection {
    NoMeeting,
    Connected(Meeting),
    NeedsChoice(Vec<Meeting>),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Relevance {
    InProgress,
    StartingSoon,
    RecentlyEnded,
    AllDay,
}

fn relevance(meeting: &Meeting, now: DateTime<FixedOffset>) -> Option<(Relevance, TimeDelta)> {
    if meeting.cancelled || meeting.declined {
        return None;
    }
    let (start, end) = (meeting.start?, meeting.end?);
    if end <= start {
        return None;
    }
    let in_progress = start <= now && now < end;
    if end - start >= ALL_DAY {
        return in_progress.then_some((Relevance::AllDay, now - start));
    }
    if in_progress {
        Some((Relevance::InProgress, now - start))
    } else if now < start && start - now <= CHOICE_WINDOW {
        Some((Relevance::StartingSoon, start - now))
    } else if end <= now && now - end <= CHOICE_WINDOW {
        Some((Relevance::RecentlyEnded, now - end))
    } else {
        None
    }
}

/// Eligible meetings in the order they are offered: in progress (latest start first), starting
/// soon, recently ended, then all-day or multi-day events.
fn ranked_choices(candidates: &[Meeting], now: DateTime<FixedOffset>) -> Vec<(Relevance, Meeting)> {
    let mut seen = HashSet::new();
    let mut ranked = candidates
        .iter()
        .filter_map(|meeting| Some((relevance(meeting, now)?, meeting)))
        .filter(|(_, meeting)| seen.insert(meeting.id.clone()))
        .collect::<Vec<_>>();
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked
        .into_iter()
        .map(|((relevance, _), meeting)| (relevance, meeting.clone()))
        .collect()
}

/// Every meeting worth offering when the user looks for one, most relevant first.
pub fn meeting_choices(candidates: &[Meeting], now: DateTime<FixedOffset>) -> Vec<Meeting> {
    ranked_choices(candidates, now)
        .into_iter()
        .map(|(_, meeting)| meeting)
        .collect()
}

/// The manual picker includes older retrieved meetings, independently of auto-connect's window.
pub fn browse_meetings(candidates: &[Meeting], now: DateTime<FixedOffset>) -> Vec<Meeting> {
    let mut seen = HashSet::new();
    let mut choices = candidates
        .iter()
        .filter_map(|meeting| {
            let (start, end) = (meeting.start?, meeting.end?);
            if meeting.cancelled
                || meeting.declined
                || end <= start
                || !seen.insert(meeting.id.clone())
            {
                return None;
            }
            let rank = if start <= now && now < end {
                (0, -start.timestamp())
            } else if end <= now {
                (1, -end.timestamp())
            } else {
                (2, start.timestamp())
            };
            Some((rank, meeting.clone()))
        })
        .collect::<Vec<_>>();
    choices.sort_by_key(|(rank, _)| *rank);
    choices.into_iter().map(|(_, meeting)| meeting).collect()
}

/// Picker text such as `Project review · 14:00–14:30`, so overlapping meetings with similar
/// titles can be told apart.
pub fn choice_label(meeting: &Meeting) -> String {
    let title = meeting.title.as_deref().unwrap_or("Untitled meeting");
    match (meeting.start, meeting.end) {
        (Some(start), Some(end)) if end - start >= ALL_DAY => {
            format!("{title} · {} · all day", start.format("%d %b"))
        }
        (Some(start), Some(end)) => {
            format!(
                "{title} · {} · {}–{}",
                start.format("%d %b"),
                start.format("%H:%M"),
                end.format("%H:%M")
            )
        }
        _ => title.to_owned(),
    }
}

/// Connects automatically only to a single timed meeting in progress. All-day events alone
/// never prompt, so a recurring all-day entry does not interrupt every note.
pub fn select_current_meeting(
    candidates: &[Meeting],
    now: DateTime<FixedOffset>,
    auto_connect: bool,
) -> MeetingSelection {
    let ranked = ranked_choices(candidates, now);
    if ranked
        .iter()
        .all(|(relevance, _)| *relevance == Relevance::AllDay)
    {
        return MeetingSelection::NoMeeting;
    }
    let mut in_progress = ranked
        .iter()
        .filter(|(relevance, _)| *relevance == Relevance::InProgress);
    if auto_connect && let (Some((_, meeting)), None) = (in_progress.next(), in_progress.next()) {
        return MeetingSelection::Connected(meeting.clone());
    }
    MeetingSelection::NeedsChoice(ranked.into_iter().map(|(_, meeting)| meeting).collect())
}

/// Timed meetings in progress, for telling the user how many overlap.
pub fn meetings_in_progress(candidates: &[Meeting], now: DateTime<FixedOffset>) -> usize {
    ranked_choices(candidates, now)
        .iter()
        .filter(|(relevance, _)| *relevance == Relevance::InProgress)
        .count()
}

/// Attaches a single in-progress meeting to a note that has no meeting yet. An automatic
/// attachment is sticky once the note has text or a file: later lookups only refresh its
/// details, so a meeting that ends or overlaps another never silently disappears. Explicit
/// choices are never changed.
///
/// Text written well before a meeting began is never filed under it: a note that already has
/// text only snaps to a meeting that started no later than [`CHOICE_WINDOW`] after the note was
/// created. An empty note snaps whenever a meeting is in progress, and an empty note that was
/// never saved moves on to the next meeting in progress, because nothing was written under the
/// earlier one.
pub fn apply_automatic_selection(capture: &mut Capture, selection: MeetingSelection) {
    let MeetingSelection::Connected(meeting) = selection else {
        return;
    };
    match capture.meeting_decision {
        MeetingDecision::Pending
            if capture.is_empty()
                || meeting
                    .start
                    .is_some_and(|start| start <= capture.created_at + CHOICE_WINDOW) =>
        {
            capture.auto_connect(meeting)
        }
        MeetingDecision::Pending => {}
        MeetingDecision::Automatic
            if (capture.is_empty() && capture.path.is_none())
                || capture
                    .meeting
                    .as_ref()
                    .is_some_and(|current| current.id == meeting.id) =>
        {
            capture.meeting = Some(meeting);
        }
        MeetingDecision::Automatic | MeetingDecision::Selected | MeetingDecision::Disconnected => {}
    }
}
