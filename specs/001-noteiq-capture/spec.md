# Feature Specification: NoteIQ Quick Capture

**Feature Branch**: `devsecninja-noteiq-capture-app`

**Created**: 2026-11-03

**Status**: Approved

**Input**: User description: "A fast, meeting-aware Markdown note capture app with
tray and shortcut launch, automatic OneDrive vault storage, optional meeting
context, autosave, no-meeting disconnect, and friendly recovery."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Capture and keep a quick note (Priority: P1)

The user invokes NoteIQ and starts typing immediately. The note records capture
time, timezone, source, and optional meeting context without requiring a title.
The user can save without closing, continue typing through autosave, and open the
saved Markdown file in the registered editor.

**Why this priority**: Fast, durable capture is the app's primary value and must
work even when no optional integration is available.

**Independent Test**: Launch a capture without Scout, enter text, wait for
autosave, save with Ctrl+S, and verify one readable Markdown file appears in the
selected destination with its text and generated time metadata.

**Acceptance Scenarios**:

1. **Given** no capture is open, **When** the user starts a new capture,
   **Then** an editor appears ready for typing without a title or metadata form.
2. **Given** a note is being edited, **When** the user pauses after changing
   text, **Then** NoteIQ saves the change without closing the editor.
3. **Given** the user continues typing, **When** the periodic save interval
   elapses, **Then** the current text is saved and editing continues.
4. **Given** the user presses Ctrl+S, **When** the note is saved,
   **Then** the window remains open and shows the saved status.
5. **Given** a note is saved, **When** the user chooses Open note,
   **Then** the exact saved Markdown file opens in the registered application.
6. **Given** an empty unsaved capture is open, **When** the user closes it,
   **Then** no empty note file is left behind.
7. **Given** the user erases all text in a previously saved note, **When**
   editing or autosave is in progress, **Then** the file is retained; only after
   the capture closes is the empty note deleted.

### User Story 2 - Add, switch, or disconnect meeting context (Priority: P1)

The user sees whether a calendar meeting is connected. Exactly one current
meeting can be connected automatically. When meetings overlap, the user chooses.
The user can change or disconnect context; a delayed lookup cannot undo an
explicit decision.

**Why this priority**: Reliable context is the main improvement over a generic
notepad, but it must never delay or prevent writing the note itself.

**Independent Test**: Use deterministic meeting fixtures to cover zero, one,
multiple, and failed lookups. Connect a meeting to a note, choose No meeting,
save, and assert the resulting file has no meeting-specific metadata while
retaining note text and identity.

**Acceptance Scenarios**:

1. **Given** automatic connection is enabled and exactly one current meeting
   exists, **When** meeting discovery finishes, **Then** that meeting is shown.
2. **Given** multiple current meetings exist, **When** discovery finishes,
   **Then** none is guessed and the user can choose among them.
3. **Given** no current meetings exist, **When** discovery succeeds,
   **Then** the capture shows No meeting.
4. **Given** discovery fails, **When** the editor is open, **Then** the editor
   remains usable and a distinct, actionable unavailable state is shown.
5. **Given** meeting metadata was saved, **When** the user chooses No meeting
   and saves, **Then** meeting identifiers, title, and attendee fields are
   removed from the saved Markdown without changing note text.
6. **Given** the user explicitly disconnects while a lookup is pending,
   **When** the lookup later succeeds, **Then** the capture remains disconnected.
7. **Given** a meeting is connected, **When** its scheduled end passes,
   **Then** NoteIQ does not silently switch or remove it.
8. **Given** automatic connection is disabled in Settings, **When** a new
   capture opens, **Then** discovery does not auto-connect a meeting.

### User Story 3 - Save to one understandable destination (Priority: P1)

The user has a default Obsidian vault destination and can instead select an
auto-detected OneDrive Notes folder. Each note is saved only to the selected
destination. If a folder is unavailable, NoteIQ asks the user to select another
destination and preserves the unsaved text.

**Why this priority**: Notes must land where the user and their existing tools
can find them, without ambiguity or accidental duplicates.

**Independent Test**: Configure temporary vault and OneDrive directories,
write notes under each selection, and verify exact paths and absence of files in
the unselected location. Simulate an unavailable destination and cancel the
chooser; verify the text remains open and unsaved.

**Acceptance Scenarios**:

1. **Given** no destination has been changed, **When** a note is saved,
   **Then** it is stored under the configured vault's
   `captures/YYYY/MM/YYYY-MM-DD_HHmm_<slug>.md` location.
2. **Given** the user selects OneDrive Notes, **When** a note is saved,
   **Then** it is stored under that folder and not in the vault.
3. **Given** a destination is missing or unwritable, **When** a save is
   attempted, **Then** the text stays in the editor and NoteIQ asks the user to
   choose a different destination.
4. **Given** the user cancels destination selection, **When** the error state
   returns, **Then** the capture remains open and is not reported as saved.
5. **Given** a new note is saved, **When** its filename is chosen, **Then** it
   uses a stable, safe slug and does not overwrite an existing note.

### User Story 4 - Open capture from the workflows already in use (Priority: P2)

The user opens capture using the tray, configurable global shortcut, `noteiq
new`, or `noteiq://new`. A repeated launch brings the active capture forward.
Ctrl+N intentionally starts another capture after safely finishing the current
one.

**Why this priority**: Multiple entry points remove launch friction while the
single capture window prevents confusion and accidental loss.

**Independent Test**: Verify each launch route opens one capture, repeated
invocations foreground it, and Ctrl+N safely starts a fresh capture.

**Acceptance Scenarios**:

1. **Given** NoteIQ is running in the tray, **When** the configured shortcut is
   pressed, **Then** the active capture opens or is brought forward.
2. **Given** no capture is open, **When** `noteiq new` or `noteiq://new` is
   invoked, **Then** a capture opens.
3. **Given** a capture is already open, **When** any normal launch route is
   invoked, **Then** that capture is brought forward without replacing text.
4. **Given** a capture is open, **When** the user presses Ctrl+N, **Then**
   NoteIQ applies the save/empty-note close rules before opening a fresh capture.

### User Story 5 - Recover from integration and application failures (Priority: P2)

The user can continue capturing notes if Scout, WorkIQ, Microsoft 365 access,
or meeting discovery fails. Clear diagnostics and documentation help the user
resolve missing dependencies and verify whether WorkIQ can retrieve Markdown.

**Why this priority**: Optional context and local environment issues must not
block the core capture workflow or hide why integration failed.

**Independent Test**: Run NoteIQ with a missing WorkIQ executable, malformed MCP
responses, and an unwritable destination. Verify the editor remains available,
errors are actionable, logs exclude private data, and Markdown retrieval limits
are described.

**Acceptance Scenarios**:

1. **Given** Scout or its WorkIQ executable is unavailable, **When** the user
   opens a capture, **Then** the note works without meeting context and the
   missing dependency is clearly identified.
2. **Given** WorkIQ fails, **When** the error is displayed, **Then** rolling
   diagnostics provide a correlation ID and sanitized failure details.
3. **Given** diagnostics are written, **When** logs are inspected, **Then**
   note bodies, credentials, and raw meeting details are absent.
4. **Given** WorkIQ cannot retrieve the saved Markdown in the user's
   environment, **When** the user reads setup guidance, **Then** the known
   limitation and verification steps are documented; no duplicate companion
   document is created.

## Edge Cases

- A failed lookup is distinct from a successful lookup returning no meetings.
- An explicit meeting choice or disconnect wins over a late asynchronous result.
- Meeting time comparisons respect timezone offsets and calendar event boundaries.
- Continuous editing does not create unbounded or overlapping save operations.
- A failed write or failed empty-file deletion never discards text or reports
  success; the error remains visible with an appropriate retry action.
- Slugs are safe for Windows paths, preserve supported Unicode, and collision
  handling never overwrites unrelated files.
- The selected root can disappear between settings and save.
- A destination that stays unavailable is not retried on every timer tick:
  automatic retries pause with a growing delay, an explicit save always retries
  at once, and a successful save resets the delay.
- WorkIQ output can be malformed, oversized, delayed, unauthenticated, or
  changed in a later version.
- A user who starts, clears, then closes a note never loses the file during the
  editing interval.
- A URI or CLI request with malformed parameters reports an actionable error
  without executing arbitrary commands or paths.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST open a focused plain-text Markdown editor without a
  required title field.
- **FR-002**: System MUST write timestamps, timezone, source, tags, and optional
  meeting metadata as Markdown front matter, without placing generated fields
  in the editable note body.
- **FR-003**: System MUST autosave after a typing pause and periodically during
  continuous typing, and support Ctrl+S without closing the window.
- **FR-004**: System MUST retain a cleared note during editing and delete an
  empty note only when its capture is closed or intentionally replaced.
- **FR-005**: System MUST show an Open note action after the note exists and open
  the exact saved file.
- **FR-006**: System MUST provide a configurable default capture destination and
  an alternative auto-detected OneDrive Notes destination.
- **FR-007**: System MUST write each note to exactly one destination and MUST
  NEVER silently fall back, dual-write, or claim an unsuccessful save succeeded.
- **FR-008**: System MUST preserve editor text and clearly prompt for a different
  destination when the selected one cannot be written.
- **FR-009**: System MUST generate safe, stable capture paths in the selected
  `captures/YYYY/MM/` folder and avoid overwriting unrelated files.
- **FR-010**: System MUST discover current meetings using Scout's local WorkIQ
  MCP executable when available and show unavailable/failed states separately
  from No meeting.
- **FR-011**: System MUST auto-connect only for exactly one eligible current
  meeting; for multiple meetings it MUST present a chooser.
- **FR-012**: System MUST allow the user to connect to, switch, and disconnect a
  meeting; an explicit choice MUST be preserved against late lookups.
- **FR-013**: System MUST remove persisted meeting and attendee metadata when
  the user chooses No meeting and saves.
- **FR-014**: System MUST allow the user to configure automatic meeting
  connection without disabling the plain-text editor.
- **FR-015**: System MUST launch capture through the tray, a configurable global
  shortcut, the `noteiq new` command, and the `noteiq://new` URI.
- **FR-016**: System MUST focus the existing capture on repeated invocation and
  expose Ctrl+N for an intentional new capture.
- **FR-017**: System MUST provide rolling diagnostic logs with failure
  correlation and exclude note bodies, credentials, and raw meeting data.
- **FR-018**: System MUST provide clear missing-dependency guidance when Scout
  or WorkIQ is unavailable; notes MUST remain usable without it.
- **FR-019**: System MUST document setup, launch, troubleshooting, and known
  WorkIQ Markdown retrieval limitations.
- **FR-020**: System MUST offer a keyboard-first modern UI with actionable,
  user-friendly save and integration states.
- **FR-021**: System MUST include comprehensive platform-independent tests,
  including a regression asserting that saved meeting fields are removed after
  selecting No meeting.

### Key Entities

- **Capture**: One user editing session; has editable text, origin, creation
  time, optional selected meeting, save state, and optional persisted path.
- **Meeting context**: A calendar event eligible at capture time, including
  stable identifier and available title, schedule, organizer, and attendees.
- **Destination**: One configured output root and destination type selected for a
  new capture.
- **Saved note**: A Markdown document identified by a stable note ID and path,
  containing metadata and user-authored body text.
- **Integration state**: WorkIQ discovery, availability, authentication, and
  error state that remains independent of editor usability.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In a warm launch, a capture is ready for typing within 2 seconds
  for at least 95% of local launches on a supported device.
- **SC-002**: A user can invoke capture and begin entering text with no required
  configuration or metadata interaction.
- **SC-003**: After a typing pause or explicit save, the exact note text is
  present in a readable Markdown file without closing the capture.
- **SC-004**: In 100% of overlap-fixture cases, the app leaves the meeting
  unconnected until the user chooses one.
- **SC-005**: In 100% of No meeting regression cases, saved meeting-specific
  fields are absent after saving while the user's note body remains unchanged.
- **SC-006**: In 100% of empty-note lifecycle cases, no saved file is deleted
  during editing; an empty note is removed when its capture closes.
- **SC-007**: In 100% of destination test cases, one note appears only in the
  selected destination.
- **SC-008**: In 100% of simulated missing-integration cases, the user can
  continue to edit and save a note when a writable destination exists.
- **SC-009**: In 100% of failed-save test cases, NoteIQ keeps unsaved text open
  and does not display a successful-save state.
- **SC-010**: A user can complete routine capture, save, disconnect, and close
  actions entirely from the keyboard.

## Assumptions

- The user has permission to write to the selected local or OneDrive-synced
  destination.
- Scout may be installed without a standalone WorkIQ executable; the app must
  discover actual local capabilities and explain missing pieces rather than
  assuming a particular installation path.
- WorkIQ meeting data is subject to existing Microsoft 365 access, tenant policy,
  authentication, connectivity, and service availability.
- OneDrive sync does not by itself prove that WorkIQ can search Markdown
  contents. The feature remains Markdown-only and documents retrieval limits.
- No local recovery copy or automatic destination fallback is created.
- Windows is the user-facing platform; Linux under WSL is used for
  platform-independent tests.
- Initial UI styling follows the operating system light/dark theme and keyboard
  conventions. Rich-text editing and rendered Markdown preview are out of scope.
- The configured vault path is a user default and MUST be overridden by local
  settings when an installation uses a different location.
