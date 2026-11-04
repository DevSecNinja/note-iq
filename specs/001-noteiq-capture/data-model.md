# Data Model: NoteIQ Quick Capture

## Capture

One active edit session, with:

- `id`: stable UUID.
- `created_at`, `updated_at`: timezone-aware timestamps.
- `timezone`: IANA zone when available; UTC offset otherwise.
- `source`: `shortcut`, `tray`, `cli`, `uri`, or `new-note`.
- `body`: user-authored plain Markdown.
- `tags`: user-entered tags, empty by default.
- `meeting`: optional selected `Meeting`.
- `meeting_decision`: `automatic`, `selected`, `disconnected`, or `pending`.
- `path`: optional stable path assigned on first successful non-empty save.
- `save_state`: `unsaved`, `saving`, `saved`, or `error`.

The generated front matter is separate from `body`. A body with no
non-whitespace characters is empty for lifecycle purposes.

## Meeting

- `id`: stable event identifier when supplied by WorkIQ.
- `title`: display title; may be unavailable.
- `start`, `end`: timezone-aware event bounds when supplied.
- `organizer`: optional display name.
- `attendees`: optional names or addresses.

Meeting details not returned by the integration remain absent. A disconnected
capture has no meeting metadata at all.

## Destination

- `kind`: `vault` or `onedrive`.
- `root`: user-selected absolute path.
- `relative_folder`: always `captures/YYYY/MM` for generated notes.

Only one destination is active for a new capture. A persisted note retains its
path. A destination setting change does not move existing notes.

## Saved Markdown

Versioned YAML front matter contains schema version, stable note ID, creation
and update timestamps, timezone, source, tags, and optional meeting fields.
The user-authored body follows the closing front-matter delimiter. Removing
meeting context removes all meeting-specific keys.

## Settings

- `destination_kind` and `destination_root`.
- `auto_connect_meeting`.
- global hotkey key and enabled state.
- optional WorkIQ executable override.

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
