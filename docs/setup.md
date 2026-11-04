# NoteIQ setup and troubleshooting

## Requirements

- Windows 11 on ARM64.
- A Rust toolchain with the `aarch64-pc-windows-msvc` target.
- Visual Studio Build Tools with the ARM64 C++ linker and Windows 11 SDK to
  build the native executable.
- Microsoft Scout, only when meeting context is wanted.

Meeting context is optional. NoteIQ remains usable when Scout or WorkIQ is
missing, unavailable, signed out, or unable to reach the WorkIQ service.

## Build and launch

From the repository root in an ARM64 Windows developer shell:

```powershell
cargo build --release --features desktop -p noteiq-windows --target aarch64-pc-windows-msvc
& .\target\aarch64-pc-windows-msvc\release\noteiq.exe
```

When NoteIQ is running, press **Win+Shift+N** to show its capture window.
`Ctrl+S` saves without closing, `Ctrl+N` begins a new capture, and the tray
menu provides Show, New note, and Quit actions. The app registers
`noteiq://new` for the current Windows user; a PowerToys Command Palette
command can invoke `noteiq.exe new` or the registered URI.

**Close** finishes the capture and hides the window; NoteIQ keeps running in
the tray so the shortcut stays available. Use **Quit** in the tray menu to
exit. Running `noteiq new` (or opening `noteiq://new`) while a capture is open
brings that capture to the front without discarding its text; after the
capture is closed, it starts a fresh one.

### Changing the capture shortcut

Another app may already own **Win+Shift+N**. NoteIQ then shows "Win+Shift+N is
already used by another app" and keeps working without a shortcut. Open
**Settings**, type another shortcut such as `Ctrl+Alt+N`, and press **Enter**
or **Done** to apply it. NoteIQ accepts a letter, digit, or F-key combined
with Ctrl, Alt, Shift, or Win. If the new shortcut is also taken, the previous
one stays active.

Portable tests and formatting can run in WSL:

```sh
cargo test -p noteiq-core
cargo fmt --all -- --check
```

The Windows UI acceptance tests run natively on Windows. They use Slint's
testing backend, which needs the debug information that `build.rs` enables
only for debug builds:

```powershell
cargo test -p noteiq-windows --features desktop
```

## Notes and destinations

By default, NoteIQ writes Markdown under:

```text
%USERPROFILE%\OneDrive - Microsoft\projects\personal-assistants\notes\captures\YYYY\MM\
```

Each filename includes the local date/time and a slug from the first line of
the note. Settings lets you select another single destination folder; NoteIQ
never writes to both. An alternate OneDrive `Notes` folder can be selected
when it exists.

The Markdown front matter records the capture time, timezone, launch source,
tags, and—when connected—meeting details. The body remains ordinary Markdown.
`Ctrl+S` saves without closing; autosave runs after an 800 ms pause and at
least every 10 seconds during continuous typing. An empty saved note is
retained while editing and deleted when the capture is closed.

## Scout and WorkIQ

NoteIQ looks for Scout's native ARM64 WorkIQ executable at:

```text
%LOCALAPPDATA%\Programs\Microsoft Scout\resources\app.asar.unpacked\node_modules\@microsoft\workiq\bin\win-arm64\workiq.exe
```

It invokes that executable directly using the MCP stdio interface. NoteIQ does
not install, download, or copy WorkIQ. If the executable is missing, or its
MCP schema does not expose a compatible Meetings search, the meeting status
explains that context is unavailable; note editing and saving continue to
work. Meeting retrieval also needs the connectivity and authentication
required by Scout/WorkIQ.

NoteIQ connects automatically only when exactly one eligible meeting overlaps
the current time and auto-connect is enabled. If meetings overlap, choose one
in the capture window. **No meeting** removes the meeting and attendee fields
from the saved note without changing the note body or ID. A deliberate
disconnect or manual connection takes precedence over a delayed lookup.

Markdown files are saved in OneDrive or an Obsidian vault, but that alone does
not guarantee immediate indexing by WorkIQ. Indexing and query availability
depend on the user's Microsoft 365 and WorkIQ configuration.

## Diagnostics

Rolling logs are stored under:

```text
%LOCALAPPDATA%\DevSecNinja\NoteIQ\data\logs\
```

They include operation type, elapsed time, failure class, and a correlation
ID. They intentionally exclude note text, meeting titles, attendees,
credentials, and raw WorkIQ responses. The app retains recent log files for up
to 14 days. For more detail, start NoteIQ with `RUST_LOG=noteiq_core=debug`
set in the environment.

A meeting lookup waits up to 45 seconds for WorkIQ before reporting that
meeting context is unavailable.

If meeting lookup fails, use **Find meeting** to retry, confirm Scout is
installed and signed in, and verify network access. If a folder is unavailable
or unwritable, select another destination in Settings; NoteIQ does not silently
switch folders.
