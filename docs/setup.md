# NoteIQ setup and troubleshooting

## Requirements

- Windows 11 x64 or ARM64.
- A Rust toolchain with the native target (`x86_64-pc-windows-msvc` or
  `aarch64-pc-windows-msvc`).
- Visual Studio Build Tools with the matching x64 or ARM64 C++ linker and
  Windows 11 SDK to build the native executable.
- Microsoft Scout, only when meeting context is wanted.

Meeting context is optional. NoteIQ remains usable when Scout or WorkIQ is
missing, unavailable, signed out, or unable to reach the WorkIQ service.

## Build and launch

From the repository root in a Windows developer shell with the matching
architecture's C++ tools installed:

```powershell
cargo build --release --features desktop -p noteiq-windows --target x86_64-pc-windows-msvc
& .\target\x86_64-pc-windows-msvc\release\noteiq.exe
```

For a native ARM64 build, substitute
`aarch64-pc-windows-msvc` in both commands. The Windows build runners used by
CI are `windows-2025` for x64 and `windows-11-arm` for ARM64; each build must
verify that the produced executable matches its requested architecture.

When NoteIQ is running, press **Win+Shift+N** to show its capture window.
`Ctrl+S` saves without closing, **New note** or `Ctrl+N` safely finishes the
current capture and begins a new one, and the tray
menu provides Show, New note, and Quit actions. The app registers
`noteiq://new` for the current Windows user; a PowerToys Command Palette
command can invoke `noteiq.exe new` or the registered URI.
The equivalent `noteiq://new/` form added by the Windows shell is also supported;
additional paths, query parameters, and fragments are rejected.

Closing the window with the Windows title-bar close button (×) finishes the
capture and hides the window; NoteIQ has no separate Close button. It keeps
running in the tray so the shortcut stays available. Use **Quit** in the tray
menu to exit. Running `noteiq new` (or opening `noteiq://new`) while a capture
is open brings that capture to the front without discarding its text; after
the capture is closed, it starts a fresh one.

The capture window uses a consistent light Fluent theme, including buttons,
the meeting picker, and the editor, even when Windows uses dark app mode.
Settings replaces the capture panel rather than taking space away from the
note; at smaller window sizes its contents scroll while **Done** stays visible.

The window, taskbar, tray, executable, and `noteiq://` links use the same
embedded NoteIQ icon. The build generates 16-256px Windows icon sizes from
`apps/noteiq-windows/assets/noteiq.svg`; no loose image files are needed at
runtime. If an older pinned shortcut still shows a generic icon after an update,
unpin it and pin the rebuilt executable again.

### Changing the capture shortcut

Another app may already own **Win+Shift+N**. NoteIQ then shows "Win+Shift+N is
already used by another app" and keeps working without a shortcut. Open
**Settings**, type another shortcut such as `Ctrl+Alt+N`, and press **Enter**
or **Done** to apply it. NoteIQ accepts a letter, digit, or F-key combined
with Ctrl, Alt, Shift, or Win. If the new shortcut is also taken, the previous
one stays active.

**OneNote conflict.** Desktop OneNote's "Send to OneNote" tray helper
(`ONENOTEM.EXE`) registers **Win+Shift+N** (new side note) and **Win+Alt+N**
(open OneNote). To release them, open OneNote, choose **File > Options >
Display**, and clear **Place OneNote icon in the notification area of the
taskbar** (stored as
`HKCU\Software\Microsoft\Office\16.0\OneNote\Options\Other\RunSystemTrayApp = 0`).
Then end `ONENOTEM.EXE` or sign out and back in, and restart NoteIQ. You lose
OneNote's side-note and open-OneNote shortcuts and its tray icon; select the
option again to get them back.

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

Settings lets you select another single destination folder; NoteIQ
never writes to both. An alternate OneDrive `Notes` folder can be selected
when it exists.

`Ctrl+S` saves without closing; autosave runs after an 800 ms pause and at
least every 10 seconds during continuous typing. An empty saved note is
retained while editing and deleted when the capture is closed.

If the destination folder is unavailable, NoteIQ keeps your text and shows
**Choose folder**. It retries automatically after 5 seconds, doubling the pause
up to one minute, so the log is not flooded; **Save** or `Ctrl+S` retries at once.

### Note filenames

The **Note filename** setting names each note. The default,
`{date}_{time}_{title}`, gives names such as `2026-11-03_1415_project-review.md`.

| Placeholder | Becomes |
|---|---|
| `{date}` | The capture's local date, `YYYY-MM-DD` |
| `{time}` | The capture's local time, `HHmm` |
| `{title}` | A safe slug of the attached meeting's name, else of the note's first non-empty line |

The `.md` extension is added automatically. **Use date–title style** fills in
`{date}-{title}`, the naming used by the vault's own captures, and **Use default
style** restores the default. Settings shows an example name while you type;
press **Enter** or **Done** to apply it. A template is rejected, with an
explanation and the previous setting kept, if it is empty or contains a folder
separator, an unknown placeholder, unbalanced braces, a control character, a
character that Windows or Obsidian links cannot use (`< > : " | ? * # ^ [ ]`),
a leading or trailing dot, a reserved Windows name such as `con`, or could
produce a name longer than 120 characters. A template without `{time}` or
`{title}` is allowed: another note with the same name in that folder gets a
numeric suffix such as `-2`. A title that would form a reserved Windows name
gets a `note-` prefix.

A changed template applies to new notes, and to the open note if it has not been
saved yet. A saved note keeps the template it was first saved with, and notes
always stay in `captures\YYYY\MM`. When that template uses `{title}`, connecting,
switching, or disconnecting a meeting renames the open note, even if autosave ran
before WorkIQ found the meeting. The original date, time, folder, and note ID are
kept, and a collision gets a numeric suffix instead of replacing another note.
Typing alone does not rename a saved file. Obsidian links to a renamed file may
need updating; NoteIQ does not rewrite vault links or rename closed notes.

### Note layout

New notes follow the vault's capture template (`_templates/capture.md`) and are
marked `noteiq_schema: 3`. Notes written by earlier versions are never migrated
or rewritten.

The front matter starts with the vault's properties: `kind: capture`,
`source: manual`, `from` (a `[[Name]]` link to you, when known), `date`,
`captured` (with milliseconds and UTC offset), `processing: raw`,
`ai_refined: false`, `status: inbox`, and `tags` (`capture`, `note`, plus any of
yours). With a meeting attached, `meeting`, `meeting_start`, `meeting_end`,
`organizer`, `your_rsvp`, the invitee lists, and `calendar_event_id` follow.
NoteIQ's bookkeeping comes last: `id`, `updated`, `timezone`, `captured_via`
(`shortcut`, `tray`, `cli`, `uri`, or `new_note`), and `noteiq_schema`.

The body starts with a `# Title` (the meeting name, else the note's first line,
else "Quick note"), a **From** and **Captured** line, and a comment explaining
how to refine the note. With a meeting attached, **Meeting details** lists the
time and duration, organizer, your RSVP, and invitees. Empty **Summary** and
**Follow-ups** sections are left for later refinement, and your text is kept
verbatim in a collapsed **Raw capture** block, frozen as non-AI evidence.
Titles and calendar text are escaped so they display literally.

`from` uses your Windows display name, looked up in the background when NoteIQ
starts. To use another name, set `author_name` in the settings file. If the
lookup finishes after the open note was saved, that note is updated; closed
notes are not, so later vault edits are never overwritten.

Invitees are flat quoted `"Name <email>"` strings in `required_invitees` and
`optional_invitees`, which are written as `[]` when empty; `resource_invitees`
and `other_invitees` appear only when needed. Missing names or addresses are
not invented. Meeting times use your Windows time zone, including the correct
offset when a meeting spans a daylight-saving change.

`your_rsvp` is `accepted`, `tentative`, `declined`, `not_responded`, or
`organizer`, and is omitted when unknown. Under **Meeting details**, each invitee
shows their RSVP and, when WorkIQ reports it, "attended" or "did not attend",
based only on WorkIQ's explicit `hasAttended` field. This is a snapshot of
reported attendance, **not a live indication that someone is currently in the
call**; accepting an invitation does not prove attendance. Some meetings omit
attendance or RSVP details.

### Settings file

Settings are kept in `%APPDATA%\DevSecNinja\NoteIQ\config\settings.toml`, which
is created the first time you change a setting. Besides the options in
Settings, it accepts `author_name`. A hand-edited `note_filename` that is not
valid falls back to the default, and the log records why without the template
text.

## Scout and WorkIQ

NoteIQ checks Scout's bundled WorkIQ executable for the native Windows
architecture first (`win-x64` or `win-arm64`), then tries the other bundled
architecture if needed. A configured executable path takes precedence. The
corresponding native ARM64 path is:

```text
%LOCALAPPDATA%\Programs\Microsoft Scout\resources\app.asar.unpacked\node_modules\@microsoft\workiq\bin\win-arm64\workiq.exe
```

The corresponding x64 path is:

```text
%LOCALAPPDATA%\Programs\Microsoft Scout\resources\app.asar.unpacked\node_modules\@microsoft\workiq\bin\win-x64\workiq.exe
```

It invokes that executable directly using the MCP stdio interface. NoteIQ does
not install, download, or copy WorkIQ. If no compatible executable is found, or
its MCP schema does not expose a compatible Meetings search, the meeting hint
explains that context is unavailable; note editing and saving continue to
work. Meeting retrieval also needs the connectivity and authentication
required by Scout/WorkIQ.

## CI/CD and release verification

Repository-local `.github/workflows/ci.yml` runs portable formatting, strict
Clippy (`-D warnings`), and core tests on both
`ubuntu-24.04` and `ubuntu-24.04-arm`. Desktop Clippy and tests run on
`windows-2025` (x64) and `windows-11-arm` (ARM64). A separate build job calls
the repository's reusable `build-windows.yml` for both targets and verifies
each executable's architecture. Shared lint, configuration-sync,
Release Please, labeler, label-sync, and assign-issue-to-codeowners automation
must reuse workflows from `DevSecNinja/.github`, referenced by immutable,
pinned commit SHA. The shared lint workflow retains its default non-blocking
behavior; the Rust checks in local CI are strict gates. There is no suitable
central Rust workflow, so the repository-local CI and Windows build workflows
provide the Rust-specific jobs.

`release-please-config.json` manages a virtual workspace release using the
simple release type, initially `0.1.0`. Its configuration updates the root
`Cargo.toml` and workspace package versions in
`Cargo.lock` as extra files, creates a draft GitHub release, and enables
`force-tag-creation`. The local `.github/workflows/release.yml` runs on `v*`
tags or by manual tag retry, builds both architectures from that exact tagged
commit through the local reusable `build-windows.yml`, verifies each
executable's architecture, and packages each ZIP with `noteiq.exe`,
`README.md`, and `LICENSE`. It also publishes `SHA256SUMS.txt` and GitHub
attestations. Publishing may only
complete an existing draft created by Release Please; it must not create a
separate release. The central release-publish workflow is not suitable because
it creates a release with `gh release create` instead of uploading assets to
and finalizing the existing Release Please draft.
Setting `skip-github-release: true` would skip the Release Please release/tag
creation rather than make the draft available for this publisher, so keep it
`false`.

For a manual retry, dispatch the workflow on the existing release tag, not on
`main`. The requested tag must already have an unpublished Release Please
draft:

```powershell
gh workflow run release.yml --ref v0.1.0 -f tag=v0.1.0
```

The tag-bound dispatch is required by the workflow's provenance guard, which
checks that the workflow ref is the same tag as the requested release source.
The workflow also checks the tag version and ancestry before building.

Before enabling Release Please automation, install the GitHub App
(App ID `3563533`) for this repository. Set the repository variable
`RELEASE_PLEASE_APP_ID` and repository secret
`RELEASE_PLEASE_APP_PRIVATE_KEY`, and enable **Settings > Actions > General >
Workflow permissions > Allow GitHub Actions to create and approve pull
requests**. Confirm Renovate covers the newly referenced reusable workflows.
These are required operator setup steps and must be completed by a repository
administrator before the Release Please workflow can run. They have not been
configured or validated here.

Local validation completed for the implemented workflows: x64 Clippy, full
tests, and release build passed under emulation; ARM64 Clippy, full tests, and
release build passed; WSL Clippy and full tests passed. The produced Windows
executables were verified as PE machine `8664` (x64) and `AA64` (ARM64).
`yamlfmt -lint`, `yamllint`, `actionlint`, and `zizmor --offline` passed.
A source-only Gitleaks scan found no leaks. The release packaging script was
exercised offline with both real executables: ZIP contents, PE machine IDs, and
SHA-256 checksums all passed. Release guard checks rejected malformed tags,
workflow-ref/provenance mismatches, and attempts to edit a published release.
The ZIP packaging uses Python's standard library and does not require an
external ZIP utility. GitHub Actions has not run these uncommitted workflow
changes; no remote writes were authorized, so hosted CI and release publication
are not claimed.

After downloading a release archive, compare its hash with the matching entry
in `SHA256SUMS.txt`:

```powershell
Get-FileHash .\noteiq-win-x64.zip -Algorithm SHA256
Get-Content .\SHA256SUMS.txt
```

GitHub CLI can verify the artifact's provenance attestation (replace the
archive name with the ARM64 package when appropriate):

```powershell
$repo = gh repo view --json nameWithOwner --jq .nameWithOwner
gh attestation verify .\noteiq-win-x64.zip --repo $repo
```

The checksums detect accidental corruption and attestations verify provenance;
neither is code signing. Releases are not Authenticode-signed, so Windows
SmartScreen may still show an unknown-publisher warning.

### Choosing a meeting

The **Meeting** list at the top of the note shows which meeting is attached. Its
first entry is always **No meeting**; the others are retrieved meetings, each
labelled with its title, date, and local time, for example
`Project review · 04 Nov · 14:00–14:30`. Meetings in progress come first, then
earlier meetings (most recent first), then upcoming ones. Declined and
cancelled meetings are not listed, although a meeting already attached to the
note stays listed even if a later lookup no longer returns it. Until a lookup
finds meetings, the list only offers **No meeting** and is unavailable.

Choosing an entry attaches it at once; choosing **No meeting** removes all
meeting and invitee fields from the saved note without changing its text or ID.
With the list closed, the arrow keys step through the entries; with it open,
**Enter** chooses and **Escape** closes it without a change. Each change is
saved after the usual 800 ms autosave pause, so stepping through the list does
not rewrite the file at every step.

NoteIQ attaches a meeting automatically only when exactly one timed meeting is
in progress and **Connect to my current meeting automatically** is on. All-day
events are never attached automatically, and an RSVP is not treated as proof of
attendance: if meetings overlap, the note stays on **No meeting** until you
choose one. Your own choice, including **No meeting**, always wins for that
note; neither **Refresh** nor a later lookup replaces it. Choosing the meeting
that was attached automatically confirms it in the same way. **New note** starts
again with automatic selection.

While the window is open, NoteIQ re-checks the cached list every minute without
contacting WorkIQ, so a note can pick up a meeting that starts while you write:

- An empty note attaches to the meeting in progress, and an empty note that was
  never saved moves on to the next meeting.
- A note with text attaches only to a meeting that started no later than 15
  minutes after you opened the note, so earlier writing is not filed under a
  later meeting.
- Once a note has text or a file, an automatically attached meeting stays, even
  after it ends or another meeting begins.

Nothing is attached automatically while the last lookup has failed. Turning
automatic connection off keeps the meeting that is already attached.

The hint under the list explains the current state:

| Hint | Meaning |
|---|---|
| Checking your calendar… | A lookup is running; cached meetings stay usable and **Refresh** waits |
| Connected automatically · in progress | It is the only meeting in progress; "· ended" once it is over |
| Selected for this note | You chose this meeting |
| No meeting attached to this note | You chose **No meeting** |
| 2 meetings in progress — choose one | Meetings overlap, so none was attached |
| 1 meeting in progress — choose it to attach | It began more than 15 minutes after you started this note |
| No meeting in progress | No timed meeting is happening right now |
| No meetings found. You can keep writing. | The lookup returned no meetings to list |
| Automatic meeting connection is off | The setting is off; choose a meeting yourself |
| Meeting lookup unavailable. WorkIQ comes with Scout. | Scout's WorkIQ executable was not found |
| Couldn't refresh meetings. Showing earlier results. | The lookup failed; earlier results remain |

Other lookup problems, such as needing to sign in to Scout, are explained in
the same place. Save progress and save errors appear in the footer instead.

WorkIQ returns a limited number of results per search, so NoteIQ asks for
meetings happening now, starting in the next hour, and today in one lookup and
merges the results. WorkIQ reports clock times in UTC; NoteIQ converts them to
your Windows time zone.

The meeting list is cached in memory for the life of the tray process. **New
note**, Ctrl+N, and reopening a closed capture reuse it immediately and
re-evaluate which meeting is current. WorkIQ is asked again when NoteIQ starts
and when you press **Refresh**. Opening a capture or changing the automatic
connection setting also asks again when the list is more than five minutes old
or the last lookup failed. A lookup that is already running is reused, meetings
missing from a newer result stay in the list, and a failed refresh leaves the
list usable with an explanation. No calendar cache is written to disk.

When the installed WorkIQ exposes a compatible `fetch` tool, NoteIQ also retrieves
calendar invitee RSVP details. This optional step has a shared 15-second timeout.
If it fails, the note still uses the available retrieval data and unknown statuses;
diagnostics record the failure without names or email addresses.

Markdown files are saved in OneDrive or an Obsidian vault, but that alone does
not guarantee immediate indexing by WorkIQ. Indexing and query availability
depend on the user's Microsoft 365 and WorkIQ configuration.

## Diagnostics

Rolling logs are stored under:

```text
%LOCALAPPDATA%\DevSecNinja\NoteIQ\data\logs\
```

They include operation type, elapsed time, failure class, and a correlation
ID. They intentionally exclude note text, meeting titles, invitee names and emails,
credentials, and raw WorkIQ responses. The app retains recent log files for up
to 14 days. For more detail, start NoteIQ with `RUST_LOG=noteiq_core=debug`
set in the environment.

Meeting searches share a 45-second limit, with up to 15 additional seconds for
optional RSVP details. Each handshake request has its own 15-second limit.

If meeting lookup fails, press **Refresh** next to the meeting list to retry,
confirm Scout is
installed and signed in, and verify network access. If a folder is unavailable
or unwritable, select another destination in Settings; NoteIQ does not silently
switch folders.
