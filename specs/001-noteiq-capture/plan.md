# Implementation Plan: NoteIQ Quick Capture

**Branch**: `devsecninja-noteiq-capture-app` | **Date**: 2026-11-03 |
**Spec**: [spec.md](spec.md)

**Input**: Approved feature specification in
`specs/001-noteiq-capture/spec.md`.

## Summary

Build a Windows x64 and ARM64 Rust/Slint quick-capture app with a small,
independently testable Rust core. The core owns note metadata, safe destination paths,
Markdown serialization, empty-note lifecycle, settings, and a fixture-tested
WorkIQ MCP adapter. The Windows surface owns the Slint editor, tray, hotkey,
CLI/URI dispatch, file dialogs, and native opener. Local WorkIQ is discovered in
the Scout installation and is never installed or copied.

## Technical Context

**Language/Version**: Rust stable (WSL currently has Rust 1.99.1; Windows Rustup
installation is in progress).

**Primary Dependencies**:

- `slint` / `slint-build` 1.18 for compiled declarative UI.
- `global-hotkey` 0.9 and `tray-icon` for the native Windows shortcut/tray.
- `interprocess` 2.2 for single-instance focus/new-note signaling.
- `rfd` for native folder/executable selection.
- `serde`, `serde_json`, `toml`, `chrono`, `uuid`, `directories`,
  `tracing`, `tracing-subscriber`, `tracing-appender`, and `tempfile` for
  portable persistence, time/metadata, configuration, diagnostics and atomic
  writes.
- `opener` for the Windows-registered Markdown application.
- Scout's existing architecture-matched WorkIQ executable via MCP stdio.

**Storage**: Markdown files under the one configured destination;
user-scoped TOML settings and rolling app logs under OS application-data paths.

**Testing**: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
-- -D warnings`, and portable tests on `ubuntu-24.04` and `ubuntu-24.04-arm`;
Windows desktop Clippy/tests and native release builds on `windows-2025`
(x64) and `windows-11-arm` (ARM64), using each runner's MSVC linker and Windows
SDK.

**Target Platform**: Windows 11 x64 and ARM64; platform-independent logic is
tested on Linux x64 and ARM64, including Debian ARM64 under WSL.

**Project Type**: Rust Cargo workspace; portable core plus Windows desktop
application.

**Performance Goals**: Do not block the UI on file or WorkIQ operations. Warm
local launch should reach the editor within 2 seconds. Idle autosave after
800 ms; during continued typing, save at least every 10 seconds.

**Constraints**: Exactly one selected destination; no recovery-copy fallback;
no credentials/notes/raw meeting details in logs; no additional WorkIQ
installation; meeting discovery must not delay editing; native x64 and ARM64
Rust/MSVC toolchains; CI and release assets must reuse pinned shared workflows
where available.

**Scale/Scope**: Single user, one active capture window, small local Markdown
notes, one optional WorkIQ child process for bounded meeting lookups.

## Constitution Check

- **Immediate and reliable capture**: separate core from UI/integration;
  startup and lookup are asynchronous; editor receives focus immediately.
- **User-controlled persistence**: one configured root; atomic same-directory
  write; visible failures keep text in memory; no silent fallback.
- **Testability**: core holds no GUI dependencies; meeting/serialization/
  autosave tests use fakes; No meeting regression asserts the persisted file.
- **Optional integration/privacy**: no WorkIQ child required to open/save;
  MCP payloads are validated and sanitized; logs omit user data.
- **Focused UI**: text-only editor, generated metadata not editable, keyboard
  workflows, and independent design review.
- **Status**: PASS subject to ARM64 compiler and live Scout capability checks.

## Architecture

### Portable core (`crates/noteiq-core`)

- `model.rs`: capture, meeting, source, save and connection states.
- `capture.rs`: capture controller tying edits, meeting changes, and autosave
  to save state.
- `markdown.rs`: schema 4 vault capture layout with editable Notes, YAML quoting, title/calendar
  escaping, DST-aware meeting times.
- `destination.rs`: selected-root resolution, safe slug, collision reservation,
  note-ID ownership check, atomic write, empty-note close cleanup.
- `note_filename.rs`: `{date}`/`{time}`/`{title}` filename templates,
  validation, and Windows-safe rendering.
- `settings.rs`: local TOML schema, defaults, OneDrive candidate discovery.
- `autosave.rs`: deterministic debounce/interval/retry-backoff state machine; UI owns timers.
- `meeting.rs`: candidate parsing, overlap/match selection, picker ordering and
  labels, automatic re-snap rules, explicit-decision precedence, metadata
  disconnect.
- `launch.rs`: CLI and `noteiq://` request parsing.
- `workiq.rs`: MCP JSON-RPC stdio handshake, tools/list, runtime capability
  validation, bounded tool call, process/error taxonomy.
- `diagnostics.rs`: rolling logs and sensitive-data-free event fields.

### Windows application (`apps/noteiq-windows`)

- `ui/main.slint`: focused editor; a toolbar with the Meeting list (No meeting
  first, two-line rows, Refresh meetings pinned last) and New note, Open note,
  and Settings icon buttons; a problem-only meeting hint; a single save status
  line; settings including the note filename template. The window's native
  title-bar close button closes the capture.
- `ui.rs`: application state, meeting lookups, cache and 60-second
  re-evaluation, picker commits, hints, and save/close flow.
- `settings_ui.rs`, `error_ui.rs`, `meeting_ui.rs`: the notes-folder picker,
  friendly WorkIQ error text, and automatic meeting selection glue.
- `author.rs`: background Windows display-name lookup for the `from` property.
- `open_note.rs`, `foreground.rs`: open the saved file and bring the window
  forward.
- `main.rs`: parse CLI, singleton IPC, initialize settings/logging, open UI.
- `tray.rs`: icon/menu and show/new/quit events.
- `hotkey.rs`: Win+Shift+N registration and settings-selected shortcut.
- `instance.rs`: local IPC server for `new`/`show`; existing instance receives
  launch instead of losing the active editor.
- `uri.rs`: current-user `noteiq://new` handler registration and URI validation.
- `workiq_windows.rs`: Scout path probing and native executable launch.
- Use Slint's Winit event loop. Tray/hotkey managers are created and polled on
  that event-loop thread; worker threads send only bounded messages back to UI.

### Workspace and tests

```text
Cargo.toml
crates/noteiq-core/
├── Cargo.toml
├── src/
└── tests/
apps/noteiq-windows/
├── Cargo.toml
├── build.rs
├── src/
└── ui/
docs/setup.md
specs/001-noteiq-capture/
```

Core tests do not initialize Slint or contact Microsoft 365. A fixture WorkIQ
executable validates stdio/MCP and meeting transitions without actual calendar
data. Windows-specific shell, hotkey, tray, and registry operations are isolated
from the portable core.

## CI/CD and Release Design

The implemented pipeline uses pinned reusable workflows from
`DevSecNinja/.github` for lint, configuration sync, Release Please, labeler,
label sync, and issue assignment to code owners. Pin each reusable-workflow
reference to an immutable commit SHA. No suitable shared Rust CI workflow is
available, so repository-local workflows define the Rust jobs:

- Repository-local `.github/workflows/ci.yml` runs formatting, Clippy, and
  portable tests on
  `ubuntu-24.04` and `ubuntu-24.04-arm`.
- Native desktop Clippy and tests run on `windows-2025` and `windows-11-arm`.
- A separate build job invokes local reusable `build-windows.yml` for both
  targets and verifies each resulting PE architecture.
- `release-please-config.json` manages the virtual Cargo workspace from
  `0.1.0`; its TOML
  config updates root `Cargo.toml` plus workspace package versions in
  `Cargo.lock`, and creates a draft release with tag creation enabled.
- Local `.github/workflows/release.yml` runs for `v*` tags and manual tag
  retries, builds both Windows architectures from the same tagged commit
  through the same local `build-windows.yml`, packages each
  executable with `README.md` and `LICENSE`, generates a `.sha256` file per
  asset, produces GitHub attestations, then uploads to and publishes only the
  pre-existing Release Please draft.

The central release-publish workflow is not used: it creates a release with
`gh release create` and does not upload assets to or finalize the existing
Release Please draft. Setting Release Please's `skip-github-release: true`
would also skip the release/tag creation needed for the draft, so configuration
keeps it `false`. A manual retry must dispatch on the release tag itself
(`gh workflow run release.yml --ref v0.1.0 -f tag=v0.1.0`) so the workflow's
provenance guard sees the requested tag as its source ref.

Repository setup requires the Release Please GitHub App
(ID `3563533`), the `RELEASE_PLEASE_APP_ID` variable and
`RELEASE_PLEASE_APP_PRIVATE_KEY` secret, Actions permission to create pull
requests, and Renovate coverage for the pinned workflow references. These
The Rust CI and release workflows passed local x64-under-emulation, ARM64, and
WSL validation, and local executable architecture checks reported PE machines
`8664` and `AA64`. `yamlfmt -lint`, `yamllint`, `actionlint`, and
`zizmor --offline` passed; a source-only Gitleaks scan found no leaks. Offline
release packaging checks with both real executables verified ZIP entries, PE
machine IDs, SHA-256 values, and guard rejection cases for malformed tags,
wrong workflow refs, and published-release edits. Packaging uses Python's
standard library. GitHub-hosted workflow execution remains outstanding; the
workflows are uncommitted, no remote writes were authorized, and no hosted CI
or release run is claimed.
Checksum and attestation verification do not constitute Authenticode signing
or remove Windows SmartScreen warnings.

## Key Decisions and Tradeoffs

- **Two-crate workspace** keeps Linux core tests independent of native Windows UI
  and UI system-library requirements. The desktop crate is Windows-targeted.
- **Architecture-matched Scout binary** prefers the native `bin/win-x64` or
  `bin/win-arm64` executable, falls back to the other architecture if needed,
  and honors a configured executable path first; the JavaScript launcher is
  not used.
- **Runtime MCP discovery** rejects unsupported or changed schemas with a clear
  user-visible state rather than guessing arguments. If no bounded calendar
  search is exposed, automatic meeting context reports unavailable.
- **Markdown-only output** honors the single-artifact constraint and states
  WorkIQ indexing caveats rather than making an unsupported queryability claim.
- **No hidden recovery destination** honors the user's save-failure choice;
  unsaved body remains visible in the active app.

## Risks and Verification

1. Scout's local executable, package path, and ARM64 payload are present. Verify
   MCP initialization and discovered tool schemas only; do not make a live
   calendar query in development tests.
2. Select a bounded WorkIQ meeting-retrieval capability from actual discovered
   schemas. If unavailable, retain usable notes and clearly report that limit.
3. Windows initially lacked Rust and MSVC tooling. Verify Rustup and required
   Visual Studio Build Tools/Windows SDK before claiming the native build.
4. Slint/tray/hotkey versions change APIs; compile the actual Windows app and
   exercise keyboard focus and the event-loop bridge.
5. OneDrive sync and Markdown visibility do not prove WorkIQ content retrieval;
   user-selected non-empty test notes are not used in live tests or logs.

## Complexity Tracking

No constitution violations. Two workspace crates isolate the platform-specific
UI from the explicitly requested platform-independent testable core.
