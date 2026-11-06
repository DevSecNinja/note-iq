# Feature Specification: Reopen meeting notes

**Feature Branch**: `devsecninja-noteiq-capture-app`

**Created**: 2026-11-06

**Status**: Accepted

**Input**: User description: "Can you implement that opening previous meeting note feature please."

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Resume a meeting note (Priority: P1)

Selecting a meeting in the dropdown opens the existing capture for that meeting
occurrence in the editor, instead of relabeling the current capture.

**Why this priority**: Users can continue their notes without creating duplicates.

**Independent Test**: Save a note, close it, then select its meeting in a new
capture and verify that its text, identity, and path are restored.

**Acceptance Scenarios**:

1. **Given** a saved meeting note, **When** its meeting is selected, **Then** its
   latest saved text opens and subsequent saving updates the same file.
2. **Given** a current draft, **When** another meeting's note is opened, **Then**
   the draft is saved under its original context before switching.
3. **Given** a save or lookup failure, **When** switching is attempted, **Then**
   the current draft remains intact and an actionable error is shown.
4. **Given** no existing note for a meeting, **When** it is selected, **Then**
   the current capture attaches to it as before.

### User Story 2 - Keep series occurrences distinct (Priority: P1)

Users can select an earlier occurrence without mixing it with another occurrence
of the same recurring meeting.

**Why this priority**: Repeated titles and series identities must not merge notes.

**Independent Test**: Save two occurrences sharing a title and calendar identity
but different start times; selecting either restores only that occurrence.

**Acceptance Scenarios**:

1. **Given** repeated meeting titles, **When** an occurrence is selected, **Then**
   matching uses calendar identity and occurrence start, never title alone.
2. **Given** multiple saved captures for one occurrence, **When** it is selected,
   **Then** the most recently updated capture opens deterministically.

### User Story 3 - Preserve vault edits (Priority: P1)

Reopening a capture preserves custom properties, Summary, Follow-ups and other
edits outside the Notes section.

**Why this priority**: Continuing notes must not destroy Obsidian refinements.

**Independent Test**: Edit the saved note outside NoteIQ, reopen, append text,
save, and verify all other content is unchanged.

**Acceptance Scenarios**:

1. **Given** a refined capture, **When** note text is changed, **Then** unrelated
   content and its original filename remain unchanged.
2. **Given** an external edit after reopening, **When** save or empty-note cleanup
   is attempted, **Then** the external content is not overwritten or deleted.

### User Story 4 - Confirm meeting changes on saved notes (Priority: P1)

When a saved note is already connected to a meeting, choosing a different meeting
asks whether to move the existing note to that meeting or start a separate,
empty note for it.

**Why this priority**: A meeting change should not silently file existing notes
under a different meeting.

**Independent Test**: Select a different meeting from a saved meeting note and
verify that changing the meeting preserves the current note, while creating a
new note saves the original and opens an empty capture for the selected meeting.

**Acceptance Scenarios**:

1. **Given** a saved note connected to one meeting, **When** another meeting is
   selected, **Then** a confirmation offers **Change meeting** and
   **Create new note** without replacing the note text.
2. **Given** the confirmation, **When** **Change meeting** is chosen, **Then**
   the same note and text remain open with the newly selected meeting.
3. **Given** the confirmation, **When** **Create new note** is chosen, **Then**
   the current note is safely saved and a blank note opens for the selected
   meeting.
4. **Given** the confirmation, **When** it is cancelled, **Then** the current
   note and its meeting remain unchanged.
5. **Given** saving the current note fails, **When** **Create new note** is
   chosen, **Then** the current note remains open and the confirmation remains
   available for retry or cancellation.

### Edge Cases

- Unavailable destination, unreadable candidate, malformed matching capture,
  unsupported layout, duplicated note identities, or ambiguous section boundaries.
- Missing occurrence start: only identity with equally absent start can match.
- Existing Notes headings inside fenced code must remain part of the note.
- Selecting the active saved occurrence must not reload over unsaved text.
- No meeting continues to detach the current note and updates persisted context.
- An empty Notes section in a refined document must not delete its refinements.
- Switching from a saved meeting note to a different meeting requires an
  explicit choice before changing the current note's context or creating a new
  note.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: Search only captures within the selected destination.
- **FR-002**: Match calendar identity and occurrence start, with exact instant
  comparison independent of timezone representation.
- **FR-003**: Restore capture identity, timestamps, path and editable note text.
- **FR-004**: Save/finish the current capture before replacing the editor state.
- **FR-005**: Preserve unrelated content and detect external changes.
- **FR-006**: Keep meeting selection and autosave consistent after reopening.
- **FR-007**: Show/log lookup and reopening failures without exposing note bodies.
- **FR-008**: Do not add routine informational captions or a second destination.
- **FR-009**: When a saved, meeting-linked note is switched to a different
  meeting, ask whether to change the meeting on that note or create a separate
  blank note; cancelling leaves the current note unchanged.

### Key Entities

- **Meeting occurrence**: Calendar identity and start instant.
- **Saved capture**: Capture identity, file path, metadata and editable Notes.
- **Original document**: Saved content retained for preservation/conflict checks.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: One dropdown selection restores the correct saved note.
- **SC-002**: All switching failure scenarios preserve 100% of the current draft.
- **SC-003**: All recurring-occurrence scenarios restore the correct file.
- **SC-004**: Saving a reopened note preserves all unrelated document content.
- **SC-005**: Changing the selected meeting from a saved note never silently
  carries its text into a new meeting; the user explicitly chooses whether to
  keep the note and change its meeting or start a blank note.

## Assumptions

- One capture per occurrence is the default; existing duplicates remain intact.
- Only existing NoteIQ capture layouts are eligible, not arbitrary vault pages.
- The dropdown's existing earlier-meeting choices provide history navigation;
  a separate series browser is outside this change.
- Automatic attachment does not reopen files over the user's current text.
