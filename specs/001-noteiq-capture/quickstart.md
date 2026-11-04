# Quickstart: NoteIQ Quick Capture

## Prerequisites

- Rustup stable and Cargo.
- Windows 11 ARM64 with the MSVC linker and Windows SDK for the native desktop
  build.
- WSL2 Debian 13 ARM64 for platform-independent test runs.
- Scout installed with its bundled ARM64 WorkIQ executable for optional meeting
  discovery. No separate WorkIQ install is required or supported.

## Build and test the core in WSL

From the repository worktree mounted in WSL:

```bash
cargo fmt --all -- --check
cargo test --workspace
```

Expected: formatting passes and all unit/integration tests pass without network
access to Microsoft 365 or a WorkIQ executable.

## Build the Windows application

From PowerShell on Windows ARM64:

```powershell
cargo build --release --target aarch64-pc-windows-msvc
```

Expected: a native ARM64 release binary under
`target/aarch64-pc-windows-msvc/release/`.

## Verify the capture lifecycle

1. Start `noteiq` and type a short note.
2. Wait for idle autosave; verify the Markdown file in the selected destination.
3. Type continuously for longer than the interval; verify saved content advances
   without closing the editor.
4. Press Ctrl+S; verify the editor stays open and reports Saved.
5. Erase all text; verify the file remains while editing.
6. Close the capture; verify the empty note is deleted.
7. Type a note, choose Open note; verify the exact file opens.

## Verify meeting behavior

Use the WorkIQ fixture tests to exercise zero, one, and overlapping meetings.
In particular, save a note with meeting context, choose No meeting, save again,
then verify the file contains the same body and no meeting or attendee metadata.

Run an optional live integration check only when the user explicitly accepts the
tenant authentication prompt. Do not collect or record actual calendar content
in test logs.

## Troubleshooting

If Scout or its WorkIQ executable is unavailable, NoteIQ must still open and
save notes; Settings offers a WorkIQ executable picker and the UI explains the
missing dependency. Review the rolling diagnostic logs for sanitized failure
codes. See `docs/setup.md` for install, shortcut, Command Palette, and
Markdown/WorkIQ retrieval details.
