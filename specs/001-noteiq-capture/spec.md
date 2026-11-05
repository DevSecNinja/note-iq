# Feature Specification: NoteIQ Quick Capture

**Feature Branch**: `devsecninja-noteiq-capture-app`

**Created**: 2026-11-03

**Status**: Approved

**Input**: User description: "A fast, meeting-aware Markdown note capture app with
tray and shortcut launch, automatic OneDrive vault storage, optional meeting
context, autosave, no-meeting disconnect, and friendly recovery."

## User Scenarios & Testing _(mandatory)_

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

The user sees whether a calendar meeting is attached in a single **Meeting**
list whose first option is always **No meeting**. Exactly one current timed
meeting can be attached automatically, including one that starts while an empty
note is open. When meetings overlap, the user chooses. The user can change or
remove the meeting; a delayed lookup, refresh, or re-evaluation cannot undo an
explicit choice.

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
   **Then** none is guessed, the list stays on No meeting, and the hint shows
   how many meetings are in progress so the user can choose one.
3. **Given** no meeting is in progress but one starts or ended within
   15 minutes, **When** discovery finishes, **Then** none is connected and the
   user can choose it from the list, labelled with its local start and end times.
4. **Given** no timed meetings are in progress or within the 15-minute window,
   **When** discovery succeeds automatically,
   **Then** no meeting is connected, but older retrieved meetings remain selectable.
5. **Given** discovery fails, **When** the editor is open, **Then** the editor
   remains usable and a distinct, actionable unavailable state is shown.
6. **Given** meeting metadata was saved, **When** the user chooses No meeting
   and saves, **Then** meeting identifiers, title, and invitee fields are
   removed from the saved Markdown without changing note text.
7. **Given** the user chooses No meeting while a lookup is pending,
   **When** the lookup later succeeds, **Then** the capture remains without a
   meeting.
8. **Given** a meeting is connected, **When** its scheduled end passes,
   **Then** NoteIQ never silently removes it, and moves an automatic meeting
   only while the note is empty and has never been saved.
9. **Given** automatic connection is disabled in Settings, **When** a new
   capture opens, **Then** discovery does not auto-connect a meeting.
10. **Given** a meeting is connected, **When** the user opens the Meeting
    list, **Then** older retrieved meetings remain available with dates and
    local times and can be explicitly chosen.
11. **Given** a fresh cached list or a pending lookup, **When** New note is
    pressed, **Then** the current note is safely finished and the new capture
    reuses the list or pending lookup instead of starting duplicate requests.
12. **Given** invitee information is available, **When** a note is saved,
    **Then** required and optional invitees retain names and email addresses,
    available RSVP and explicit attendance observations; missing values remain
    unknown and RSVP is never treated as evidence of attendance.
13. **Given** the Meeting list is shown, **When** the user opens it, **Then**
    its first option is No meeting, followed by retrieved meetings labelled with
    title, date, and local times and without ordinal numbers; declined and
    cancelled meetings are not offered.
14. **Given** an empty, never-saved note with no meeting choice is open,
    **When** a single cached timed meeting starts, **Then** the note attaches
    to it within a minute without a new WorkIQ request.
15. **Given** a note with text opened more than 15 minutes before a meeting
    starts, **When** that meeting is in progress, **Then** it is not attached
    automatically and the hint offers it for selection.
16. **Given** the user chose a meeting or No meeting, **When** a refresh or
    re-evaluation finds a different single meeting in progress, **Then** the
    choice is kept for that note; New note starts again with automatic
    selection.
17. **Given** the last lookup failed, **When** cached meetings are re-evaluated,
    **Then** nothing is attached automatically and the hint says earlier
    results are shown.

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
   **Then** it is stored under the configured vault's `captures/YYYY/MM/`
   folder, named `YYYY-MM-DD_HHmm_<slug>.md` by the default
   `{date}_{time}_{title}` note filename setting.
2. **Given** the user selects OneDrive Notes, **When** a note is saved,
   **Then** it is stored under that folder and not in the vault.
3. **Given** a destination is missing or unwritable, **When** a save is
   attempted, **Then** the text stays in the editor and NoteIQ asks the user to
   choose a different destination.
4. **Given** the user cancels destination selection, **When** the error state
   returns, **Then** the capture remains open and is not reported as saved.
5. **Given** a new note is saved, **When** its filename is chosen, **Then** it
   uses a safe meeting-name slug (or note-text fallback) and does not overwrite
   an existing note. Meeting changes update only the active filename.
6. **Given** the user changes the note filename setting, **When** a later note
   is saved, **Then** it uses the new template while notes already saved keep
   their names; an invalid template is explained inline and the previous
   setting is kept.

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

### User Story 6 - Obtain a verified native Windows release (Priority: P2)

The user can obtain a release package for Windows x64 or ARM64. Each package
contains the matching native executable and its supporting README and LICENSE,
with a SHA-256 manifest and GitHub provenance attestation. Releases are created
through the Release Please draft workflow and published only after both
architectures are built from the same tagged commit.

**Why this priority**: Both supported Windows architectures need reproducible
native packages, and users need a way to check package integrity and provenance.

**Independent Test**: Exercise both native build jobs from one tag, verify each
executable's PE architecture, inspect both ZIP contents, compare package hashes
with `SHA256SUMS.txt`, verify attestations with GitHub CLI, and confirm the
existing Release Please draft is published without creating a second release.

**Acceptance Scenarios**:

1. **Given** a pull request changes the application, **When** CI runs,
   **Then** portable checks execute on Linux x64 and ARM64 and native desktop
   checks execute on Windows x64 and ARM64.
2. **Given** a `v*` tag or an explicitly retried tag, **When** the release
   workflow runs, **Then** both native architectures are built from that exact
   tagged commit and their executable architectures are verified.
3. **Given** both builds succeed, **When** release archives are prepared,
   **Then** each ZIP contains `noteiq.exe`, `README.md`, and `LICENSE`, and the
   release includes `SHA256SUMS.txt` and GitHub attestations.
4. **Given** Release Please has made a draft release, **When** the local release
   workflow finishes successfully, **Then** it uploads both packages and
   publishes that draft; it MUST NOT create an independent release.
5. **Given** a user checks a downloaded package, **When** they compare its
   digest and verify its attestation, **Then** integrity and provenance
   verification steps are documented, along with the limitation that packages
   are not Authenticode-signed and SmartScreen warnings may remain.

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
- A refresh that no longer returns the attached meeting keeps it listed and
  attached.
- A meeting that crosses a daylight-saving change shows each endpoint in the
  offset in force at that instant.
- A saved file replaced or edited in the vault so that it no longer carries the
  note's ID is never overwritten, renamed, or deleted.
- Calendar titles containing Markdown or HTML display literally and cannot hide
  or restructure the generated note.

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: System MUST open a focused plain-text Markdown editor without a
  required title field.
- **FR-002**: System MUST write timestamps, timezone, source, tags, and optional
  meeting metadata as Markdown front matter and generate the surrounding note
  layout, without placing generated fields in the editor.
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
- **FR-009**: System MUST generate safe capture paths in the selected
  `captures/YYYY/MM/` folder and avoid overwriting unrelated files.
- **FR-010**: System MUST discover current meetings using Scout's local WorkIQ
  MCP executable when available and show unavailable/failed states separately
  from No meeting.
- **FR-011**: System MUST auto-connect only for exactly one timed meeting in
  progress that is not all-day, declined, or cancelled, and never while the last
  lookup failed. While a capture is open it MUST re-evaluate the cached list
  every 60 seconds without a WorkIQ request. A note without a meeting choice
  attaches when it is empty or the meeting started no later than 15 minutes
  after the note was created; an automatic meeting moves only while the note is
  empty and has never been saved. Otherwise the note stays on No meeting and
  the hint reports how many meetings are in progress.
- **FR-012**: System MUST allow the user to connect to, switch, and disconnect a
  meeting through one Meeting list whose first option is No meeting and whose
  meeting options are unnumbered and show title, date, and local times. A
  committed choice, including No meeting, MUST be applied at once, saved after
  the autosave pause, and preserved against late lookups, refreshes, and
  re-evaluation for that note.
- **FR-013**: System MUST remove persisted meeting and invitee metadata when
  the user chooses No meeting and saves.
- **FR-014**: System MUST allow the user to configure automatic meeting
  connection without disabling the plain-text editor.
- **FR-015**: System MUST launch capture through the tray, a configurable global
  shortcut, the `noteiq new` command, and the `noteiq://new` URI.
- **FR-016**: System MUST focus the existing capture on repeated invocation and
  expose Ctrl+N and a New note button for an intentional new capture, preserving
  the existing draft if saving fails.
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
- **FR-022**: System MUST save required/optional invitees separately, preserving
  `Name <email>` values and available RSVP/attendance without inventing missing data.
- **FR-023**: System MUST place a blank line between closing YAML front matter
  and the unchanged Markdown body.
- **FR-024**: System MUST embed a consistent, scaling-aware icon for the native
  window, taskbar, executable, tray, and registered URI.
- **FR-025**: System MUST retain the full retrieved meeting list in memory across
  captures, allow selecting earlier meetings while connected, share in-flight
  lookups, refresh on request or after five minutes on a new capture, and retain
  cached entries when refresh fails.
- **FR-026**: When the note's filename template uses `{title}`, System MUST use
  the connected meeting name in the active note's filename, falling back to note
  text when absent. Late discovery, switching, or
  disconnecting MUST safely update that filename without changing its original
  timestamp, folder, or note ID, overwriting another note, or leaving a duplicate.
  Body edits alone MUST NOT rename an already-saved note. Closed notes and vault
  links MUST NOT be bulk-modified.
- **FR-027**: System MUST write notes in the vault's capture layout (schema 3):
  vault properties first, then meeting properties, then NoteIQ bookkeeping; a
  body with an escaped title, a From/Captured line, Meeting details, empty
  Summary and Follow-ups sections, and the user's text verbatim in a collapsed
  Raw capture block. Meeting times MUST use the capture's named time zone at
  each instant. Existing notes MUST NOT be migrated.
- **FR-028**: System MUST record the note taker as a `from` wikilink from the
  Windows display name, looked up without delaying the editor, or from an
  `author_name` setting. A lookup that finishes after the open note was saved
  MUST update that note through the ordinary save path; closed notes are not
  rewritten.
- **FR-029**: System MUST let the user set the note filename template with the
  `{date}`, `{time}`, and `{title}` placeholders (default
  `{date}_{time}_{title}`), show an example while editing, and reject empty,
  path, unknown-placeholder, unbalanced, invalid-character, reserved-name, and
  over-long templates with a specific inline explanation while keeping the
  previous setting. A template MUST be fixed when a note is first saved, so a
  change applies only to notes not yet saved.
- **FR-030**: System MUST show meeting state as a hint under the Meeting list,
  separate from save status, distinguishing checking, connected automatically,
  selected, No meeting chosen, overlapping meetings, nothing found, automatic
  connection off, WorkIQ unavailable, and failed refresh with earlier results.
- **FR-031**: System MUST close the capture through the window's native close
  button or Alt+F4, without a duplicate Close button, applying the same save
  and empty-note rules.
- **FR-032**: Before rewriting, renaming, or deleting a saved file, System MUST
  confirm the file still carries the note's ID and otherwise report that the
  note changed outside NoteIQ instead of changing it.
- **FR-033**: System MUST support native Windows x64 and ARM64 builds. WorkIQ
  discovery MUST prefer Scout's native architecture payload, fall back to the
  other bundled architecture when the native payload is absent, and honor a
  configured executable path before either discovered path.
- **FR-034**: CI MUST run portable formatting, Clippy, and tests on Linux x64
  and ARM64, and native desktop Clippy/tests on Windows x64 and ARM64. Each
  Windows build MUST verify that its executable matches the requested target.
- **FR-035**: Shared lint, configuration-sync, Release Please, labeler,
  label-sync, and assign-issue-to-codeowners automation MUST reuse pinned
  reusable workflows from `DevSecNinja/.github`. Repository-specific Rust CI,
  Windows builds, and release artifact publishing MUST remain local where no
  suitable shared Rust workflow or draft-compatible publishing workflow exists.
- **FR-036**: Release Please MUST manage the virtual Cargo workspace starting
  at `0.1.0`, update root `Cargo.toml` and workspace package versions in
  `Cargo.lock`, and create a draft release with tag creation enabled. On a
  `v*` tag or manual tag retry, release automation MUST build x64 and ARM64
  from the same tagged commit, package each executable with `README.md` and
  `LICENSE`, generate `SHA256SUMS.txt` and GitHub attestations, and publish
  only the existing Release Please draft.
- **FR-037**: Documentation MUST explain the Release Please GitHub App and
  repository settings required for automation, provide SHA-256 and GitHub
  attestation verification steps, and state that neither mechanism is
  Authenticode signing or removes SmartScreen warnings.

### Key Entities

- **Capture**: One user editing session; has editable text, origin, creation
  time, optional author, optional selected meeting and how it was chosen, save
  state, optional persisted path, and the filename template fixed at first save.
- **Meeting context**: A calendar event eligible at capture time, including
  stable identifier and available title, schedule, organizer, invitees, the
  user's RSVP, and cancelled/declined state.
- **Destination**: One configured output root and destination type selected for a
  new capture.
- **Note filename template**: A user setting combining `{date}`, `{time}`, and
  `{title}` that names new notes inside `captures/YYYY/MM/`.
- **Saved note**: A Markdown document identified by a stable note ID and its current path,
  containing schema 3 metadata, a generated layout, and the user's text verbatim.
- **Integration state**: WorkIQ discovery, availability, authentication, and
  error state that remains independent of editor usability.

## Success Criteria _(mandatory)_

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
- **SC-010**: A user can complete routine capture, save, meeting choice
  (including No meeting), and close actions entirely from the keyboard.
- **SC-011**: In 100% of meeting re-evaluation cases, an explicit choice of a
  meeting or No meeting is unchanged by later lookups, refreshes, and
  re-evaluation, and nothing is attached automatically after a failed lookup.
- **SC-012**: In 100% of note filename validation cases, an invalid template is
  rejected with a specific explanation and the saved setting is unchanged; valid
  templates never overwrite an existing note.
- **SC-013**: In 100% of release validation cases, x64 and ARM64 packages come
  from the same tag, have verified executable architectures, match their
  published SHA-256 entries, have verifiable attestations, and are attached to
  the pre-existing Release Please draft only.

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
- Windows x64 and ARM64 are the user-facing platforms; Linux x64 and ARM64 are
  used for platform-independent CI tests.
- Initial UI styling follows the operating system light/dark theme and keyboard
  conventions. Rich-text editing and rendered Markdown preview are out of scope.
- The configured vault path is a user default and MUST be overridden by local
  settings when an installation uses a different location.
- Overlapping meetings are never tie-broken by RSVP or organizer; the user
  chooses. RSVP is not evidence of attendance.
- Re-evaluation every 60 seconds uses cached meetings only. WorkIQ is contacted
  at startup and on Refresh, and also for a new capture or a change to
  automatic connection when the cache is older than five minutes or the last
  lookup failed.
- Turning automatic connection off does not detach a meeting that is already
  attached, and a meeting cancelled after it was attached stays attached until
  the user changes it.
- An author lookup that finishes after a note was closed does not rewrite that
  note, so later edits in the vault are never overwritten in the background.
