//! Benchmarks Foundry Local models in-process for NoteIQ (#32). Synthetic notes only.
#![allow(deprecated)] // The OpenAI-style chat client is the SDK's documented path for JSON Schema output.

mod notes;

use anyhow::{Context, Result, bail};
use foundry_local_sdk::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestUserMessage, EmbeddingsSession, FoundryLocalConfig, FoundryLocalManager,
    Item, LogLevel, Model, Request, Session, TextKind,
};
use noteiq_core::tidy::{FactWarning, fact_warnings};
use std::sync::Arc;
use std::time::Instant;
use tokio_stream::StreamExt;

const USAGE: &str = "usage: foundry-bench <command>
  identity               show whether this process has package identity
  eps [--register]       list execution providers (optionally download/register them)
  catalog                list catalog variants with device, size, and license
  download <variant-id>  download one variant (ask before running: models are large)
  bench <variant-id> [tidy|summary|followups]...
  search <variant-id>    embedding search accuracy (embedding models only)
  remove <variant-id>    delete a variant from the cache

Env: NOTEIQ_BENCH_DIR overrides the data/cache directory (default %LOCALAPPDATA%\\NoteIQ-bench).
     NOTEIQ_BENCH_CPU_ONLY=1 skips execution provider registration.";

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        println!("{USAGE}");
        return Ok(());
    };
    if command == "identity" {
        println!(
            "package identity: {}",
            package_identity().unwrap_or_else(|| "none".into())
        );
        return Ok(());
    }

    let start = Instant::now();
    let manager = manager()?;
    println!("[time] manager create: {} ms", start.elapsed().as_millis());
    println!(
        "[identity] {}",
        package_identity().unwrap_or_else(|| "none (unpackaged)".into())
    );
    // EP registration is per process; an app would do this once at startup.
    if command != "eps" && std::env::var_os("NOTEIQ_BENCH_CPU_ONLY").is_none() {
        let start = Instant::now();
        let result = manager.download_and_register_eps(None).await?;
        println!(
            "[time] register EPs {:?}: {} ms",
            result.registered_eps,
            start.elapsed().as_millis()
        );
    }

    match (command, args.get(1).map(String::as_str)) {
        ("eps", flag) => eps(&manager, flag == Some("--register")).await?,
        ("catalog", _) => catalog(&manager).await?,
        ("download", Some(id)) => download(&*variant(&manager, id).await?).await?,
        ("bench", Some(id)) => bench(&*variant(&manager, id).await?, &args[2..]).await?,
        ("search", Some(id)) => search(&*variant(&manager, id).await?).await?,
        ("remove", Some(id)) => {
            variant(&manager, id).await?.remove_from_cache().await?;
            println!("removed {id}");
        }
        _ => println!("{USAGE}"),
    }
    println!("[memory] peak working set {} MB", peak_working_set_mb());
    // Without an explicit shutdown the native engine sometimes aborts at process exit
    // (ucrtbase 0xC0000409) after all work has finished.
    if std::env::var_os("NOTEIQ_BENCH_NO_SHUTDOWN").is_none() {
        manager.shutdown()?;
    }
    Ok(())
}

fn manager() -> Result<Arc<FoundryLocalManager>> {
    let dir = std::env::var("NOTEIQ_BENCH_DIR").unwrap_or_else(|_| {
        format!(
            "{}\\NoteIQ-bench",
            std::env::var("LOCALAPPDATA").unwrap_or_default()
        )
    });
    let level = match std::env::var("NOTEIQ_BENCH_LOG").as_deref() {
        Ok("trace") => LogLevel::Trace,
        Ok("debug") => LogLevel::Debug,
        Ok("info") => LogLevel::Info,
        _ => LogLevel::Warn,
    };
    let config = FoundryLocalConfig::new("noteiq-bench")
        .app_data_dir(&dir)
        .model_cache_dir(format!("{dir}\\models"))
        .logs_dir(format!("{dir}\\logs"))
        .log_level(level);
    // Points the catalog at an unreachable URL to simulate running offline.
    let config = match std::env::var("NOTEIQ_BENCH_CATALOG_URL") {
        Ok(url) => config.catalog_url(url),
        Err(_) => config,
    };
    Ok(FoundryLocalManager::create(config)?)
}

async fn variant(manager: &FoundryLocalManager, id: &str) -> Result<Arc<Model>> {
    match manager.catalog().get_model_variant(id).await {
        Ok(model) => Ok(model),
        Err(online) => {
            // Offline: fall back to models already on disk.
            println!("[offline] catalog lookup failed ({online}); trying cached and local models");
            let local = manager.local_catalog().ok();
            for catalog in [Some(manager.catalog()), local.as_ref()]
                .into_iter()
                .flatten()
            {
                match catalog.get_cached_models().await {
                    Ok(models) => {
                        println!(
                            "[offline] {} catalog: {} cached",
                            catalog.name(),
                            models.len()
                        );
                        if let Some(model) = models.into_iter().find(|m| m.id() == id) {
                            return Ok(model);
                        }
                    }
                    Err(error) => println!("[offline] {} catalog: {error}", catalog.name()),
                }
            }
            if let Some(model) = register_downloaded(manager, id).await? {
                return Ok(model);
            }
            Err(online).with_context(|| format!("no catalog variant with id {id}; run `catalog`"))
        }
    }
}

/// Offline route: register an already downloaded model folder in the local catalog (BYOM).
/// An app would store the catalog metadata at download time; here it is derived from the id.
async fn register_downloaded(
    manager: &FoundryLocalManager,
    id: &str,
) -> Result<Option<Arc<Model>>> {
    let dir = std::env::var("NOTEIQ_BENCH_DIR").unwrap_or_else(|_| {
        format!(
            "{}\\NoteIQ-bench",
            std::env::var("LOCALAPPDATA").unwrap_or_default()
        )
    });
    let (name, version) = id.rsplit_once(':').unwrap_or((id, "1"));
    let path = format!("{dir}\\models\\Microsoft\\{name}-{version}\\v{version}");
    if !std::path::Path::new(&path)
        .join("genai_config.json")
        .is_file()
    {
        return Ok(None);
    }
    let (device, provider) = if id.contains("qnn-npu") {
        ("NPU", "QNNExecutionProvider")
    } else if id.contains("generic-gpu") {
        ("GPU", "WebGPUExecutionProvider")
    } else {
        ("CPU", "CPUExecutionProvider")
    };
    let task = if id.contains("embedding") {
        "embeddings"
    } else {
        "chat-completion"
    };
    let local = manager.local_catalog()?;
    let mut metadata = local.create_model_info()?;
    metadata
        .set_string_property("task", task)?
        .set_string_property("device_type", device)?
        .set_string_property("execution_provider", provider)?;
    let start = Instant::now();
    let model = local.register_model(&path, id, metadata).await?;
    println!(
        "[offline] registered {path} as {id} ({} ms)",
        start.elapsed().as_millis()
    );
    Ok(Some(model))
}

async fn eps(manager: &FoundryLocalManager, register: bool) -> Result<()> {
    for ep in manager.discover_eps()? {
        println!("{:40} registered={}", ep.name, ep.is_registered);
    }
    if register {
        let start = Instant::now();
        let result = manager.download_and_register_eps(None).await?;
        println!(
            "[eps] success={} status={} registered={:?} failed={:?} ({} ms)",
            result.success,
            result.status,
            result.registered_eps,
            result.failed_eps,
            start.elapsed().as_millis()
        );
    }
    Ok(())
}

async fn catalog(manager: &FoundryLocalManager) -> Result<()> {
    println!("id\tdevice\tep\tMB\ttask\tlicense\tcached\ttools\tctx");
    for model in manager.catalog().get_models().await? {
        for variant in model.variants() {
            let info = variant.info()?;
            let (device, ep) = info
                .runtime
                .as_ref()
                .map(|r| (format!("{:?}", r.device_type), r.execution_provider.clone()))
                .unwrap_or_default();
            println!(
                "{}\t{device}\t{ep}\t{}\t{}\t{}\t{}\t{}\t{}",
                info.id,
                info.file_size_mb
                    .map(|mb| mb.to_string())
                    .unwrap_or_default(),
                info.task.unwrap_or_default(),
                info.license.unwrap_or_default(),
                info.cached,
                info.supports_tool_calling.unwrap_or(false),
                variant
                    .context_length()
                    .map(|c| c.to_string())
                    .unwrap_or_default(),
            );
        }
    }
    Ok(())
}

async fn download(model: &Model) -> Result<()> {
    let start = Instant::now();
    let mut last = -10.0;
    model
        .download(Some(move |percent: f64| {
            if percent - last >= 10.0 || percent >= 100.0 {
                println!("  {percent:5.1}%");
                last = percent;
            }
        }))
        .await?;
    println!(
        "[time] download {}: {} s -> {}",
        model.id(),
        start.elapsed().as_secs(),
        model.path().await?.display()
    );
    Ok(())
}

async fn load(model: &Model) -> Result<()> {
    if !model.is_cached().await? {
        bail!(
            "{} is not downloaded; run `download {}` first",
            model.id(),
            model.id()
        );
    }
    let start = Instant::now();
    model.load().await?;
    println!(
        "[time] load {}: {} ms",
        model.id(),
        start.elapsed().as_millis()
    );
    Ok(())
}

async fn bench(model: &Model, tasks: &[String]) -> Result<()> {
    let wanted = |task: &str| tasks.is_empty() || tasks.iter().any(|t| t == task);
    load(model).await?;

    if wanted("tidy") {
        for (name, note) in [
            ("rough", notes::ROUGH),
            ("budget", notes::BUDGET),
            ("long", notes::LONG),
        ] {
            // The long note exceeds Tidy up's 700-character limit; one run is enough as a stress test.
            let runs = if name == "long" { 1 } else { 3 };
            for run in 1..=runs {
                let out = chat(
                    model,
                    notes::TIDY_SYSTEM,
                    note,
                    &format!("tidy {name} #{run}"),
                )
                .await?;
                if run == 1 {
                    println!("{out}");
                }
                report_warnings(&fact_warnings(note, &out), false);
            }
            println!();
        }
    }

    if wanted("summary") {
        for run in 1..=2 {
            let out = chat(
                model,
                notes::SUMMARY_SYSTEM,
                notes::LONG,
                &format!("summary long #{run}"),
            )
            .await?;
            println!("{out}");
            // A summary drops detail by design, so only additions count as warnings.
            report_warnings(&fact_warnings(notes::LONG, &out), true);
            println!();
        }
    }

    if wanted("followups") {
        for (name, note) in [
            ("rough", notes::ROUGH),
            ("budget", notes::BUDGET),
            ("long", notes::LONG),
        ] {
            let out = chat_json(
                model,
                notes::FOLLOWUPS_SYSTEM,
                note,
                &format!("followups {name}"),
            )
            .await?;
            println!("{out}");
            grounding(note, &out);
            println!();
        }
    }

    model.unload().await?;
    Ok(())
}

async fn chat(model: &Model, system: &str, note: &str, label: &str) -> Result<String> {
    let client = model.create_chat_client().temperature(0.0).max_tokens(1024);
    let messages: Vec<ChatCompletionRequestMessage> = vec![
        ChatCompletionRequestSystemMessage::from(system).into(),
        ChatCompletionRequestUserMessage::from(format!("NOTE:\n{note}")).into(),
    ];
    let start = Instant::now();
    let mut stream = client.complete_streaming_chat(&messages, None).await?;
    let mut first_token = None;
    let mut out = String::new();
    while let Some(chunk) = stream.next().await {
        for choice in chunk?.choices {
            if let Some(text) = choice.delta.content {
                first_token.get_or_insert_with(|| start.elapsed().as_millis());
                out.push_str(&text);
            }
        }
    }
    let total = start.elapsed();
    let tokens_out = out.split_whitespace().count();
    println!(
        "[time] {label}: first token {} ms, total {} ms, {} words",
        first_token.unwrap_or(0),
        total.as_millis(),
        tokens_out
    );
    Ok(out.trim().to_owned())
}

/// Schema-constrained chat. SDK 2.4.1's `ChatClient` sends `json_schema` as a JSON string,
/// which the native engine rejects ("schema must be an object or boolean"), so this sends
/// the OpenAI request through a raw session with the schema as an object.
async fn chat_json(model: &Model, system: &str, note: &str, label: &str) -> Result<String> {
    let schema: serde_json::Value = serde_json::from_str(notes::FOLLOWUPS_SCHEMA)?;
    let request = serde_json::json!({
        "model": model.id(),
        "temperature": 0.0,
        "max_tokens": 1024,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": format!("NOTE:\n{note}")},
        ],
        "response_format": {"type": "json_schema", "json_schema": schema},
    });
    let session = Session::new(model).await?;
    let start = Instant::now();
    let response = session
        .process_request(Request::from_items(vec![Item::Text {
            text: request.to_string(),
            kind: TextKind::OpenAiJson,
        }]))
        .await?;
    let raw = response.text();
    let parsed: serde_json::Value = serde_json::from_str(&raw).context("response is not JSON")?;
    let content = parsed["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    println!(
        "[time] {label}: total {} ms, {} completion tokens",
        start.elapsed().as_millis(),
        parsed["usage"]["completion_tokens"]
    );
    Ok(content.trim().to_owned())
}

fn report_warnings(warnings: &[FactWarning], additions_only: bool) {
    let shown: Vec<String> = warnings
        .iter()
        .filter(|w| {
            !additions_only
                || matches!(
                    w,
                    FactWarning::NumberAdded(_)
                        | FactWarning::NameAdded(_)
                        | FactWarning::NegationAdded
                        | FactWarning::SymbolAdded(_)
                )
        })
        .map(|w| format!("{w:?}"))
        .collect();
    println!("[fact_warnings] {} {:?}", shown.len(), shown);
}

/// Every `quote` must appear verbatim (ignoring case and spacing) in the note.
fn grounding(note: &str, json: &str) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        println!("[grounding] invalid JSON");
        return;
    };
    let hay = normalize(note);
    let (mut grounded, mut total) = (0, 0);
    for key in ["action_items", "decisions", "open_questions"] {
        for item in value[key].as_array().into_iter().flatten() {
            let quote = item["quote"].as_str().unwrap_or_default();
            total += 1;
            let ok = !quote.trim().is_empty() && hay.contains(&normalize(quote));
            grounded += usize::from(ok);
            if !ok {
                println!("  ungrounded {key}: {quote:?}");
            }
        }
    }
    println!("[grounding] quotes found in note: {grounded}/{total}");
}

fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

async fn search(model: &Model) -> Result<()> {
    load(model).await?;
    let session = EmbeddingsSession::new(model).await?;
    // Qwen3-Embedding is trained with an instruction prefix on queries only.
    let prefix = std::env::var("NOTEIQ_BENCH_QUERY_PREFIX").unwrap_or_default();
    if !prefix.is_empty() {
        println!("[search] query prefix: {prefix:?}");
    }
    rank(
        &session,
        "#29 set",
        &notes::CORPUS,
        &notes::QUERIES,
        &prefix,
    )
    .await?;
    rank(
        &session,
        "hard set",
        &notes::HARD_CORPUS,
        &notes::HARD_QUERIES,
        &prefix,
    )
    .await?;
    drop(session);
    model.unload().await?;
    Ok(())
}

async fn rank(
    session: &EmbeddingsSession,
    label: &str,
    corpus: &[&str],
    queries: &[(&str, usize)],
    prefix: &str,
) -> Result<()> {
    let start = Instant::now();
    let docs = session
        .embed_batch(corpus.iter().map(|d| d.to_string()).collect())
        .await?;
    println!(
        "[time] {label}: embed {} notes: {} ms (dim {})",
        docs.len(),
        start.elapsed().as_millis(),
        docs.first().map_or(0, Vec::len)
    );
    let (mut top1, mut top3) = (0, 0);
    for &(query, expected) in queries {
        let start = Instant::now();
        let q = session.embed(format!("{prefix}{query}")).await?;
        let ms = start.elapsed().as_millis();
        let mut ranked: Vec<(f32, usize)> = docs
            .iter()
            .enumerate()
            .map(|(i, d)| (cosine(&q, d), i))
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        top1 += usize::from(ranked[0].1 == expected);
        top3 += usize::from(ranked.iter().take(3).any(|r| r.1 == expected));
        println!(
            "[search] {query:45} -> {} ({:.3}) expected {expected} | 2nd {} ({:.3}) [{ms} ms]",
            ranked[0].1, ranked[0].0, ranked[1].1, ranked[1].0
        );
    }
    println!(
        "[search] {label}: top-1 {top1}/{n}, top-3 {top3}/{n}",
        n = queries.len()
    );
    Ok(())
}
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm(a) * norm(b))
}

fn package_identity() -> Option<String> {
    use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
    let mut len = 0u32;
    // SAFETY: querying the required length with a null buffer is the documented pattern.
    let rc = unsafe { GetCurrentPackageFullName(&mut len, std::ptr::null_mut()) };
    if rc == 15700 {
        return None; // APPMODEL_ERROR_NO_PACKAGE
    }
    let mut buf = vec![0u16; len as usize];
    // SAFETY: buf has the length the previous call asked for.
    unsafe { GetCurrentPackageFullName(&mut len, buf.as_mut_ptr()) };
    Some(String::from_utf16_lossy(
        &buf[..len.saturating_sub(1) as usize],
    ))
}

fn peak_working_set_mb() -> u64 {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    counters.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    // SAFETY: counters is a correctly sized, writable struct for the current process.
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    if ok == 0 {
        return 0;
    }
    (counters.PeakWorkingSetSize / (1024 * 1024)) as u64
}
