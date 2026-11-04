# NoteIQ

NoteIQ is a Windows x64 and ARM64 quick-capture app for Markdown notes. It opens a
small editor, timestamps each capture, optionally adds current-meeting context
from Scout's existing WorkIQ executable, and writes to one selected notes
folder.

The default destination is the current user's
`OneDrive - Microsoft\projects\personal-assistants\notes` vault. Notes are
created under `captures\YYYY\MM\`. NoteIQ does not install WorkIQ, duplicate
notes into another location, or require an Obsidian process.

Filenames follow the **Note filename** setting, `{date}_{time}_{title}` by
default, such as `2026-11-03_1415_project-review.md`. The title is the meeting
name, or the note's first line when no meeting is attached. While a note is
open, its file follows meeting changes without losing its timestamp or
overwriting other notes. Closed notes are never renamed, and a changed setting
applies only to new notes.

## Build and test

Run the portable core tests on Linux x64 or ARM64 (including WSL):

```sh
cargo test -p noteiq-core
cargo fmt --all -- --check
```

Run the Windows UI acceptance tests natively on Windows:

```powershell
cargo test -p noteiq-windows --features desktop
```

Build on Windows x64 with the x64 Rust target, Visual Studio C++ tools, and
Windows SDK:

```powershell
cargo build --release --features desktop -p noteiq-windows --target x86_64-pc-windows-msvc
```

For a native ARM64 build, use a Windows ARM64 developer environment with the
ARM64 Rust target, Visual Studio C++ tools, and Windows SDK:

```powershell
cargo build --release --features desktop -p noteiq-windows --target aarch64-pc-windows-msvc
```

The resulting `noteiq.exe` is under the corresponding
`target\<target-triple>\release\` directory.

## CI and releases

The CI/CD workflows are implemented for Linux x64/ARM64 core checks and
Windows x64/ARM64 desktop checks and release packages. Shared repository
automation reuses pinned `DevSecNinja/.github` workflows; Rust checks, Windows
builds, and draft-release publishing remain repository-local. Local x64
(emulated), ARM64, and WSL validation passed, including PE architecture
verification. GitHub-hosted CI has not run: these changes are uncommitted and
no remote writes were authorized. Release archives include the executable,
README, LICENSE, a SHA-256 checksum manifest, and GitHub build attestations. See
[setup and release verification](docs/setup.md#ci-cd-and-release-verification)
for release verification, the tag-bound manual retry command, and required
repository setup. Downloads are not Authenticode-signed; checksums and
attestations do not remove the Windows SmartScreen warning.

## Launch

Start `noteiq.exe` to open a capture. While NoteIQ is running, use
**Win+Shift+N** to show the capture window. If another app already owns that
shortcut, choose one such as `Ctrl+Alt+N` in **Settings** and press **Enter**
or **Done** to apply it. `Ctrl+S` saves without closing, **New note** or `Ctrl+N` starts
another capture, and **Open note** opens the saved Markdown file with the
registered application. Closing the window with its title-bar **×** saves the
note, deletes it if it is empty, and keeps NoteIQ in the tray; use the tray
menu to show the window, start a new note, or quit.

The app registers the per-user `noteiq://new` protocol on first launch. This
can be entered or invoked by a PowerToys Command Palette command. The CLI
command `noteiq new` brings an open capture to the front, or starts a fresh
capture when none is open.

The **Meeting** list starts with **No meeting**, followed by current, earlier,
and upcoming meetings with their dates and times. A note attaches automatically
when exactly one meeting is in progress, including one that starts while an
empty note is open; a meeting you choose, or **No meeting**, is never replaced.
The tray process retains the list between notes, and **Refresh** asks WorkIQ
again. Saved meeting metadata groups required and optional invitees using
`Name <email>` values; the note body lists their RSVP and reported attendance.
See [Choosing a meeting](docs/setup.md#choosing-a-meeting) and
[Note layout](docs/setup.md#note-layout).

For folder selection, settings, Scout/WorkIQ troubleshooting, and logs, see
[docs/setup.md](docs/setup.md).
