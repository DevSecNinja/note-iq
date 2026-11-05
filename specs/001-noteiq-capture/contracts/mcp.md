# Local WorkIQ MCP Contract

## Process discovery

On Windows, prefer the architecture-matched WorkIQ executable packaged with
the installed Scout application. A locally configured executable path overrides
discovery. Never install, download, or copy a WorkIQ executable.

Expected entry point: `workiq.exe mcp`, using stdin/stdout for MCP and stderr
for diagnostics. Do not pass note body text to the meeting lookup.

## Capability discovery

1. Start the process with a bounded lifetime and captured standard streams.
2. Send MCP `initialize` with a supported protocol version, client identity, and
   client capabilities.
3. Validate the successful JSON-RPC response and send
   `notifications/initialized`.
4. Request `tools/list`; require a valid list of unique tool names and schemas.
5. Prefer a discovered meeting-search/retrieval capability. Validate its input
   schema before invoking it; never guess required parameters.
6. If no compatible meeting search is available, show an actionable
   unsupported/unavailable state and keep note capture usable.

## Meeting retrieval

Call the discovered `retrieve` tool with three queries, sent together in one
WorkIQ process and answered concurrently: `My calendar meetings happening now`,
`My calendar meetings starting in the next hour`, and `My calendar meetings
today`. Each uses the `Meetings` capability and the `grounding` strategy. Never
send note text. WorkIQ returns a variable subset of meetings per query ranked by
relevance, so the day query alone misses current and upcoming meetings on busy
days. Results are merged and de-duplicated by meeting ID; the lookup fails only
when every query fails, preferring a sign-in failure over other classes.
Parse `structuredContent["application/vnd.ms-workiq.retrieval"].results[]`
(see research.md for the field format) into candidate meetings with validated
start/end times, then select the current meeting locally. WorkIQ writes
relative clock times in UTC; each response is calibrated against the
timezone-independent `timeUntilMeeting` field (UTC unless local time matches
more results) and converted to local time. Older or alternative
builds that return plain meeting objects (`id`/`eventId`, RFC 3339
`start`/`end`) are still accepted. If the actual tool does not expose bounded
meeting search, do not call a general natural-language question that returns
unrelated email or chat content; show the user a clear limitation instead.

Timeouts: 15 seconds per handshake request, 45 seconds shared by the meeting
queries. If `tools/list` exposes `fetch` with an `entityUrls` string-array input
and no unsupported required arguments, fetch returned `/me/events/` or
`/me/calendar/events/` paths in batches of five under a shared 15-second timeout.
Match response event IDs before applying organizer, invitees, and RSVP details.
Fetch is optional: failures log a sanitized warning and preserve retrieval data.
Batch `isError` can represent partial throttling: apply successful 2xx rows even
when the batch is flagged, retaining retrieval metadata for failed rows and
logging only their HTTP status code.

Invitees retain their required/optional/resource/unknown role, name, email,
`status.response` RSVP, and optional `hasAttended` observation. A calendar fetch
must preserve retrieval attendance by matching email addresses, even when
display names differ. `responseStatus.response` and retrieval's RSVP Info give
the signed-in user's response, not every invitee's response. `hasAttended`
is reported attendance, never a live call roster; absent values are unknown.

Selection rule: cancelled, declined, and zero-length meetings are ignored.
Exactly one timed meeting in progress is auto-selected when auto-connect is
enabled and the last lookup succeeded; all-day and multi-day events (24 hours
or longer) never auto-connect. Overlapping meetings are never guessed. A note
without a meeting choice attaches only while it is empty or when the meeting
started no later than 15 minutes after the note was created; an automatic
meeting moves on to another only while the note is empty and has never been
saved. While a capture is open, the cached list is re-evaluated every
60 seconds without a WorkIQ request, so an empty note snaps to a meeting
within a minute of its start.

The Meeting list always starts with No meeting, followed by every valid
retrieved meeting — in progress (latest start first), then ended (most recent
end first), then upcoming (soonest first) — labelled with title, date, and
local start/end times and without ordinal numbers. It is independent of the
automatic selection window: older meetings remain selectable while another is
attached, and an attached meeting stays listed even when a refresh no longer
returns it. Choosing a meeting or No meeting is an explicit decision that
takes precedence over late results, refreshes, and re-evaluation for that
note; New note starts again with automatic selection.

The tray process keeps the full list in memory and merges later successful
results by ID. New captures use the cache immediately and re-evaluate current
meeting selection. New captures, and changes to automatic connection, start a
lookup when the cache is older than five minutes or the last lookup failed;
Refresh always starts one. A pending lookup is shared rather than duplicated,
and failed refreshes retain cached entries with an explicit hint. No calendar
cache is persisted.

## Failure classes

Distinguish executable missing, process start failure, initialization failure,
capability missing, authentication/consent, timeout, malformed response, and
unexpected exit. Log sanitized correlation ID, phase, duration, and failure
class. Never log note bodies, access tokens, raw calendar results, attendee
lists, or meeting titles.

## Development fixtures

Tests use local JSON-RPC streams or fixture tools for handshake, discovery,
one/zero/multiple meetings, malformed output, timeout, and subprocess failure.
`crates/noteiq-core/tests/fixtures/workiq/retrieval_today.json` is a synthetic
response that reproduces the real escaping; `workiq_process_tests.rs` runs a
scripted fake `workiq mcp` server on Unix. Live meeting contents are not
required for the platform-independent suite.
