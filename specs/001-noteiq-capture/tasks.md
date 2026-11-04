# Tasks: NoteIQ Quick Capture

**Input**: Design documents from `specs/001-noteiq-capture/`

**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`,
`contracts/mcp.md`, and `quickstart.md`.

**Tests**: Required by the feature specification and constitution.

**Organization**: Tasks are grouped by user story and acceptance-test criteria.

**Format**: Every task uses `- [ ] [TaskID] [P?] [Story?] Description with file
path`. `[P]` indicates distinct files and no unfinished prerequisite.

## Phase 1: Setup

**Purpose**: Establish Spec Kit, workspace layout, and runnable package skeleton.

- [x] T001 Initialize GitHub Spec Kit 1.0.4 with Copilot integration in `.specify/` and `.github/`
- [x] T002 Ratify project guardrails in `.specify/memory/constitution.md` and specify the feature in `specs/001-noteiq-capture/spec.md`
- [x] T003 Create planning artifacts in `specs/001-noteiq-capture/plan.md`, `research.md`, `data-model.md`, `contracts/mcp.md`, and `quickstart.md`
- [x] T004 Create the Rust workspace and platform-independent crate in `Cargo.toml`, `crates/noteiq-core/Cargo.toml`, and `crates/noteiq-core/src/lib.rs`
- [x] T005 [P] Create the Windows application crate and feature-gated Slint target in `apps/noteiq-windows/Cargo.toml`, `apps/noteiq-windows/build.rs`, and `apps/noteiq-windows/src/main.rs`
- [x] T006 [P] Add formatting/build ignores and Rust toolchain policy to `.gitignore` and `rust-toolchain.toml`
- [x] T007 [P] Document prerequisites and initial launch/test commands in `README.md`

## Phase 2: Foundational

**Purpose**: Define portable types, errors, safe serialization, and fixture seams.

- [x] T008 Create typed capture, source, save, connection, meeting, and settings models in `crates/noteiq-core/src/model.rs`
- [x] T009 [P] Define actionable core errors and privacy-safe diagnostic fields in `crates/noteiq-core/src/error.rs` and `crates/noteiq-core/src/diagnostics.rs`
- [x] T010 [P] Add deterministic clock and fixture directory helpers in `crates/noteiq-core/tests/common/mod.rs`
- [x] T011 Add front-matter test fixtures for meeting, No meeting, Unicode, quotes, and empty body in `crates/noteiq-core/tests/fixtures/`

**Checkpoint**: Portable boundaries and fixtures are ready before user-story work.

## Phase 3: User Story 1 - Capture and keep a quick note (Priority: P1)

**Goal**: Fast text entry, Markdown metadata, autosave, Open note, and empty-note
cleanup only on close.

**Independent Test**: Run `cargo test -p noteiq-core`; verify the Markdown
round-trip and empty-note lifecycle with temporary files and a fake clock.

### Tests for User Story 1

- [x] T012 [P] [US1] Test metadata/body round-trip and YAML escaping in `crates/noteiq-core/tests/markdown_tests.rs`
- [x] T013 [P] [US1] Test stable IDs, timestamps, tags, and meeting front matter in `crates/noteiq-core/tests/metadata_tests.rs`
- [x] T014 [P] [US1] Test pause and continuous-typing save deadlines in `crates/noteiq-core/tests/autosave_tests.rs`
- [x] T015 [P] [US1] Test that cleared text remains on disk while editing and deletes only on close in `crates/noteiq-core/tests/empty_note_lifecycle_tests.rs`
- [x] T016 [P] [US1] Test repeated/failed writes preserve the latest body and stable path in `crates/noteiq-core/tests/save_lifecycle_tests.rs`

### Implementation for User Story 1

- [x] T017 [P] [US1] Serialize versioned Markdown front matter and body in `crates/noteiq-core/src/markdown.rs`
- [x] T018 [US1] Implement capture edit/save/finish transitions in `crates/noteiq-core/src/capture.rs`
- [x] T019 [US1] Implement deterministic debounce and maximum save interval in `crates/noteiq-core/src/autosave.rs`
- [x] T020 [P] [US1] Implement focused editor, context row, save state, and Open note action in `apps/noteiq-windows/ui/main.slint`
- [x] T021 [US1] Bind editor text, Ctrl+S, timers, save state, Open note, and close transition in `apps/noteiq-windows/src/ui.rs`

**Checkpoint**: User Story 1 is testable without Scout or Microsoft 365.

## Phase 4: User Story 2 - Add, switch, or disconnect meeting context (Priority: P1)

**Goal**: Optional current meeting discovery with overlap selection and
user-controlled No meeting updates.

**Independent Test**: Fixture-test meeting selection and run the No meeting
regression: save connected metadata, disconnect, save, and assert metadata was
removed while text and ID remain unchanged.

### Tests for User Story 2

- [x] T022 [P] [US2] Test zero/one/multiple/current-boundary and declined-event selection in `crates/noteiq-core/tests/meeting_selection_tests.rs`
- [x] T023 [P] [US2] Test No meeting removes persisted meeting and attendee fields without changing body in `crates/noteiq-core/tests/no_meeting_regression_tests.rs`
- [x] T024 [P] [US2] Test explicit connect/disconnect wins over delayed lookups in `crates/noteiq-core/tests/meeting_decision_tests.rs`
- [x] T025 [P] [US2] Test MCP initialization, dynamic tools/list, unsupported schema, timeout, malformed result, and process error with fixtures in `crates/noteiq-core/tests/workiq_protocol_tests.rs`

### Implementation for User Story 2

- [x] T026 [P] [US2] Implement meeting candidate validation and auto-select only when exactly one match exists in `crates/noteiq-core/src/meeting.rs`
- [x] T027 [US2] Implement bounded MCP handshake, runtime tool discovery, and process/error classes in `crates/noteiq-core/src/workiq.rs`
- [x] T028 [US2] Discover Scout's architecture-matched WorkIQ executable in `apps/noteiq-windows/src/workiq_windows.rs`
- [x] T029 [US2] Add meeting chip, selection list, No meeting action, and settings state in `apps/noteiq-windows/ui/main.slint`
- [x] T030 [US2] Start lookup asynchronously and preserve explicit decisions in `apps/noteiq-windows/src/meeting_ui.rs`

**Checkpoint**: Notes remain usable if meeting discovery is absent or fails.

## Phase 5: User Story 3 - Save to one understandable destination (Priority: P1)

**Goal**: One selected destination, safe capture paths, first-class defaults,
and visible write failures.

**Independent Test**: Use two isolated temporary roots and verify each selected
root receives the note while the other stays empty; verify failed/cancelled
selection retains unsaved text.

### Tests for User Story 3

- [x] T031 [P] [US3] Test vault `captures/YYYY/MM`, slugging, timestamp, and collision suffixes in `crates/noteiq-core/tests/destination_path_tests.rs`
- [x] T032 [P] [US3] Test atomic updates, exact one-destination writes, write errors, and external conflicts in `crates/noteiq-core/tests/destination_io_tests.rs`
- [x] T033 [P] [US3] Test settings defaults, OneDrive candidate detection, persistence, and invalid paths in `crates/noteiq-core/tests/settings_tests.rs`

### Implementation for User Story 3

- [x] T034 [P] [US3] Implement vault/OneDrive configuration and settings migration in `crates/noteiq-core/src/settings.rs`
- [x] T035 [US3] Implement safe unique path selection, atomic replacement, and close-only deletion in `crates/noteiq-core/src/destination.rs`
- [x] T036 [US3] Add folder chooser, destination status, and save-error selection flow in `apps/noteiq-windows/src/settings_ui.rs` and `apps/noteiq-windows/ui/main.slint`
- [x] T037 [US3] Open the exact saved note with the Windows registered application in `apps/noteiq-windows/src/open_note.rs`

**Checkpoint**: Each note is written to one selected root; save failures preserve text.

## Phase 6: User Story 4 - Open capture from existing workflows (Priority: P2)

**Goal**: Single capture window, tray, configurable global shortcut, CLI, URI,
and forwarding of repeated invocations.

**Independent Test**: Exercise process activation and normalized `new`/`show`
messages without the UI, then verify shortcuts and tray on Windows.

### Tests for User Story 4

- [x] T038 [P] [US4] Test accepted/rejected CLI and URI commands in `crates/noteiq-core/tests/launch_request_tests.rs`
- [x] T039 [P] [US4] Test hotkey defaults and key parsing in `crates/noteiq-core/tests/hotkey_settings_tests.rs`

### Implementation for User Story 4

- [x] T040 [US4] Parse `noteiq new`, forwarding `show`/`new`, and malformed arguments in `apps/noteiq-windows/src/launch.rs`
- [x] T041 [US4] Implement per-user single-instance IPC and request forwarding in `apps/noteiq-windows/src/instance.rs`
- [x] T042 [US4] Implement Windows tray menu and focus/new/quit actions in `apps/noteiq-windows/src/tray.rs`
- [x] T043 [US4] Implement configurable default Win+Shift+N with conflict reporting in `apps/noteiq-windows/src/hotkey.rs`
- [x] T044 [US4] Register and validate `noteiq://new` in current-user Windows URL protocol settings in `apps/noteiq-windows/src/uri.rs`
- [x] T045 [US4] Connect launch, singleton, tray, shortcut, and UI lifecycle in `apps/noteiq-windows/src/main.rs`

**Checkpoint**: Each supported launch source focuses or intentionally creates
the single capture without discarding editor text.

## Phase 7: User Story 5 - Recover from integration and application failures (Priority: P2)

**Goal**: Useful missing-dependency messaging, sanitized rolling logs, and
installation/WorkIQ Markdown guidance.

**Independent Test**: Fixture-test failure classes and inspect logs for
correlation IDs while asserting note bodies and meeting values are absent.

### Tests for User Story 5

- [x] T046 [P] [US5] Test missing executable, timeout, authentication, process exit, and log redaction in `crates/noteiq-core/tests/diagnostics_tests.rs`
- [x] T047 [P] [US5] Test absent integration never blocks draft editing or serialization in `crates/noteiq-core/tests/offline_capture_tests.rs`
- [x] T048 [P] [US5] Add Windows UI acceptance checks for keyboard focus, contrast, saving, disconnected, and failed states in `apps/noteiq-windows/tests/ui_acceptance.rs`

### Implementation for User Story 5

- [x] T049 [US5] Implement rolling diagnostic writer and startup pruning in `crates/noteiq-core/src/diagnostics.rs`
- [x] T050 [US5] Map WorkIQ errors to non-sensitive actionable UI states in `apps/noteiq-windows/src/error_ui.rs` and `apps/noteiq-windows/ui/main.slint`
- [x] T051 [US5] Document installation, launch/Command Palette, troubleshooting, and Markdown retrieval limits in `docs/setup.md` and `README.md`

## Phase 8: Polish and Cross-Cutting Concerns

**Purpose**: Review design, verify release surfaces, and close requirements.

- [x] T052 [P] Have a UI design-focused agent review `apps/noteiq-windows/ui/main.slint` and report accessibility/interaction defects
- [x] T053 Apply any actionable review changes in `apps/noteiq-windows/ui/main.slint` and `apps/noteiq-windows/src/ui.rs`
- [x] T054 Run `cargo fmt --all -- --check`, workspace tests, and the Windows ARM64 MSVC target check
- [x] T055 Run `cargo test -p noteiq-core` in WSL and `cargo build --release --target aarch64-pc-windows-msvc` on Windows
- [x] T056 Verify the Spec Kit artifacts and requirement checklist in `specs/001-noteiq-capture/`
- [x] T057 [P] Test that a failed autosave backs off, keeps the draft, never throttles explicit saves, and recovers once the destination returns in `crates/noteiq-core/tests/autosave_tests.rs` and `crates/noteiq-core/tests/save_lifecycle_tests.rs`
- [x] T058 Pause automatic retries with a growing delay after a failed save in `crates/noteiq-core/src/autosave.rs` and `crates/noteiq-core/src/capture.rs`

## Dependencies and Execution Order

### Phase dependencies

- Phase 1 establishes the workspace; Phase 2 establishes types and fixtures.
- Phase 3 uses the portable model for the first independently valuable capture.
- Phase 4 consumes note metadata and save lifecycle from Phase 3.
- Phase 5 uses the capture model but can test path/settings logic independently.
- Phase 6 depends on the Windows application shell; it must not discard the
  active capture when forwarding launch requests.
- Phase 7 is cross-cutting and depends on both core errors and UI states.
- Phase 8 depends on all implemented story phases.

### User-story dependencies

- **US1 (P1)**: after setup/foundation; independent MVP.
- **US2 (P1)**: core metadata from foundation; persistence regression uses US1
  serialization.
- **US3 (P1)**: core settings and metadata; independent temporary-root tests.
- **US4 (P2)**: depends on the Windows application crate and UI lifecycle.
- **US5 (P2)**: depends on core and application error states.

### Parallel opportunities

- After workspace setup, model/errors/fixture helpers (T008-T011) are separable.
- Within US1, independent test modules (T012-T016) can be authored in parallel;
  serialization and autosave source modules are separate after the model.
- Within US2, selection, serialization regression, and MCP fixtures use separate
  files; production UI depends on actual runtime schema discovery.
- Within US3, destination and settings tests are independent.
- Within US4, launch parsing, tray, hotkey, and URI units use distinct files.
- Within US5, fixture diagnostics and offline capture tests use distinct files.
- The UI review (T052) was performed against the implemented UI, and actionable
  fixes were applied in T053 before final validation.

## Implementation Strategy

### MVP

Complete setup/foundation, then US1: non-blocking plain editor, portable
Markdown save lifecycle, idle/interval autosave, Ctrl+S, Open note, and
close-only empty-note deletion. Validate in WSL before building Windows UI.

### Incremental delivery

1. Add fixture-driven meeting context and the No meeting saved-file regression.
2. Add exactly-one-destination settings and failure-preserving folder chooser.
3. Add Windows tray, hotkey, CLI/URI forwarding, and single-instance handling.
4. Add diagnostics, setup documentation, UI design review, and native build.

## Notes

- `[P]` tasks touch distinct files with no dependency on unfinished work.
- Every user-story task uses a story label and explicit path.
- Tests MUST precede matching implementation and fail for the intended missing
  behavior before implementation starts.
- No live calendar data, note body, or credentials may be used in fixtures/logs.
- Native Windows ARM64 release build, core and UI tests, and Clippy with
  `-D warnings` pass. The earlier toolchain installation blocker is resolved.
  Portable workspace tests and formatting also pass from WSL.
- A synthetic-data Windows smoke test verifies registered URI activation from
  a foreground launcher, preservation of an open capture, fresh capture after
  close, URI source metadata, autosave, and deletion of erased notes only on close.
- A live-meeting acceptance check on the real calendar (UI Automation only, a
  temporary destination, no meeting values printed or kept) verifies automatic
  connect, the saved meeting block, No meeting removing it from the saved file,
  and Find meeting then Connect restoring it. A missing-destination run verifies
  that autosave pauses retries (5 s, then 10 s) and recovers once the folder returns.
