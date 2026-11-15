# foundry-bench

Research harness for [#32](https://github.com/DevSecNinja/note-iq/issues/32): runs
[Foundry Local](https://learn.microsoft.com/azure/foundry-local/what-is-foundry-local) models
**in-process** through the Rust SDK (`foundry-local-sdk`) and scores them on NoteIQ tasks.
Findings: [`docs/research/foundry-local.md`](../../docs/research/foundry-local.md).

It is not part of the app or the root Cargo workspace (it has its own `[workspace]`), so CI never
builds it. It uses **synthetic notes only** (`src/notes.rs`); never point it at a real vault.

## Build (Windows)

```powershell
# The SDK's build script downloads the native runtime from NuGet; this pins the version.
$env:FOUNDRY_LOCAL_RUNTIME_VERSION = '2.4.1'
# Optional: a NuGet v3 feed if nuget.org is not reachable.
# $env:FOUNDRY_LOCAL_NUGET_FEEDS = 'https://<your-feed>/v3/index.json'
cargo build --release
```

On ARM64, the build script's HTTPS client (`ring`) needs `clang` on `PATH` (for example
`winget install LLVM.LLVM`). The app binary does not need it.

## Run

```powershell
.\target\release\foundry-bench.exe catalog                    # list variants: device, size, license
.\target\release\foundry-bench.exe download <variant-id>      # large: check the size first
.\measure.ps1 bench <variant-id> [tidy|summary|followups]     # samples NPU/GPU counters too
$env:NOTEIQ_BENCH_QUERY_PREFIX = "Instruct: Given a search query, retrieve the meeting note that answers it`nQuery: "
.\measure.ps1 search <embedding-variant-id>
.\target\release\foundry-bench.exe remove <variant-id>
```

Models and logs go to `%LOCALAPPDATA%\NoteIQ-bench` (override with `NOTEIQ_BENCH_DIR`).
Execution providers (QNN for the NPU, WebGPU) are registered at startup; set
`NOTEIQ_BENCH_CPU_ONLY=1` to skip that. Other switches: `NOTEIQ_BENCH_LOG=debug|trace` for the
native log, and `NOTEIQ_BENCH_NO_SHUTDOWN=1` to skip `manager.shutdown()`. When the catalog
service is unreachable, the harness registers already downloaded models in the local catalog,
so it also runs offline. `power.ps1 <command>` compares battery discharge while idle and busy
(on battery only).

Each chat task runs at temperature 0. Tidy and summary outputs go through
`noteiq_core::tidy::fact_warnings`. Follow-up extraction uses JSON Schema constrained decoding,
and every item's `quote` must appear verbatim in the note.
