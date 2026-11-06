# Tasks: Reopen meeting notes

**Input**: Design documents in this directory
**Prerequisites**: spec.md, plan.md, research.md, data-model.md, contracts/

## Phase 1: Setup

- [x] T001 Specify occurrence-safe reopening in specs/002-reopen-meeting-notes/spec.md

## Phase 2: Foundational

- [x] T002 Reuse the locked YAML parser in crates/noteiq-core/Cargo.toml and add restored-document state in crates/noteiq-core/src/model.rs

## Phase 3: US1 - Resume a meeting note

**Goal**: Restore saved text and identity through selection.
**Independent test**: Resume a saved capture and save a current draft before switching.

- [x] T003 [US1] Test saved-note restoration in crates/noteiq-core/tests/saved_note_tests.rs
- [x] T004 [US1] Implement capture discovery and loading in crates/noteiq-core/src/saved_note.rs
- [x] T005 [US1] Wire background selection, stale-result guards and safe switching in apps/noteiq-windows/src/ui.rs
- [x] T006 [US1] Test successful switching, failed-save preservation and obsolete responses in apps/noteiq-windows/src/ui.rs

## Phase 4: US2 - Distinct series occurrences

**Goal**: Never mix series occurrences.
**Independent test**: Same calendar identity, different start instants.

- [x] T007 [US2] Test repeated titles, shared IDs, offset equivalence and duplicate ranking in crates/noteiq-core/tests/saved_note_tests.rs
- [x] T008 [US2] Use occurrence-safe picker identity in apps/noteiq-windows/src/ui.rs

## Phase 5: US3 - Preserve vault edits

**Goal**: Preserve refinements and reject external conflicts.
**Independent test**: Edit in the vault before and after reopening.

- [x] T009 [US3] Test refinements, custom YAML, No meeting and external conflicts in crates/noteiq-core/tests/saved_note_tests.rs
- [x] T010 [US3] Preserve original document and path on save/finish in crates/noteiq-core/src/markdown.rs and crates/noteiq-core/src/destination.rs

## Phase 6: Polish

- [x] T011 [P] Complete caption-collapse regression in apps/noteiq-windows/tests/ui_acceptance.rs
- [x] T012 [P] Document reopening and supported layouts in docs/setup.md
- [x] T013 Run native core/controller/UI tests, Clippy and formatting per specs/002-reopen-meeting-notes/quickstart.md

## Dependencies & Execution Order

T001 -> T002 -> T003/T004 -> T005/T006. T007/T008 and T009/T010 build on
the restored capture model. T013 follows all implementation and regression work.
T011 and T012 are independent polish tasks.

## Parallel Examples

US1 controller tests can be designed alongside core restoration tests.
US2 occurrence fixtures and US3 preservation fixtures can be specified separately,
then combined in the same runner. Caption regression and documentation are independent.

## Implementation Strategy

Implement restoration and preservation as one safe MVP; then wire background
selection and exercise all persistence failures before claiming completion.
No commit, deployment or runtime restart is included.
