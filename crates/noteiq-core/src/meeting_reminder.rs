//! Decides when to suggest writing a note after an untranscribed meeting ends. Only meetings
//! the user was in a call for are suggested.

use crate::{
    markdown::contact_name,
    meeting::same_occurrence,
    model::{Meeting, Rsvp},
};
use chrono::{DateTime, FixedOffset, SecondsFormat, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, io, path::Path};
use tempfile::NamedTempFile;

/// Meetings that ended longer ago than this are never suggested, so starting NoteIQ late in
/// the day does not raise a reminder for every meeting of the morning.
pub const REMINDER_WINDOW: TimeDelta = TimeDelta::hours(2);
/// Sent reminders are remembered this long so a restart never repeats one.
pub const HISTORY_RETENTION: TimeDelta = TimeDelta::days(7);
/// Longer delays would push reminders past [`REMINDER_WINDOW`].
pub const MAX_DELAY_MINUTES: u32 = 60;
/// A reminder Windows failed to show is retried this many times in all, then given up.
pub const MAX_DELIVERY_ATTEMPTS: u32 = 3;
/// Events at least this long are all-day or multi-day blocks, not meetings to take notes on.
const ALL_DAY: TimeDelta = TimeDelta::hours(24);

/// The `meeting_reminder_delay_minutes` setting as a duration, capped at
/// [`MAX_DELAY_MINUTES`].
pub fn reminder_delay(minutes: u32) -> TimeDelta {
    TimeDelta::minutes(i64::from(minutes.min(MAX_DELAY_MINUTES)))
}

/// A short, stable identifier for one meeting occurrence, safe to put in a `noteiq://` link.
/// The start is hashed in UTC, so the key survives a time zone or daylight saving change.
pub fn reminder_key(meeting: &Meeting) -> String {
    // FNV-1a: stable across builds and platforms, unlike `DefaultHasher`.
    let start = meeting
        .start
        .map(|start| {
            start
                .with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::AutoSi, true)
        })
        .unwrap_or_default();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in meeting
        .id
        .bytes()
        .chain(std::iter::once(b'\n'))
        .chain(start.bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

pub fn is_reminder_key(key: &str) -> bool {
    key.len() == 16
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Whether the meeting itself is worth a note, regardless of timing: it took place, the user
/// did not decline it, it is not an all-day or personal block, and it was not transcribed.
/// An unknown transcript status still qualifies; a needless reminder costs one dismiss.
pub fn wants_note(meeting: &Meeting) -> bool {
    let (Some(start), Some(end)) = (meeting.start, meeting.end) else {
        return false;
    };
    !meeting.cancelled
        && !meeting.declined
        && meeting.your_rsvp != Some(Rsvp::Declined)
        && !meeting.all_day
        && start < end
        && end - start < ALL_DAY
        && meeting.has_transcript != Some(true)
        && !is_personal_block(meeting)
}

/// Lunch, travel or focus time: the user organised it and invited nobody.
fn is_personal_block(meeting: &Meeting) -> bool {
    meeting.your_rsvp == Some(Rsvp::Organizer) && meeting.invitees.is_empty()
}

/// When a reminder for `meeting` becomes due.
pub fn due_at(meeting: &Meeting, delay: TimeDelta) -> Option<DateTime<FixedOffset>> {
    meeting.end.map(|end| end + delay)
}

/// The reminder window starts when the meeting ends, or when the call still going on at its
/// end does, so a meeting that ran over or led into another call is not forgotten.
fn within_window(meeting: &Meeting, now: DateTime<FixedOffset>, history: &ReminderHistory) -> bool {
    meeting.end.is_some_and(|end| {
        let ended = history
            .call_end_at(end)
            .map_or(end, |call_end| call_end.max(end));
        end <= now && now - ended <= REMINDER_WINDOW
    })
}

/// Meetings stay tracked this long after they end, longer than [`REMINDER_WINDOW`] so a
/// reminder delayed by a long call still finds its meeting.
const TRACKING_WINDOW: TimeDelta = TimeDelta::hours(12);

/// Meetings seen in calendar lookups, kept until their reminder window has passed.
#[derive(Clone, Debug, Default)]
pub struct ReminderTracker {
    meetings: Vec<Meeting>,
    last_lookup: Option<DateTime<FixedOffset>>,
}

impl ReminderTracker {
    /// Records a successful lookup that started at `looked_up_at`. A newer copy of a meeting
    /// replaces the older one, so a transcript that became available is noticed.
    pub fn observe(&mut self, meetings: &[Meeting], looked_up_at: DateTime<FixedOffset>) {
        for meeting in meetings {
            match self
                .meetings
                .iter_mut()
                .find(|known| same_occurrence(known, meeting))
            {
                Some(known) => *known = meeting.clone(),
                None => self.meetings.push(meeting.clone()),
            }
        }
        self.meetings.retain(|meeting| {
            meeting
                .end
                .is_some_and(|end| looked_up_at - end <= TRACKING_WINDOW)
        });
        self.last_lookup = self.last_lookup.max(Some(looked_up_at));
    }

    fn pending<'a>(
        &'a self,
        now: DateTime<FixedOffset>,
        delay: TimeDelta,
        history: &'a ReminderHistory,
        noted: &'a [Meeting],
    ) -> impl Iterator<Item = (&'a Meeting, DateTime<FixedOffset>)> + 'a {
        self.meetings.iter().filter_map(move |meeting| {
            let due = due_at(meeting, delay)?;
            (due <= now
                && within_window(meeting, now, history)
                && wants_note(meeting)
                && history.attended(meeting)
                && !history.blocks(meeting)
                && !noted.iter().any(|noted| same_occurrence(noted, meeting)))
            .then_some((meeting, due))
        })
    }

    /// True when a reminder may be due but no lookup has run since, so the transcript status
    /// is not yet known.
    pub fn needs_lookup(
        &self,
        now: DateTime<FixedOffset>,
        delay: TimeDelta,
        history: &ReminderHistory,
        noted: &[Meeting],
    ) -> bool {
        self.pending(now, delay, history, noted)
            .any(|(_, due)| self.last_lookup.is_none_or(|lookup| lookup < due))
    }

    /// Meetings to remind about now. Each is judged on a lookup that started after its
    /// reminder became due, so a transcript that appeared after the meeting ended counts.
    /// `noted` holds meetings already connected to a note.
    pub fn due(
        &self,
        now: DateTime<FixedOffset>,
        delay: TimeDelta,
        history: &ReminderHistory,
        noted: &[Meeting],
    ) -> Vec<Meeting> {
        self.pending(now, delay, history, noted)
            .filter(|(_, due)| self.last_lookup.is_some_and(|lookup| lookup >= *due))
            .map(|(meeting, _)| meeting.clone())
            .collect()
    }
}

/// A reminder that was shown, kept so it is not shown again and so its link can reopen the
/// meeting after a restart.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SentReminder {
    pub key: String,
    pub meeting: Meeting,
    pub sent_at: DateTime<FixedOffset>,
}

/// A reminder handed to Windows that has not been confirmed yet.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Delivery {
    meeting: Meeting,
    attempts: u32,
    in_flight: bool,
}

/// A stretch of time the user was in a call, from the first to the last check that saw it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CallSpan {
    pub start: DateTime<FixedOffset>,
    pub end: DateTime<FixedOffset>,
}

/// Checks closer together than this belong to the same call. Checks run every 30 seconds.
const CALL_GAP: TimeDelta = TimeDelta::minutes(2);
/// Calls are remembered this long, enough to match them to meetings looked up later.
const CALL_RETENTION: TimeDelta = TimeDelta::hours(24);

/// Reminders already shown and the calls the user was in, stored next to the settings file,
/// and the reminders being delivered.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReminderHistory {
    #[serde(default)]
    pub reminders: Vec<SentReminder>,
    /// Kept apart from meetings, so a call is matched to meetings whenever their details
    /// arrive, even after NoteIQ restarts.
    #[serde(default)]
    pub calls: Vec<CallSpan>,
    /// Only shown reminders are saved: one that never appeared is tried again after a restart.
    #[serde(skip)]
    deliveries: HashMap<String, Delivery>,
}

impl ReminderHistory {
    /// A missing or unreadable history starts empty: at worst a reminder repeats once.
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|_| {
                tracing::warn!(failure_class = "reminder_history_invalid");
                Self::default()
            }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(_) => {
                tracing::warn!(failure_class = "reminder_history_unreadable");
                Self::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent folder"))?;
        fs::create_dir_all(parent)?;
        let contents = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let mut temporary = NamedTempFile::new_in(parent)?;
        io::Write::write_all(&mut temporary, contents.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|error| error.error)?;
        Ok(())
    }

    /// Whether a reminder for this occurrence was shown. Occurrences are also compared by
    /// instant, so entries saved with an older key format still count.
    pub fn contains(&self, meeting: &Meeting) -> bool {
        let key = reminder_key(meeting);
        self.reminders
            .iter()
            .any(|sent| sent.key == key || same_occurrence(&sent.meeting, meeting))
    }

    /// Whether no reminder should be started now: it was shown, is being delivered, or
    /// failed [`MAX_DELIVERY_ATTEMPTS`] times.
    pub fn blocks(&self, meeting: &Meeting) -> bool {
        self.contains(meeting)
            || self
                .deliveries
                .get(&reminder_key(meeting))
                .is_some_and(|delivery| {
                    delivery.in_flight || delivery.attempts >= MAX_DELIVERY_ATTEMPTS
                })
    }

    /// Records a delivery attempt and returns the key for the reminder's link. The reminder
    /// counts as shown only after [`Self::delivered`].
    pub fn begin(&mut self, meeting: &Meeting) -> String {
        let key = reminder_key(meeting);
        let delivery = self
            .deliveries
            .entry(key.clone())
            .or_insert_with(|| Delivery {
                meeting: meeting.clone(),
                attempts: 0,
                in_flight: false,
            });
        delivery.attempts += 1;
        delivery.in_flight = true;
        key
    }

    /// Windows showed the reminder, or it turned out not to be needed.
    pub fn delivered(&mut self, key: &str, sent_at: DateTime<FixedOffset>) {
        let Some(delivery) = self.deliveries.remove(key) else {
            return;
        };
        self.record(&delivery.meeting, sent_at);
    }

    /// The attempt failed; it is retried until [`MAX_DELIVERY_ATTEMPTS`] is reached.
    pub fn failed(&mut self, key: &str) {
        if let Some(delivery) = self.deliveries.get_mut(key) {
            delivery.in_flight = false;
        }
    }

    /// Reminders were turned off before the reminder appeared. It is neither recorded nor
    /// counted as an attempt, so turning reminders back on can still show it.
    pub fn cancelled(&mut self, key: &str) {
        self.deliveries.remove(key);
    }

    /// Records a shown reminder and returns the key for its link.
    pub fn record(&mut self, meeting: &Meeting, sent_at: DateTime<FixedOffset>) -> String {
        let key = reminder_key(meeting);
        if !self.contains(meeting) {
            self.reminders.push(SentReminder {
                key: key.clone(),
                meeting: meeting.clone(),
                sent_at,
            });
        }
        key
    }

    /// The meeting a reminder link refers to.
    pub fn meeting(&self, key: &str) -> Option<&Meeting> {
        self.reminders
            .iter()
            .find(|sent| sent.key == key)
            .map(|sent| &sent.meeting)
            .or_else(|| self.deliveries.get(key).map(|delivery| &delivery.meeting))
    }

    /// Forgets reminders older than [`HISTORY_RETENTION`] and calls older than a day. Returns
    /// whether anything was removed.
    pub fn prune(&mut self, now: DateTime<FixedOffset>) -> bool {
        let before = (self.reminders.len(), self.calls.len());
        self.reminders
            .retain(|sent| now - sent.sent_at <= HISTORY_RETENTION);
        self.calls.retain(|call| now - call.end <= CALL_RETENTION);
        (self.reminders.len(), self.calls.len()) != before
    }

    /// Records that the user is in a call at `now`, extending the latest call when the
    /// previous check also saw one. Returns whether a new call started, which is worth saving.
    pub fn record_call(&mut self, now: DateTime<FixedOffset>) -> bool {
        if let Some(last) = self.calls.last_mut()
            && now >= last.end
            && now - last.end <= CALL_GAP
        {
            last.end = now;
            return false;
        }
        self.calls.push(CallSpan {
            start: now,
            end: now,
        });
        true
    }

    /// Whether the user was in a call at some point while the meeting was scheduled.
    pub fn attended(&self, meeting: &Meeting) -> bool {
        let (Some(start), Some(end)) = (meeting.start, meeting.end) else {
            return false;
        };
        self.calls
            .iter()
            .any(|call| call.start < end && call.end >= start)
    }

    /// When the call going on at `instant` ended, as far as the checks have seen.
    fn call_end_at(&self, instant: DateTime<FixedOffset>) -> Option<DateTime<FixedOffset>> {
        self.calls
            .iter()
            .find(|call| call.start <= instant && instant <= call.end)
            .map(|call| call.end)
    }
}

/// What a reminder says about its meeting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReminderText {
    /// `Write a quick note for Project review?`
    pub title: String,
    /// `14:00–14:30 · Alex Morgan`
    pub details: String,
    pub body: String,
}

/// What the reminder says, with meeting times shown at `display` (the user's current offset),
/// since calendar times can arrive in UTC or an offset the user has since left.
pub fn reminder_text(meeting: &Meeting, display: FixedOffset) -> ReminderText {
    let title = meeting
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map_or_else(
            || "Write a quick note about your meeting?".to_owned(),
            |title| format!("Write a quick note for {title}?"),
        );
    let time = match (meeting.start, meeting.end) {
        (Some(start), Some(end)) => Some(format!(
            "{}\u{2013}{}",
            start.with_timezone(&display).format("%H:%M"),
            end.with_timezone(&display).format("%H:%M")
        )),
        _ => None,
    };
    let organizer = meeting
        .organizer
        .as_deref()
        .map(contact_name)
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let details = time
        .into_iter()
        .chain(organizer.map(str::to_owned))
        .collect::<Vec<_>>()
        .join(" \u{b7} ");
    let body = if meeting.has_transcript == Some(false) {
        "This meeting wasn't transcribed. Capture the key points while they're fresh."
    } else {
        "No transcript was found. Capture the key points while they're fresh."
    };
    ReminderText {
        title,
        details,
        body: body.to_owned(),
    }
}
