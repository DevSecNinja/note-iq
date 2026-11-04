# Research: NoteIQ Quick Capture

## Decisions

### Spec Kit bootstrap and feature location

- **Decision**: Initialize Spec Kit in the current repository using its installed
  1.0.4 CLI and Copilot integration; use feature directory
  `specs/001-noteiq-capture`.
- **Rationale**: The repository was empty and already has a dedicated feature
  branch. Reinitializing git or switching to the CLI-generated branch would
  conflict with the managed worktree.
- **Alternatives considered**: New repository (not needed); Spec Kit branch
  workflow (not compatible with the existing managed branch).
- **Evidence**: `specify init --here --force --non-interactive --integration
  copilot --script ps`; `create-new-feature.ps1 -DryRun -Json` reported the
  001 feature path without modifying the branch.

### Scout WorkIQ discovery and launch

- **Decision**: On Windows, discover the installed Scout package and launch its
  architecture-matched `@microsoft/workiq` native executable directly with
  `mcp`. Never download, install, or copy WorkIQ into the application.
- **Rationale**: Inspection confirmed an installed ARM64 executable at
  `%LOCALAPPDATA%\Programs\Microsoft Scout\resources\app.asar.unpacked\node_modules\@microsoft\workiq\bin\win-arm64\workiq.exe`.
  The bundled package's JavaScript wrapper does not choose that ARM64 path and
  documents a different platform support list; directly selecting the native
  Scout payload avoids that wrapper mismatch.
- **Alternatives considered**: Invoke `bin/workiq.js`, which would resolve
  incorrectly for this Windows ARM64 host; install the public npm package
  independently, which violates reuse-only requirements.
- **Validation**: The installed package declares `workiq` as its CLI entry point
  and bundles the ARM64 executable. Use sanitized fixtures for meeting behavior.
  The response format was confirmed with a masked live probe that printed only
  structure, counts, and timings; no calendar content was recorded or committed.

### WorkIQ meeting retrieval format

- **Decision**: Call `retrieve` with the query `My calendar meetings today`,
  `capabilities: [{"name": "Meetings"}]`, and `strategy: "grounding"`, then
  select the in-progress meeting locally.
- **Rationale**: A query for meetings "active at <timestamp>" returned only the
  next upcoming meeting, while the day query returned the full calendar for the
  day. Local filtering (`start <= now < end`, skipping cancelled and declined
  meetings) is deterministic and testable.
- **Response shape** (Scout WorkIQ, November 2026): `result.content` is empty;
  results are under
  `result.structuredContent["application/vnd.ms-workiq.retrieval"].results[]`.
  Each meeting result has `resourceId` (Exchange event ID, used as the stable
  meeting ID), `entityType: "Meetings"`, `resourceMetadata.meeting.title`
  (`<title>` markup with HTML entities) and `startTime` (`M/D/YYYY`), plus a
  Markdown summary of `- **field:** value` lines:
  - `time`: local, relative text such as `today at 9 AM to today at 9:30 AM`,
    `Monday at 9 AM to Thursday at 5 PM`, or
    `last Friday at 10 AM to today at 10 AM`. Weekday-relative starts are
    anchored with `startTime`; results without a clock time are skipped and
    logged as unrecognised.
  - `organizer`, `invitees`, `additionalMetadata`: Markdown-escaped JSON with
    `\u00XX`-escaped markup (`<person>Name</person>`). `RSVP Info` containing
    `Declined` marks a declined meeting; a `Canceled:` title marks cancellation.
- **Timing**: The handshake took about 2 seconds; the meeting search took
  7–18 seconds (slowest when cold). Handshake requests time out after
  15 seconds and the search after 45 seconds. WorkIQ is shut down in the
  background so results are not delayed by its exit grace period.
- **Assumption**: Relative times use the mailbox time zone, assumed to match
  the Windows time zone.

### MCP protocol boundary

- **Decision**: Treat the local WorkIQ binary as a stdio JSON-RPC/MCP server.
  Initialize it, inspect `tools/list`, and call only a discovered meeting
  retrieval capability with runtime-validated arguments. Bound process time and
  output; redact sensitive values from logs.
- **Rationale**: The installed package is a standalone native executable;
  discovery and schemas can change, so dynamic capabilities must be preferred
  over a hard-coded tool schema.
- **Alternatives considered**: Call the hosted WorkIQ service independently,
  shelling to `workiq ask` without machine-readable output, or assuming a tool
  schema without discovery.

### Markdown retrieval limitations

- **Decision**: Keep one Markdown artifact per note and document that OneDrive
  sync and Markdown visibility do not prove WorkIQ content retrieval.
- **Rationale**: The user explicitly rejected companion files. Microsoft's
  hosted retrieve documentation describes indexed-content search and runtime
  tool discovery, but does not guarantee Markdown support in this user's tenant
  or Scout's bundled local build.
- **Alternatives considered**: Create a Word/PDF companion or block delivery
  until semantic retrieval is available; both conflict with user decisions.
- **Sources**:
  - https://learn.microsoft.com/en-us/microsoft-365/copilot/extensibility/work-iq/mcp/retrieve
  - https://github.com/microsoft/work-iq

### UI and launch framework

- **Decision**: Use Slint for the capture surface and Windows UI; use focused
  Windows integrations for tray, hotkey, CLI/URI forwarding, and folder
  selection, with a small platform-independent domain core.
- **Rationale**: Slint supports compiled `.slint` UI markup, Rust bindings, and
  Windows; Rust, Slint, WorkIQ and hotkey crates support ARM64. The user
  specifically selected Rust/Slint.
- **Alternatives considered**: A browser-based/Electron UI or a full notes
  manager, neither of which matches the requested quick-capture workflow.
- **Sources**:
  - https://docs.rs/slint/latest/slint/
  - https://docs.rs/global-hotkey/latest/global_hotkey/
  - https://docs.rs/tray-icon/latest/tray_icon/

### Rust toolchain and platform testing

- **Decision**: Use Rustup stable in WSL for cross-platform core tests and the
  Windows ARM64 Rustup toolchain for a native release build when the MSVC
  linker/Windows SDK are present.
- **Rationale**: The host runs Windows ARM64; WSL is Debian 13 ARM64. The
  Windows host initially had no Rust toolchain; WSL already had Rustup stable
  1.99.1. Install the native toolchain non-interactively and record native
  linker gaps instead of claiming a Windows build that was not run.
- **Alternatives considered**: Use Debian stable's older Rust package or claim a
  Linux test as proof of the Windows ARM64 build.
