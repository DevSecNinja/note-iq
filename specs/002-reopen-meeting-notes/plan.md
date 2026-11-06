# Implementation Plan: Reopen meeting notes

**Branch**: `devsecninja-noteiq-capture-app` | **Date**: 2026-11-06
**Spec**: [spec.md](spec.md)

## Summary

Read saved captures in the selected destination on a background worker. Restore
the selected occurrence only after the current capture saves/finishes successfully.
Keep the original document to preserve refinements and detect external changes.

## Technical Context

**Language/Version**: Existing Rust workspace
**Primary Dependencies**: Slint, chrono, yaml-rust2 (already locked/tested)
**Storage**: Existing captures/year/month Markdown files
**Testing**: Core integration tests; Windows controller and Slint acceptance tests
**Target Platform**: Windows ARM64; platform-independent storage core
**Project Type**: Desktop application
**Performance Goals**: File discovery must not block the UI event loop
**Constraints**: No live account or personal vault data in tests; no destructive migration
**Scale/Scope**: Local capture history, no separate series browser

## Constitution Check

Pass before and after design: one destination, save-before-switch, explicit
errors, no note content in logs, occurrence-safe matching and focused regression
coverage. Existing controls suffice; no new significant UI surface.

## Project Structure

- `crates/noteiq-core/src/saved_note.rs`: discovery, parsing and preservation
- `crates/noteiq-core/src/{model,markdown,destination}.rs`: restored document lifecycle
- `crates/noteiq-core/tests/saved_note_tests.rs`: persistence regressions
- `apps/noteiq-windows/src/ui.rs`: asynchronous selection and safe editor replacement
- `apps/noteiq-windows/tests/ui_acceptance.rs`: caption layout regression
- `docs/setup.md`: selection behavior and limitations

## Complexity Tracking

No constitutional deviations.
