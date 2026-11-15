# Research: other on-device models via Foundry Local (#32)

Tested on 2026-11-15 on a Surface Laptop 7 (Snapdragon X Elite X1E80100, Hexagon NPU, Adreno GPU,
32 GB RAM, Windows 11 build 26300, ARM64), **on battery** with the Balanced power plan.
Harness: [`tools/foundry-bench`](../../tools/foundry-bench) (`foundry-local-sdk` 2.4.1, native
runtime 2.4.1, ONNX Runtime 1.31.0, ORT GenAI 0.19.0). Synthetic notes only; the prompts and
notes match the [#29 investigation](https://github.com/DevSecNinja/note-iq/issues/29#issuecomment-6875868317)
and `crates/noteiq-core/tests/tidy_tests.rs`. Temperature 0 throughout.

## Summary

**Decision: keep the built-in Windows model for Tidy up. Keep this harness to re-evaluate when
newer models ship.**

- **In-process works, with no identity and no server.** An unpackaged exe (no sparse package,
  no signing, no Developer Mode) found the NPU, ran models on it, and used no child process or
  listening port.
- **Search clears the bar. Phi Silica embeddings did not.** Qwen3-Embedding-0.6B (0.5 GB) found
  the right note 5/5 on the #29 set (Phi Silica: 1/5) and 9/10 on a harder 20-note set.
- **Follow-up extraction is promising, but only with Qwen2.5-7B on the NPU.** Every quote was
  verbatim (11/11), it invented no owners or dates, but it missed some items and needs a 7 GB
  download and 7 GB of RAM.
- **Summaries still fail.** Every model invented or overstated facts, and `fact_warnings` caught
  none of it.
- **Tidy up: no reason to switch.** Qwen2.5-7B is faithful but barely rewrites; Phi Silica stays.
- **Operational blockers to solve first:**
  - offline loading needs a workaround;
  - the SDK sends telemetry;
  - GPU models crash at process exit;
  - the SDK's JSON Schema option is broken;
  - ARM64 builds need clang.

## Platform (demonstrated)

| Question                   | Result                                                                                                                                                                                                                                                                            |
| -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Package identity / signing | **Not needed.** `GetCurrentPackageFullName` reported no package, and NPU inference still worked. Phi Silica needs a signed sparse package (#31).                                                                                                                                  |
| In-process                 | Yes. No child process (`foundry-local-host.exe` was not started) and no listening socket.                                                                                                                                                                                         |
| NPU use                    | Confirmed by counters: the Hexagon compute-only adapter peaked at 100–118% (summed engine samples) while the Adreno compute engine stayed idle. The QNN models' own telemetry reports `EP=CPUExecutionProvider`, so the reported EP is not proof either.                          |
| GPU models                 | Run through WebGPU on the Adreno `3d` engine (peak 79–101%).                                                                                                                                                                                                                      |
| Execution providers        | `download_and_register_eps` took 20 s the first time, then ~100 ms per process. It installed one per-user framework package, `Microsoft.WinML.Qualcomm.QNN.EP.Framework.2` 2.2480.53, and downloaded the WebGPU EP (27 MB) into the app data folder. Registration is per process. |
| Runtime size               | ARM64 45.9 MB raw / **15.7 MB zipped**; x64 41.6 / 15.8 MB, vs the current 7–8 MB release ZIPs. The docs say "~20 MB".                                                                                                                                                            |
| x64                        | Cross-built and ran under Prism emulation. It registered QNN and used the NPU, but was 3–6× slower. Native x64 hardware was not tested.                                                                                                                                           |
| Build                      | The SDK's `build.rs` downloads native packages from NuGet only when `FOUNDRY_LOCAL_RUNTIME_VERSION` is set (otherwise it builds without them and fails at run time). On ARM64 its HTTPS client (`ring`) needs **clang**.                                                          |
| Native lookup              | `library_path` config, then `FOUNDRY_LOCAL_LIB_DIR`, then the **compile-time `target\` path**, then the exe folder. A shipped app must set `library_path`.                                                                                                                        |

### Network and privacy (demonstrated)

- **Catalog.** `api.catalog.azureml.ms` is fetched on every start, taking 4–5 s, in the background.
- **Telemetry.** The 1DS telemetry pipeline uploads usage events: model id, token counts, timings,
  CPU/RAM, OS and app name. **No note text** appeared in the debug log of these events.
  - The log says non-essential upload is "disabled via `ORT_DISABLE_TELEMETRY`" whether or not
    that variable is set, and that "ProcessInfo still uploads".
  - NoteIQ would need to disclose this or find a supported opt-out.
- **Offline.** With outbound traffic blocked by a firewall rule for the exe, downloaded models
  **could not be found** through the catalog API: `get_model_variant` and `get_cached_models`
  returned nothing.
  - Registering the downloaded folder in the local catalog (`register_model`, "bring your own
    model") worked, and the registration persisted across runs. Inference then matched the
    online results.
  - NoteIQ would have to save the catalog metadata at download time and register models itself.
- **Exit crash.** Every run that loaded a **WebGPU (GPU) model** (20/20) aborted at process exit
  (`ucrtbase.dll`, `0xC0000409`) after all work had finished. None of the 12 runs that used only
  the NPU or CPU did. `manager.shutdown()` did not prevent it.

### SDK defect

`ChatClient::response_format(JsonSchema(..))` sends the schema as a JSON _string_. The native
engine rejects even `{"type":"object"}` with "schema must be an object or boolean". The harness
works around it by sending the OpenAI request through `Session` with `TextKind::OpenAiJson` and
the schema as an object. JSON Schema defaults to allowing extra properties, so the schema also
needs `additionalProperties: false` and `maxItems`; otherwise small models emit invented keys.

## Models tested

| Variant                            | Device | Download | License (catalog) | Load        | Peak RAM (process) |
| ---------------------------------- | ------ | -------- | ----------------- | ----------- | ------------------ |
| qwen2.5-1.5b-instruct-qnn-npu:3    | NPU    | 1.1 GB   | MIT*              | 5.2–6.8 s   | 1.6 GB             |
| phi-3.5-mini-instruct-qnn-npu:3    | NPU    | 2.1 GB   | MIT               | 4.5 s       | 3.0 GB             |
| qwen2.5-7b-instruct-qnn-npu:3      | NPU    | 7.0 GB   | MIT*              | 10.0–10.7 s | **7.3 GB**         |
| Phi-4-mini-instruct-generic-gpu:5  | GPU    | 3.8 GB   | MIT               | 9.5 s       | 4.8 GB             |
| qwen3-embedding-0.6b-generic-gpu:1 | GPU    | 0.5 GB   | Apache-2.0        | 2.8–2.9 s   | 1.1 GB             |
| qwen3-embedding-0.6b-generic-cpu:1 | CPU    | 0.5 GB   | Apache-2.0        | 2.4 s       | 1.5 GB             |

\* The catalog lists the Qwen2.5 QNN builds as MIT, while the upstream Qwen2.5 1.5B/7B models
and the catalog's own GPU/CPU builds are Apache-2.0. Both are permissive, but the mismatch should
be clarified before NoteIQ offers these models.

The catalog has **no NPU build of Phi-4-mini or of any embedding model**. Phi Silica, by
comparison, has no download and runs out of process (24 MB in the app).

## Results

### Tidy up (short notes, 3 runs each; output was identical across runs)

| Model                                   | Latency (short note)                     | `fact_warnings`                                       | Manual review                                                                               |
| --------------------------------------- | ---------------------------------------- | ----------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| Phi Silica `TextRewriter` Concise (#29) | 4.6–5.6 s                                | `??` and "me" lost, `$` added                         | Faithful                                                                                    |
| Qwen2.5-1.5B NPU                        | 3.0–3.8 s                                | negation added, `??` lost                             | **Invented** "scheduled for December"; long note looped ("50 users, 50 users, …")           |
| Phi-3.5-mini NPU                        | 9–13 s                                   | `NegationRemoved`, `$` added                          | Appends "Note: The above bullet points…"; marks Priya's concern "(tentative)" without cause |
| Qwen2.5-7B NPU                          | 7.5–8.8 s (10–13 s later in the session) | only false positives ("2nd"→"second", "q"→"Question") | **Faithful**; kept `??`→"(tentative)", "me", NOT, 40k vs 400k. Barely rewrites              |
| Phi-4-mini GPU                          | 4.4–6.3 s                                | `$` added (budget)                                    | Reads best; kept `??` and "Me"                                                              |

First token was 0.2–2.5 s depending on the model. No model clearly beats Phi Silica for this job.

### Summary (long note, ≤4 bullets)

| Model        | Problems found by manual review                                                                                   | `fact_warnings` (additions only) |
| ------------ | ----------------------------------------------------------------------------------------------------------------- | -------------------------------- |
| Qwen2.5-1.5B | "The meeting was rescheduled" (invented)                                                                          | 0                                |
| Phi-3.5-mini | 6 bullets, "[Your Name]" placeholder                                                                              | 4 (labels only)                  |
| Qwen2.5-7B   | States the January rollout as decided (it was Sam's preference); omits the on-device requirement                  | 0                                |
| Phi-4-mini   | "led to a decision to keep meeting notes on device only" and "Sam Lee received a pilot checklist" (both invented) | 0                                |

The deterministic guard compares numbers, names, negations and symbols, so it **cannot catch
invented claims written with existing words**. Summaries stay rejected.

### Follow-ups (JSON Schema constrained; `quote` must be verbatim in the note)

| Model        | Quotes grounded           | Problems                                                                                                                                                                                    |
| ------------ | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Qwen2.5-1.5B | invalid JSON on 2/3 notes | Invented due dates (`2022-12-05`), repeated items to the token limit                                                                                                                        |
| Phi-3.5-mini | 8/13                      | Invented due dates ("As soon as possible…"), wrong owner                                                                                                                                    |
| Qwen2.5-7B   | **11/11**                 | No invented owners or dates (left null). Missed "by fri" / "by tue" due dates and Alex's tentative beta date; one open question reworded with an inference ("When will Dana be available…") |
| Phi-4-mini   | 10/11                     | Ravi's task given to "me"; one item pairs Alex's quote with the author's task                                                                                                               |

Latency for Qwen2.5-7B: 14–24 s per note.

### Search (Qwen3-Embedding-0.6B, query instruction prefix as documented by Qwen)

| Set                                         | Phi Silica (#29) | GPU                   | CPU            | Without query prefix (GPU / CPU) |
| ------------------------------------------- | ---------------- | --------------------- | -------------- | -------------------------------- |
| #29 set (6 notes, 5 queries)                | 1/5 top-1        | **5/5**               | **5/5**        | 2/5 / 2/5                        |
| Hard set (20 notes, 10 paraphrased queries) | –                | **9/10** (top-3 9/10) | **9/10**       | – / 8/10                         |
| Per query                                   | ~140 ms          | ~155 ms               | 0.6–1.0 s      |                                  |
| Indexing                                    |                  | ~160 ms/note          | 0.7–1.3 s/note |                                  |

The only miss is "app takes too long to open", which ranked the battery-drain note above the
cold-start note. The query prefix matters: without it, the #29 set dropped to 2/5.

### Power

Not reliably measured. Whole-system battery discharge was 22–25 W during runs, but idle varied
20–28 W between runs because of background activity (`tools/foundry-bench/power.ps1`).

## Decision

**Keep the built-in Windows model (Phi Silica) for Tidy up and add no Foundry Local models for
now.** Keep `tools/foundry-bench` and re-run it in a few months, when newer models are available:
Aion Instruct replaces Phi Silica on retail PCs in January 2027, and the Foundry Local catalog
changes often.

No Foundry Local feature is ready to ship today:

- Search is the one capability that clears the quality bar.
- Follow-up extraction only works with a 7 GB model.
- Summaries fail with every model.
- The operational issues below would all need solving first.

## When we re-evaluate

1. **Semantic search: the strongest candidate.** It only reads notes and never changes them, so
   it doesn't need the review → Apply → Undo flow.
   - Model: Qwen3-Embedding-0.6B (0.5 GB, Apache-2.0).
   - Prefer the **CPU variant** unless the WebGPU exit crash is fixed. It works on any Windows PC
     and costs ~1 s per query, versus 155 ms on the GPU.
   - Show which note matched and why (its title and the matching passage).
   - Keep embeddings in a local index next to the settings, never in the vault.
2. **Follow-up suggestions.** Look for a smaller model that grounds every item as well as
   Qwen2.5-7B did, and evaluate on a larger labelled set.
   - Any version must drop items whose quote is not verbatim and show each suggestion with its
     quote.
   - The user ticks which suggestions go into `## Follow-ups`, then Apply / Undo.
   - Record the model id in front matter as provenance.
3. **Tidy up.** Compare Aion Instruct against these results. A Foundry Local model (for example
   Phi-4-mini on CPU/GPU) could bring Tidy up to PCs without the built-in model; that is untested
   on non-Copilot+ hardware.
4. **Summaries.** Only reconsider with a model that passes manual review, because
   `fact_warnings` cannot catch invented claims.

### Prerequisites before any product work

- **Offline:** save catalog metadata at download time and register models in the local catalog,
  or get an SDK fix so cached models load without the catalog service.
- **Telemetry:** decide how to disclose 1DS telemetry, or find a supported opt-out; confirm the
  `ORT_DISABLE_TELEMETRY` behaviour with the Foundry Local team.
- **Report upstream:** the JSON Schema string bug, the WebGPU exit abort, and the QNN license
  metadata.
- **Build:** set `FOUNDRY_LOCAL_RUNTIME_VERSION` in CI and make clang available on the
  `windows-11-arm` runner. Ship the native DLLs (+15.7 MB zipped) beside `noteiq.exe` and set
  `library_path`.
- **Settings model picker** (from #32):
  - show download size, device, license and quality notes;
  - downloads need explicit consent, show progress and can be cancelled;
  - models can be removed;
  - the built-in model stays the default, and there is no cloud fallback.
- **Memory:** consider running models in a helper process so a 1–7 GB working set and native
  aborts can't take down the editor. That process would not be a model server: no port and no
  other clients.

## Reproduce

See [`tools/foundry-bench/README.md`](../../tools/foundry-bench/README.md). To re-evaluate:

1. Run `catalog` to see new variants.
2. Download candidates after checking their size and license.
3. Run `measure.ps1 bench <id>` and `measure.ps1 search <id>`, then compare with the tables above.
   Review the outputs by hand; `fact_warnings` and quote grounding are only first-pass checks.

Raw outputs from this run are kept with the investigation session, not in the repository.
