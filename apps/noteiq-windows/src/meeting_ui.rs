use chrono::{DateTime, FixedOffset};
use noteiq_core::{
    meeting::{MeetingSelection, apply_automatic_selection, select_current_meeting},
    model::{Capture, Meeting},
};

pub fn select_and_apply(
    capture: &mut Capture,
    meetings: &[Meeting],
    now: DateTime<FixedOffset>,
    auto_connect: bool,
) -> MeetingSelection {
    let selection = select_current_meeting(meetings, now, auto_connect);
    apply_automatic_selection(capture, selection.clone());
    selection
}
