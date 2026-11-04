# NoteIQ

NoteIQ is a Windows ARM64 quick-capture app for Markdown notes. It opens a
small editor, timestamps each capture, optionally adds current-meeting context
from Scout's existing WorkIQ executable, and writes to one selected notes
folder.

The default destination is the current user's
`OneDrive - Microsoft\projects\personal-assistants\notes` vault. Notes are
created under `captures\YYYY\MM\`. NoteIQ does not install WorkIQ, duplicate
notes into another location, or require an Obsidian process.

## Build and test

Run the portable core tests in WSL:

```sh
cargo test -p noteiq-core
cargo fmt --all -- --check
```

Run the Windows UI acceptance tests natively on Windows:

```powershell
cargo test -p noteiq-windows --features desktop
```

Build the native ARM64 Windows app from a Windows ARM64 Rust environment with
the Visual Studio C++ ARM64 build tools and Windows SDK installed:

```powershell
cargo build --release --features desktop -p noteiq-windows --target aarch64-pc-windows-msvc
```

The resulting `noteiq.exe` is in
`target\aarch64-pc-windows-msvc\release\`.

## Launch

Start `noteiq.exe` to open a capture. While NoteIQ is running, use
**Win+Shift+N** to show the capture window. If another app already owns that
shortcut, choose one such as `Ctrl+Alt+N` in **Settings** and press **Enter**
or **Done** to apply it. `Ctrl+S` saves without closing, `Ctrl+N` starts
another capture, and **Open note** opens the saved Markdown file with the
registered application. **Close** hides the window and keeps NoteIQ in the
tray; use the tray menu to show the window, start a new note, or quit.

The app registers the per-user `noteiq://new` protocol on first launch. This
can be entered or invoked by a PowerToys Command Palette command. The CLI
command `noteiq new` brings an open capture to the front, or starts a fresh
capture when none is open.

For folder selection, settings, Scout/WorkIQ troubleshooting, and logs, see
[docs/setup.md](docs/setup.md).
