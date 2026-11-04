# Implementation Plan: NoteIQ Quick Capture

**Branch**: `devsecninja-noteiq-capture-app` | **Date**: 2026-11-03 |
**Spec**: [spec.md](spec.md)

**Input**: Approved feature specification in
`specs/001-noteiq-capture/spec.md`.

## Summary

Build a Windows ARM64 Rust/Slint quick-capture app with a small, independently
testable Rust core. The core owns note metadata, safe destination paths,
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

**Testing**: `cargo fmt --check`, `cargo test -p noteiq-core` in WSL,
`cargo test --workspace` when the WSL windowing prerequisites are available,
and `cargo build --release --target aarch64-pc-windows-msvc` with the native
MSVC linker and Windows SDK.

**Target Platform**: Windows 11 ARM64; platform-independent logic is tested in
Debian ARM64 under WSL.

**Project Type**: Rust Cargo workspace; portable core plus Windows desktop
application.

**Performance Goals**: Do not block the UI on file or WorkIQ operations. Warm
local launch should reach the editor within 2 seconds. Idle autosave after
800 ms; during continued typing, save at least every 10 seconds.

**Constraints**: Exactly one selected destination; no recovery-copy fallback;
no credentials/notes/raw meeting details in logs; no additional WorkIQ
installation; meeting discovery must not delay editing; Rust ARM64 toolchains.

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
- `markdown.rs`: versioned YAML front matter, escaping, serialization.
- `destination.rs`: selected-root resolution, safe slug, collision reservation,
  atomic write, empty-note close cleanup.
- `settings.rs`: local TOML schema, defaults, OneDrive candidate discovery.
- `autosave.rs`: deterministic debounce/interval state machine; UI owns timers.
- `meeting.rs`: candidate parsing, overlap/match selection, explicit-decision
  precedence, metadata disconnect.
- `workiq.rs`: MCP JSON-RPC stdio handshake, tools/list, runtime capability
  validation, bounded tool call, process/error taxonomy.
- `diagnostics.rs`: rolling logs and sensitive-data-free event fields.

### Windows application (`apps/noteiq-windows`)

- `ui/main.slint`: focused editor, context chip, meeting chooser, settings,
  save state, error strip, Open note button.
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

## Key Decisions and Tradeoffs

- **Two-crate workspace** keeps Linux core tests independent of native Windows UI
  and UI system-library requirements. The desktop crate is Windows-targeted.
- **Direct Scout ARM64 binary** uses the actual `bin/win-arm64/workiq.exe`; the
  installed JavaScript launcher selects x64 under ARM64 and is not used.
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
