# Data Model: NoteIQ Quick Capture

## Capture

One active edit session, with:

- `id`: stable UUID.
- `created_at`, `updated_at`: timezone-aware timestamps.
- `timezone`: IANA zone when available; UTC offset otherwise.
- `source`: `shortcut`, `tray`, `cli`, `uri`, or `new_note`.
- `author`: optional note taker, from the `author_name` setting or the Windows
  display name; written as the `from` wikilink.
- `body`: user-authored plain Markdown.
- `tags`: user-entered tags, empty by default.
- `meeting`: optional selected `Meeting`.
- `meeting_decision`: `automatic`, `selected`, `disconnected`, or `pending`.
  `selected` and `disconnected` are explicit user choices that lookups,
  refreshes, and re-evaluation never change; New note starts again at
  `pending`.
- `path`: assigned on first successful non-empty save; updated if the meeting
  name changes and the filename template uses `{title}`, while retaining the
  original folder and timestamp.
- `first_saved_at`, `filename_meeting_slug`: in-memory filename context, used
  to distinguish meeting-driven renames from ordinary body edits.
- `filename_template`: the note filename setting, fixed at first save so a
  later settings change never renames this note.
- `save_state`: `unsaved`, `saving`, `saved`, or `error`.

The generated front matter is separate from `body`. A body with no
non-whitespace characters is empty for lifecycle purposes.

## Meeting

- `id`: stable event identifier when supplied by WorkIQ.
- `title`: display title; may be unavailable.
- `start`, `end`: timezone-aware event bounds when supplied.
- `organizer`: optional `Name <email>` contact; falls back to the available part.
- `invitees`: names and email addresses, invitation role (required, optional,
  resource, unknown), RSVP, and optional `has_attended` observation.
- `your_rsvp`: the signed-in user's response when supplied.
- `entity_url`: transient WorkIQ event path used for optional RSVP retrieval;
  never written to the note.
- `cancelled`, `declined`: excluded from the Meeting list and automatic
  connection; a meeting already attached stays attached.

Meeting details not returned by the integration remain absent. A disconnected
capture has no meeting metadata at all.

## Destination

- `kind`: `vault` or `onedrive`.
- `root`: user-selected absolute path.
- `relative_folder`: always `captures/YYYY/MM` for generated notes.
- File name: the note filename template rendered with the first-save date and
  time and a safe slug, plus `.md` and a collision suffix when needed.

Only one destination is active for a new capture. A persisted note retains its
path. A destination setting change does not move existing notes.

## Saved Markdown

Schema 3 follows the vault's capture layout. YAML front matter lists vault
properties first (`kind`, `source`, `from`, `date`, `captured`, `processing`,
`ai_refined`, `status`, `tags`), then meeting properties (`meeting`,
`meeting_start`, `meeting_end`, `organizer`, `your_rsvp`, flat quoted
`"Name <email>"` lists in `required_invitees` and `optional_invitees` (always
present, possibly empty) and non-empty `resource_invitees`/`other_invitees`,
and `calendar_event_id`), then NoteIQ
bookkeeping (`id`, `updated`, `timezone`, `captured_via`, `noteiq_schema: 4`).
Strings are quoted so YAML never coerces them. The body follows a blank line:
an escaped `# Title`, a From/Captured line, a refine hint, `## Meeting details`
with local times, duration, organizer, RSVP and invitee RSVP/attendance,
empty `## Summary` and `## Follow-ups` sections, and the user's text verbatim
under `## Notes` as ordinary editable Markdown, without a generated HTML wrapper
or code fence. Unknown attendance is not false, and RSVP is
not evidence of attendance. Existing files are not migrated. Removing meeting
context removes all meeting-specific keys and the Meeting details section.

## Settings

- `destination_kind` and `destination_root`.
- `auto_connect_meeting`.
- global hotkey key and enabled state.
- optional WorkIQ executable override.
- optional `author_name`, which takes precedence over the Windows display name.
- `note_filename`: template using `{date}`, `{time}`, and `{title}`; default
  `{date}_{time}_{title}`. An invalid value in the file falls back to the
  default.

Settings are local to the current user and never contain note text, meeting
payloads, credentials, or access tokens.

## Lifecycle invariants

1. A capture body update changes the in-memory body before any save begins.
2. Saves are serialized. A stale asynchronous completion cannot mark newer
   content saved.
3. A No meeting decision clears the capture's meeting before serialization.
4. An empty file is never deleted while the capture remains in edit state.
5. On close, the latest save completes before empty-note deletion.
6. A failed save keeps the body and path visible as unsaved/error.
7. Meeting changes are applied to the capture at once and saved after the
   autosave pause; an author change is saved at once only when the note
   already has a file. Neither saves an empty draft that was never saved.
8. A saved file is rewritten, renamed, or deleted only while it still carries
   the capture's `id`.
