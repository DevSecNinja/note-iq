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
  configured WorkIQ executable when supplied; otherwise prefer the native
  `@microsoft/workiq` executable (`win-x64` or `win-arm64`) and fall back to
  the other bundled architecture. Launch directly with `mcp`; never download,
  install, or copy WorkIQ into the application.
- **Rationale**: Inspection confirmed bundled native x64 and ARM64 payloads.
  The package's JavaScript wrapper does not reliably select the ARM64 path and
  documents a different platform support list; selecting a native Scout
  payload directly avoids that wrapper mismatch.
- **Alternatives considered**: Invoke `bin/workiq.js`, which would resolve
  incorrectly for this Windows ARM64 host; install the public npm package
  independently, which violates reuse-only requirements.
- **Validation**: The installed package declares `workiq` as its CLI entry point
  and bundles native executables. Use sanitized fixtures for meeting behavior.
  The response format was confirmed with a masked live probe that printed only
  structure, counts, and timings; no calendar content was recorded or committed.

### WorkIQ meeting retrieval format

- **Decision**: Call `retrieve` with three queries in one process —
  `My calendar meetings happening now`, `My calendar meetings starting in the
  next hour`, and `My calendar meetings today` — each with
  `capabilities: [{"name": "Meetings"}]` and `strategy: "grounding"`, merge the
  results by ID, then select the meeting locally.
- **Rationale**: Searches return a variable subset of meetings, often about
  ten results rather than the whole calendar. On a busy day the day query returned only past and in-progress
  meetings and missed upcoming ones; the "happening now" and "next hour"
  queries cover the current window directly. WorkIQ answers concurrent
  `tools/call` requests in one process, so three queries take about as long as
  one (about 8 seconds). "Upcoming today" returned nothing and "ended in the
  last 30 minutes" was slow and empty, so neither is used. Local filtering is
  deterministic and testable.
- **Response shape** (Scout WorkIQ, November 2026): `result.content` is empty;
  results are under
  `result.structuredContent["application/vnd.ms-workiq.retrieval"].results[]`.
  Each meeting result has `resourceId` (Exchange event ID, used as the stable
  meeting ID), `entityType: "Meetings"`, `resourceMetadata.meeting.title`
  (`<title>` markup with HTML entities) and `startTime` (`M/D/YYYY`), plus a
  Markdown summary of `- **field:** value` lines:
  - `time`: relative text in **UTC**, such as `today at 9 AM to today at 9:30 AM`,
    `Monday at 9 AM to Thursday at 5 PM`, or
    `last Friday at 10 AM to today at 10 AM`; `today` and `tomorrow` are UTC
    dates. Weekday-relative starts are
    anchored with `startTime`; results without a clock time are skipped and
    logged as unrecognised.
  - `timeUntilMeeting`: timezone-independent status such as
    `event is in progress`, `event is in the past`, `3 minutes`, or
    `1 day, 3 hours and 53 minutes`. NoteIQ reads each response in UTC and in
    local time and keeps the reading that agrees with more of these statuses
    (UTC on a tie), so a WorkIQ change to local times is still handled.
  - `organizer`, `invitees`, `additionalMetadata`: Markdown-escaped JSON with
    `\u00XX`-escaped markup (`<person>Name</person>`). `RSVP Info` containing
    `Declined` marks a declined meeting; a `Canceled:` title marks cancellation.
- **Timing**: The handshake took about 2 seconds; the meeting queries took
  7–18 seconds (slowest when cold). Handshake requests time out after
  15 seconds and the queries share a 45-second limit. WorkIQ is shut down in the
  background so results are not delayed by its exit grace period.
- **Time zone**: Live probes in UTC+1 showed a meeting listed as starting
  `today at 1 PM` reported as starting in `3 minutes` shortly before 14:00
  local time, which only fits UTC. Reading these times as local placed
  meetings an hour early, so an in-progress meeting could appear to have ended.

### MCP protocol boundary

- **Invitee metadata follow-up**: Masked live inspection confirmed invitation
  `type`, `emailAddress.name/address`, and optional `hasAttended` and
  `attendanceDurationMinutes` in retrieval. The discovered `fetch` endpoint
  returns event `attendees[].status.response` and `responseStatus.response`.
  Calendar details supplement retrieval; they do not replace attendance
  observations. The app records reported attendance, not currently joined status.
  Schema 3 writes role-specific invitee lists as flat quoted `"Name <email>"`
  strings, which Obsidian shows as list properties; RSVP and attendance appear
  in the Meeting details section of the body instead of nested YAML.

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

### Vault capture layout (schema 4)

- **Decision**: Match the vault's existing hand-made captures: Obsidian
  properties first (`kind`, `source`, `from`, `date`, `captured`, `processing`,
  `ai_refined`, `status`, `tags`), meeting properties next, NoteIQ bookkeeping
  last; a body with title, From/Captured line, Meeting details, empty Summary
  and Follow-ups sections for later Copilot refinement, and the user's text
  verbatim under an editable Notes heading, without HTML wrappers or generated
  code fences. This supersedes schema 3's frozen Raw capture block, which made
  editing in Obsidian inconvenient. Escape titles and calendar text,
  and show meeting times in the named time zone at each instant.
- **Rationale**: Notes sit alongside existing captures and account notes, so
  shared properties make them filterable together, and Copilot can refine
  them in place, while manual notes stay editable directly in Obsidian.
- **Alternatives considered**: Keep schema 2's nested invitee objects (shown
  poorly in Obsidian's Properties view), migrate existing notes (risks
  overwriting vault edits), or wikilink every invitee (deferred).

### Note filename template

- **Decision**: A Settings field with `{date}`, `{time}`, and `{title}`
  placeholders, default `{date}_{time}_{title}`, a live example, and inline
  validation of both the template and the rendered Windows filename. The
  template is fixed at a note's first save.
- **Rationale**: The user asked for a configurable name, and the UX review
  advised three placeholders, applying changes to new notes only, and
  rejecting problems without punishing typing. Existing collision suffixes make
  templates without a time component safe.
- **Alternatives considered**: Year/month/day placeholders or a preset
  dropdown (unnecessary complexity), renaming saved notes when the setting
  changes (unexpected), and rejecting templates without a unique part.

### Meeting picker and automatic selection

- **Decision**: One Meeting list whose first option is No meeting, unnumbered
  meeting options, a Refresh button, and a hint below for meeting state. Only
  committed choices are applied, an explicit choice is kept for that note, and
  automatic selection is re-evaluated every 60 seconds from the cache so an
  empty note snaps to a meeting once it starts.
- **Rationale**: The user asked for No meeting first, auto-snap, and no
  numbering; a nested UX review found that the previous separate buttons could
  show a meeting while saying No meeting, and asked for sticky choices, honest
  overlaps, and hints separate from save status.
- **Alternatives considered**: Tie-break overlaps by RSVP (acceptance does not
  prove attendance), poll WorkIQ every minute (slow and noisy; local
  re-evaluation suffices), auto-attach from a stale list after a failed lookup,
  and keeping a separate Close button beside the native title-bar close.

### Windows architecture support and CI/release pipeline

- **Decision**: Support Windows x64 and ARM64 builds. Scout WorkIQ discovery
  prefers the native `win-x64` or `win-arm64` payload for the current host,
  falls back to the other bundled architecture, and gives a configured
  executable path precedence.
- **CI**: Repository-local `.github/workflows/ci.yml` runs Rust checks.
  Reuse the pinned `DevSecNinja/.github` reusable workflows for
  linting, configuration sync, Release Please, labeler, label sync, and
  assign-issue-to-codeowners. No central Rust CI workflow is suitable, so
  repository-local CI runs formatting, Clippy, and core tests on
  `ubuntu-24.04` and `ubuntu-24.04-arm`; native desktop Clippy/tests run on
  `windows-2025` and `windows-11-arm`. A separate build job calls the local
  reusable `build-windows.yml` for both targets and verifies executable
  architecture.
- **Release**: `release-please-config.json` uses `release-type: simple` for the virtual
  workspace, starts at `0.1.0`, updates root `Cargo.toml` and workspace package
  versions in `Cargo.lock` through TOML `extra-files`, and sets
  `skip-github-release: false`, `draft: true`, and
  `force-tag-creation: true`. Local `release.yml` responds to `v*` tags and
  manual tag retries, builds both architectures from the exact tagged commit,
  packages the executable, README, and LICENSE in each ZIP, creates
  `SHA256SUMS.txt` and GitHub attestations, and publishes only the draft already
  created by Release Please. Local `.github/workflows/release.yml` uses
  `.github/workflows/build-windows.yml` for the architecture-verified release
  builds.
- **Rationale**: The shared repository workflows provide the required
  organization baseline while local Rust jobs can target the platform-specific
  toolchains. The central release-publish workflow calls `gh release create`;
  it cannot upload assets to and finalize an existing Release Please draft, so
  release artifact upload/publication must remain repository-local.
- **Operator prerequisites**: Install Release Please GitHub App ID `3563533`
  scoped to the repository; configure repository variable
  `RELEASE_PLEASE_APP_ID`, secret `RELEASE_PLEASE_APP_PRIVATE_KEY`, and Actions
  permission to create pull requests; verify Renovate covers the reusable
  workflow references. Signing is not configured. SHA-256 checksums and GitHub
  attestations aid integrity/provenance checks but do not remove SmartScreen.
- **Status**: Workflow files/configuration are implemented. Local x64-under-
  emulation, ARM64, and WSL Clippy/tests/build checks passed; PE machine
  verification reported `8664` and `AA64`; `yamlfmt -lint`, `yamllint`,
  `actionlint`, and `zizmor --offline` passed; a source-only Gitleaks scan
  found no leaks. Offline packaging tests using both real executables verified
  ZIP entries, PE machine IDs, and SHA-256 values. Release guard checks rejected
  malformed tags, ref/provenance mismatch, and published-release edits. The
  Python standard library is used for ZIP packaging. Hosted CI/release
  execution remains outstanding. The uncommitted changes were not pushed, and
  no hosted run is claimed.
