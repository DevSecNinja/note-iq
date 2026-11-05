# Quickstart: NoteIQ Quick Capture

## Prerequisites

- Rustup stable and Cargo.
- Windows 11 x64 or ARM64 with the matching MSVC linker and Windows SDK for a
  native desktop build.
- Linux x64 or ARM64 (including WSL2 Debian 13 ARM64) for
  platform-independent test runs.
- Scout installed with its bundled WorkIQ executable for optional meeting
  discovery. NoteIQ prefers the native `win-x64`/`win-arm64` payload, falls
  back to the other architecture, and uses a configured executable path first.
  No separate WorkIQ install is required or supported.

## Build and test the core in WSL

From the repository worktree mounted in WSL:

```bash
cargo fmt --all -- --check
cargo test --workspace
```

Expected: formatting passes and all unit/integration tests pass without network
access to Microsoft 365 or a WorkIQ executable.

## Build the Windows application

From PowerShell on Windows x64:

```powershell
cargo build --release --features desktop -p noteiq-windows --target x86_64-pc-windows-msvc
```

For Windows ARM64, use the matching target:

```powershell
cargo build --release --features desktop -p noteiq-windows --target aarch64-pc-windows-msvc
```

Expected: a native `noteiq.exe` under the requested target's
`target/<target-triple>/release/` directory.

## Release artifact checks

The release pipeline produces x64 and ARM64 ZIPs from one tagged
commit, plus `SHA256SUMS.txt` and GitHub attestations. Compare a downloaded
archive's `Get-FileHash -Algorithm SHA256` output with its entry in the
checksum manifest, then verify its attestation:

```powershell
$repo = gh repo view --json nameWithOwner --jq .nameWithOwner
gh attestation verify .\noteiq-v0.1.0-windows-x64.zip --repo $repo
```

Release verification and the required Release Please app/repository setup are
described in `docs/setup.md`. To retry a draft release, dispatch on its tag so
the artifact provenance identifies the tag as the source:

```powershell
gh workflow run release.yml --ref v0.1.0 -f tag=v0.1.0
```

Local x64-under-emulation, ARM64, and WSL validation passed. PR #5 also passed
hosted Linux x64/ARM64 checks, Windows x64/ARM64 tests and both artifact builds.
Release publication remains unexercised. Checksums and attestations are not code
signing; SmartScreen may still warn because Authenticode signing is not
configured.

## Verify the capture lifecycle

1. Start `noteiq` and type a short note.
2. Wait for idle autosave; verify the Markdown file in the selected destination.
3. Type continuously for longer than the interval; verify saved content advances
   without closing the editor.
4. Press Ctrl+S; verify the editor stays open and reports Saved.
5. Erase all text; verify the file remains while editing.
6. Close the capture with the window's title-bar × (or Alt+F4); verify the empty
   note is deleted.
7. Type a note, choose Open note; verify the exact file opens.

## Verify meeting behavior

Use the WorkIQ fixture tests to exercise zero, one, and overlapping meetings.
In particular, save a note with meeting context, choose **No meeting** from the
Meeting list, save again, then verify the file contains the same body and no
meeting or attendee metadata. Choosing the meeting again restores its metadata.

With an empty note open, a meeting that starts while the window is open is
attached within a minute. A meeting chosen by hand, including No meeting, is
kept for that note until New note.

Run an optional live integration check only when the user explicitly accepts the
tenant authentication prompt. Do not collect or record actual calendar content
in test logs.

## Troubleshooting

If Scout or its WorkIQ executable is unavailable, NoteIQ must still open and
save notes, and the meeting hint explains the missing dependency. Review the
rolling diagnostic logs for sanitized failure
codes. See `docs/setup.md` for install, shortcut, Command Palette, and
Markdown/WorkIQ retrieval details.
