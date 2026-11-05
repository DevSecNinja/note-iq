<!--
Sync Impact Report
Version change: scaffold → 1.0.0 (initial ratification)
Modified principles: none (initial project principles established)
Added sections: additional constraints; development workflow
Removed sections: none
Follow-up TODOs: none
-->

# NoteIQ Constitution

## Core Principles

### I. Capture Must Be Immediate and Reliable

Opening a capture MUST place the cursor in a plain-text Markdown editor without
requiring a title or metadata form. Saving MUST preserve user text and explicit
context; closing an empty capture MUST remove its empty file without deleting it
while the user is still editing.

### II. User Data Stays Under User Control

Each note MUST be written to exactly one selected destination. A failed write
MUST remain visible and MUST NOT silently fall back, duplicate, or claim success.
File replacement and empty-note cleanup MUST avoid losing newer edits or
overwriting an unrelated note.

### III. Observable Behavior Is Testable

Platform-independent behavior MUST be tested independently of Windows UI and
live Microsoft 365 services. Meeting changes, especially choosing No meeting,
MUST have regression tests that inspect the resulting saved Markdown. Failures
MUST be actionable to the user and diagnosable without logging note content or
private meeting data.

### IV. Integrations Are Optional and Explicit

The note editor and file saving MUST work when Scout, WorkIQ, authentication,
network access, or calendar context is unavailable. Meeting lookup MUST use the
documented local integration, validate discovered capabilities and returned
data, and distinguish lookup failure from no matching meeting. User disconnect
or meeting selection MUST take precedence over a delayed lookup result.

### V. Keep the Capture Surface Focused

The capture window MUST remain keyboard-first, accessible, and limited to
features that reduce interruption. Plain Markdown is the editing format.
Generated context MUST remain separate from user-editable note text, and
diagnostic logging MUST exclude note bodies, credentials, and raw meeting data.

## Additional Constraints

- The application is a Rust desktop app using Slint, with Windows 11 Fluent-
  inspired UI and a native Windows ARM64 release build.
- The Rust domain and storage core MUST run platform-independent tests on Linux
  under WSL. Windows-only integrations MUST be isolated behind boundaries that
  can be validated without a live account.
- Notes are Markdown files in one chosen destination: the configured vault's
  `captures/YYYY/MM/` folder by default, or an explicitly selected
  auto-detected OneDrive `Notes` folder.
- WorkIQ retrieval support and tenant indexing are environmental capabilities,
  not assumed from OneDrive sync; documentation MUST state any limitation.

## Development Workflow

- New product behavior MUST be specified with independently testable user
  stories and acceptance scenarios before implementation.
- Behavior that can be unit tested MUST have focused unit tests. Cross-component
  persistence, meeting-state transitions, and platform adapters MUST have
  integration or fixture-based tests.
- Changes MUST retain the selected destination, editor contents, and privacy
  constraints. Build and relevant test failures MUST be resolved before claiming
  completion.
- Significant UI changes MUST receive a focused design review covering
  keyboard operation, focus, accessibility, and key application states.

## Governance

This constitution governs feature specifications, plans, implementation, and
reviews. Amendments MUST be made in this file, state their version rationale,
and update the last-amended date. Version increments follow semantic versioning:
MAJOR for incompatible principle changes, MINOR for new principles or material
expansion, and PATCH for clarifications. Reviews MUST check tests, persistence,
single-destination behavior, accessibility, and privacy-safe diagnostics.

**Version**: 1.0.0 | **Ratified**: 2026-11-03 | **Last Amended**: 2026-11-03
