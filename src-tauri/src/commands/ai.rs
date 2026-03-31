use crate::core::ai_provider;
use serde::Serialize;
use tauri::Emitter;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiStreamPayload {
    request_id: String,
    event: String,
    delta: Option<String>,
    message: Option<String>,
}

fn emit_ai_stream_event(
    window: &tauri::Window,
    channel: &str,
    request_id: &str,
    event: &str,
    delta: Option<String>,
    message: Option<String>,
) -> Result<(), String> {
    let payload = AiStreamPayload {
        request_id: request_id.to_string(),
        event: event.to_string(),
        delta,
        message,
    };

    window
        .emit(channel, payload)
        .map_err(|e| format!("Failed to emit {} event: {}", channel, e))
}

fn emit_translate_stream_event(
    window: &tauri::Window,
    request_id: &str,
    event: &str,
    delta: Option<String>,
    message: Option<String>,
) -> Result<(), String> {
    emit_ai_stream_event(
        window,
        "ai://translate-stream",
        request_id,
        event,
        delta,
        message,
    )
}

fn emit_summarize_stream_event(
    window: &tauri::Window,
    request_id: &str,
    event: &str,
    delta: Option<String>,
    message: Option<String>,
) -> Result<(), String> {
    emit_ai_stream_event(
        window,
        "ai://summarize-stream",
        request_id,
        event,
        delta,
        message,
    )
}

#[tauri::command]
pub async fn get_ai_config() -> Result<ai_provider::AiConfig, String> {
    Ok(ai_provider::load_config())
}

#[tauri::command]
pub async fn save_ai_config(config: ai_provider::AiConfig) -> Result<(), String> {
    ai_provider::save_config(&config).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_translate_skill(content: String) -> Result<String, String> {
    let config = ai_provider::load_config();
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    if config.api_key.trim().is_empty() {
        return Err(
            "AI provider is not configured. Please set up your API key in Settings.".to_string(),
        );
    }
    ai_provider::translate_text(&config, &content)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_translate_skill_stream(
    window: tauri::Window,
    request_id: String,
    content: String,
) -> Result<String, String> {
    let config = ai_provider::load_config();
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    if config.api_key.trim().is_empty() {
        return Err(
            "AI provider is not configured. Please set up your API key in Settings.".to_string(),
        );
    }

    let _ = emit_translate_stream_event(&window, &request_id, "start", None, None);

    let mut on_delta = |delta: &str| -> anyhow::Result<()> {
        emit_translate_stream_event(&window, &request_id, "delta", Some(delta.to_string()), None)
            .map_err(anyhow::Error::msg)
    };

    match ai_provider::translate_text_streaming(&config, &content, &mut on_delta).await {
        Ok(result) => {
            let _ = emit_translate_stream_event(&window, &request_id, "complete", None, None);
            Ok(result)
        }
        Err(err) => {
            let message = err.to_string();
            let _ = emit_translate_stream_event(
                &window,
                &request_id,
                "error",
                None,
                Some(message.clone()),
            );
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn ai_translate_short_text_stream(
    window: tauri::Window,
    request_id: String,
    content: String,
) -> Result<String, String> {
    let config = ai_provider::load_config();
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    if config.api_key.trim().is_empty() {
        return Err(
            "AI provider is not configured. Please set up your API key in Settings.".to_string(),
        );
    }

    let _ = emit_translate_stream_event(&window, &request_id, "start", None, None);

    let mut on_delta = |delta: &str| -> anyhow::Result<()> {
        emit_translate_stream_event(&window, &request_id, "delta", Some(delta.to_string()), None)
            .map_err(anyhow::Error::msg)
    };

    match ai_provider::translate_short_text_streaming(&config, &content, &mut on_delta).await {
        Ok(result) => {
            let _ = emit_translate_stream_event(&window, &request_id, "complete", None, None);
            Ok(result)
        }
        Err(err) => {
            let message = err.to_string();
            let _ = emit_translate_stream_event(
                &window,
                &request_id,
                "error",
                None,
                Some(message.clone()),
            );
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn ai_summarize_skill(content: String) -> Result<String, String> {
    let config = ai_provider::load_config();
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    if config.api_key.trim().is_empty() {
        return Err(
            "AI provider is not configured. Please set up your API key in Settings.".to_string(),
        );
    }
    ai_provider::summarize_text(&config, &content)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_summarize_skill_stream(
    window: tauri::Window,
    request_id: String,
    content: String,
) -> Result<String, String> {
    let config = ai_provider::load_config();
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    if config.api_key.trim().is_empty() {
        return Err(
            "AI provider is not configured. Please set up your API key in Settings.".to_string(),
        );
    }

    let _ = emit_summarize_stream_event(&window, &request_id, "start", None, None);

    let mut on_delta = |delta: &str| -> anyhow::Result<()> {
        emit_summarize_stream_event(&window, &request_id, "delta", Some(delta.to_string()), None)
            .map_err(anyhow::Error::msg)
    };

    match ai_provider::summarize_text_streaming(&config, &content, &mut on_delta).await {
        Ok(result) => {
            let _ = emit_summarize_stream_event(&window, &request_id, "complete", None, None);
            Ok(result)
        }
        Err(err) => {
            let message = err.to_string();
            let _ = emit_summarize_stream_event(
                &window,
                &request_id,
                "error",
                None,
                Some(message.clone()),
            );
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn ai_test_connection() -> Result<String, String> {
    let config = ai_provider::load_config();
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    if config.api_key.trim().is_empty() {
        return Err("API key is empty".to_string());
    }
    ai_provider::test_connection(&config)
        .await
        .map_err(|e| e.to_string())
}

#[derive(serde::Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
}

#[tauri::command]
pub async fn ai_pick_skills(prompt: String, skills: Vec<SkillMeta>) -> Result<Vec<String>, String> {
    let config = ai_provider::load_config();
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    if config.api_key.trim().is_empty() {
        return Err(
            "AI provider is not configured. Please set up your API key in Settings.".to_string(),
        );
    }

    // Build a YAML-like catalog from skill metadata
    let catalog = skills
        .iter()
        .map(|s| format!("- name: {}\n  description: {}", s.name, s.description))
        .collect::<Vec<_>>()
        .join("\n");

    ai_provider::pick_skills(&config, &prompt, &catalog)
        .await
        .map_err(|e| e.to_string())
}

// ── Security Scan Commands ──────────────────────────────────────────

use crate::core::security_scan::{
    self, FileRole, FileScanResult, PreparedChunk, PreparedSkillScan, SecurityScanResult,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::Semaphore as TokioSemaphore;

pub static CANCEL_SCAN: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub async fn cancel_security_scan() -> Result<(), String> {
    CANCEL_SCAN.store(true, Ordering::Relaxed);
    Ok(())
}

use crate::core::security_scan::ScanMode;

fn parse_scan_mode(mode: &str) -> ScanMode {
    match mode {
        "smart" => ScanMode::Smart,
        "deep" => ScanMode::Deep,
        "static" => ScanMode::Static,
        // Legacy "ai" maps to Smart for backward compatibility
        "ai" => ScanMode::Smart,
        _ => ScanMode::Static,
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SecurityScanPayload {
    request_id: String,
    event: String, // skill-start | file-start | skill-complete | progress | error | done
    skill_name: Option<String>,
    file_name: Option<String>,
    result: Option<SecurityScanResult>,
    scanned: Option<usize>,
    total: Option<usize>,
    skill_file_scanned: Option<usize>,
    skill_file_total: Option<usize>,
    skill_chunk_completed: Option<usize>,
    skill_chunk_total: Option<usize>,
    active_chunk_workers: Option<usize>,
    max_chunk_workers: Option<usize>,
    message: Option<String>,
    phase: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityScanEstimatePayload {
    requested_mode: String,
    effective_mode: String,
    total_skills: usize,
    total_files: usize,
    ai_eligible_files: usize,
    estimated_chunks: usize,
    estimated_api_calls: usize,
    estimated_total_chars: usize,
    chunk_char_limit: usize,
}

struct PreparedSkillExecution {
    prepared: PreparedSkillScan,
    fresh_file_results: Vec<FileScanResult>,
    worker_failures: Vec<(String, FileRole, String)>,
    next_chunk_index: usize,
    inflight_chunks: usize,
    completed_chunks: usize,
}

impl PreparedSkillExecution {
    fn pending_chunks(&self) -> usize {
        self.prepared
            .chunks
            .len()
            .saturating_sub(self.next_chunk_index)
    }
}

struct ChunkTaskOutcome {
    #[allow(dead_code)]
    skill_idx: usize,
    chunk: PreparedChunk,
    result: Result<Vec<FileScanResult>, String>,
}

fn emit_scan_event(window: &tauri::Window, payload: SecurityScanPayload) {
    let _ = window.emit("ai://security-scan", payload);
}

fn effective_scan_mode(requested: ScanMode, config: &ai_provider::AiConfig) -> ScanMode {
    if requested.requires_ai() && config.enabled && !config.api_key.trim().is_empty() {
        requested
    } else {
        ScanMode::Static
    }
}

fn select_next_skill_index(states: &[(usize, usize)]) -> Option<usize> {
    let guaranteed = states
        .iter()
        .enumerate()
        .filter(|(_, (pending, inflight))| *pending > 0 && *inflight == 0)
        .max_by_key(|(idx, (pending, _))| (*pending, usize::MAX - idx));

    guaranteed
        .or_else(|| {
            states
                .iter()
                .enumerate()
                .filter(|(_, (pending, _))| *pending > 0)
                .max_by_key(|(idx, (pending, _))| (*pending, usize::MAX - idx))
        })
        .map(|(idx, _)| idx)
}

fn select_next_skill_for_chunk(skills: &[PreparedSkillExecution]) -> Option<usize> {
    let states = skills
        .iter()
        .map(|skill| (skill.pending_chunks(), skill.inflight_chunks))
        .collect::<Vec<_>>();
    select_next_skill_index(&states)
}

#[tauri::command]
pub async fn estimate_security_scan(
    skill_names: Vec<String>,
    mode: String,
) -> Result<SecurityScanEstimatePayload, String> {
    let config = ai_provider::load_config();
    let parsed_mode = parse_scan_mode(&mode);
    let effective_mode = effective_scan_mode(parsed_mode, &config);
    let resolved = ai_provider::resolve_scan_params(&config);
    let chunk_limit = resolved.chunk_char_limit;

    let hub_dir = crate::core::paths::hub_skills_dir();
    let target_names: Vec<String> = if skill_names.is_empty() {
        match std::fs::read_dir(&hub_dir) {
            Ok(entries) => entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_str().map(String::from))
                .collect(),
            Err(_) => vec![],
        }
    } else {
        skill_names
    };

    let mut total_files = 0usize;
    let mut ai_eligible_files = 0usize;
    let mut estimated_chunks = 0usize;
    let mut estimated_api_calls = 0usize;
    let mut estimated_total_chars = 0usize;

    for name in &target_names {
        let skill_dir = hub_dir.join(name);
        let real_dir = std::fs::canonicalize(&skill_dir).unwrap_or(skill_dir.clone());
        if !real_dir.is_dir() {
            continue;
        }

        let (files, _) = security_scan::collect_scannable_files(&real_dir);
        if files.is_empty() {
            continue;
        }
        let classifications = security_scan::classify_files(&files);
        let estimate =
            security_scan::estimate_scan(&files, &classifications, effective_mode, chunk_limit);

        total_files += estimate.total_files;
        ai_eligible_files += estimate.ai_eligible_files;
        estimated_chunks += estimate.estimated_chunks;
        estimated_api_calls += estimate.estimated_api_calls;
        estimated_total_chars += estimate.estimated_total_chars;
    }

    Ok(SecurityScanEstimatePayload {
        requested_mode: parsed_mode.label().to_string(),
        effective_mode: effective_mode.label().to_string(),
        total_skills: target_names.len(),
        total_files,
        ai_eligible_files,
        estimated_chunks,
        estimated_api_calls,
        estimated_total_chars,
        chunk_char_limit: chunk_limit,
    })
}

/// Batch security scan: up to 4 skills processed concurrently, files within
/// each skill analyzed concurrently via sub-agent workers.
#[tauri::command]
pub async fn ai_security_scan(
    window: tauri::Window,
    request_id: String,
    skill_names: Vec<String>,
    force: bool,
    mode: String,
) -> Result<Vec<SecurityScanResult>, String> {
    let run_started_at = chrono::Utc::now();
    let requested_mode = mode.clone();
    let request_id_value = request_id.clone();
    let config = Arc::new(ai_provider::load_config());

    // Resolve skill directories
    let hub_dir = crate::core::paths::hub_skills_dir();
    let target_names: Vec<String> = if skill_names.is_empty() {
        // Scan all installed skills — is_dir() already follows symlinks
        match std::fs::read_dir(&hub_dir) {
            Ok(entries) => entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_str().map(String::from))
                .collect(),
            Err(_) => vec![],
        }
    } else {
        skill_names
    };

    let total = target_names.len();
    let scanned_count = Arc::new(AtomicUsize::new(0));

    // Global AI concurrency semaphore — shared across all skill scans
    let resolved = ai_provider::resolve_scan_params(&config);
    let ai_concurrency = resolved.max_concurrent_requests.max(1) as usize;
    let ai_semaphore = Arc::new(TokioSemaphore::new(ai_concurrency));

    // Shared state across concurrent tasks
    let window = Arc::new(window);
    let request_id = Arc::new(request_id);
    let hub_dir = Arc::new(hub_dir);
    let parsed_mode = parse_scan_mode(&mode);
    let resolved_mode = effective_scan_mode(parsed_mode, &config);
    let mut cached_skill_names: Vec<String> = Vec::new();

    // --- Phase 1: fast cache hits (serial, no semaphore cost) ---
    let mut cached_results: Vec<SecurityScanResult> = Vec::new();
    let mut needs_scan: Vec<String> = Vec::new();
    CANCEL_SCAN.store(false, Ordering::Relaxed);

    if !force {
        for name in &target_names {
            let skill_dir = hub_dir.join(name);
            let real_dir = std::fs::canonicalize(&skill_dir).unwrap_or(skill_dir.clone());
            if !real_dir.is_dir() {
                continue;
            }
            // Compute content hash for cache check
            let (_, content_hash) = security_scan::collect_scannable_files(&real_dir);
            if let Some(cached) = security_scan::try_reuse_cached(
                name,
                resolved_mode,
                Some(&content_hash),
                &config.target_language,
            ) {
                security_scan::log_cached_skill_result(name, Some(&content_hash), &cached);
                let scanned = scanned_count.fetch_add(1, Ordering::Relaxed) + 1;
                emit_scan_event(
                    &window,
                    SecurityScanPayload {
                        request_id: request_id.to_string(),
                        event: "skill-complete".to_string(),
                        skill_name: Some(name.clone()),
                        file_name: None,
                        result: Some(cached.clone()),
                        scanned: Some(scanned),
                        total: Some(total),
                        skill_file_scanned: Some(cached.files_scanned),
                        skill_file_total: Some(cached.files_scanned),
                        skill_chunk_completed: Some(cached.chunks_used),
                        skill_chunk_total: Some(cached.chunks_used),
                        active_chunk_workers: Some(0),
                        max_chunk_workers: Some(ai_concurrency),
                        message: Some("cached".to_string()),
                        phase: Some("done".to_string()),
                    },
                );
                cached_results.push(cached);
                cached_skill_names.push(name.clone());
                continue;
            }
            needs_scan.push(name.clone());
        }
    } else {
        for name in &target_names {
            let skill_dir = hub_dir.join(name);
            let real_dir = std::fs::canonicalize(&skill_dir).unwrap_or(skill_dir.clone());
            if real_dir.is_dir() {
                needs_scan.push(name.clone());
            }
        }
    }

    let mut scan_results: Vec<SecurityScanResult> = Vec::new();
    let mut scan_errors: Vec<(String, String)> = Vec::new();

    // --- Phase 2: Streaming pipeline (no batch barriers) ---
    // Architecture: prepare skills one at a time, feed chunks into a shared
    // JoinSet of workers, finalize each skill as soon as all its chunks
    // complete.  The semaphore is the sole concurrency gate.
    //
    // Each execution tracks its owner skill_name so we can map outcomes back
    // even after Vec removals.
    let mut executions: Vec<PreparedSkillExecution> = Vec::new();
    let mut chunk_join_set: tokio::task::JoinSet<(String, ChunkTaskOutcome)> =
        tokio::task::JoinSet::new();
    let mut active_chunk_workers = 0usize;
    let mut skill_queue_idx = 0usize;

    loop {
        if CANCEL_SCAN.load(Ordering::Relaxed) {
            break;
        }

        // ── A. Prepare next skill if workers have room ──────────────
        // We keep (ai_concurrency + 1) skills alive so there are always
        // chunks ready when a worker slot opens.
        while skill_queue_idx < needs_scan.len()
            && executions.len() < ai_concurrency + 1
            && !CANCEL_SCAN.load(Ordering::Relaxed)
        {
            let name = needs_scan[skill_queue_idx].clone();
            skill_queue_idx += 1;

            let skill_dir = hub_dir.join(&name);
            let real_dir = std::fs::canonicalize(&skill_dir).unwrap_or(skill_dir);

            let progress_window = window.clone();
            let r_id = request_id.clone();
            let sn = name.clone();
            let on_progress = move |stage: &str, file_name: Option<&str>| {
                emit_scan_event(
                    &progress_window,
                    SecurityScanPayload {
                        request_id: r_id.to_string(),
                        event: if file_name.is_some() {
                            "file-start".to_string()
                        } else {
                            "progress".to_string()
                        },
                        skill_name: Some(sn.clone()),
                        file_name: file_name.map(String::from),
                        result: None,
                        scanned: None,
                        total: None,
                        skill_file_scanned: None,
                        skill_file_total: None,
                        skill_chunk_completed: None,
                        skill_chunk_total: None,
                        active_chunk_workers: Some(0),
                        max_chunk_workers: Some(ai_concurrency),
                        message: Some(stage.to_string()),
                        phase: Some(stage.to_string()),
                    },
                );
            };

            match security_scan::prepare_skill_scan(
                &config,
                &name,
                &real_dir,
                resolved_mode,
                Some(&on_progress),
                None,
            )
            .await
            {
                Ok(prepared) => {
                    let chunk_total = prepared.chunks.len();
                    let phase = if chunk_total > 0 {
                        "ai-analyze"
                    } else if prepared.actual_mode.requires_ai() {
                        "aggregate"
                    } else {
                        "static"
                    };
                    emit_scan_event(
                        &window,
                        SecurityScanPayload {
                            request_id: request_id.to_string(),
                            event: "skill-start".to_string(),
                            skill_name: Some(name.clone()),
                            file_name: None,
                            result: None,
                            scanned: Some(scanned_count.load(Ordering::Relaxed)),
                            total: Some(total),
                            skill_file_scanned: Some(prepared.files.len()),
                            skill_file_total: Some(prepared.files.len()),
                            skill_chunk_completed: Some(0),
                            skill_chunk_total: Some(chunk_total),
                            active_chunk_workers: Some(active_chunk_workers),
                            max_chunk_workers: Some(ai_concurrency),
                            message: None,
                            phase: Some(phase.to_string()),
                        },
                    );
                    executions.push(PreparedSkillExecution {
                        prepared,
                        fresh_file_results: Vec::new(),
                        worker_failures: Vec::new(),
                        next_chunk_index: 0,
                        inflight_chunks: 0,
                        completed_chunks: 0,
                    });
                }
                Err(e) => {
                    let scanned = scanned_count.fetch_add(1, Ordering::Relaxed) + 1;
                    let msg = e.to_string();
                    emit_scan_event(
                        &window,
                        SecurityScanPayload {
                            request_id: request_id.to_string(),
                            event: "error".to_string(),
                            skill_name: Some(name.clone()),
                            file_name: None,
                            result: None,
                            scanned: Some(scanned),
                            total: Some(total),
                            skill_file_scanned: Some(0),
                            skill_file_total: Some(0),
                            skill_chunk_completed: Some(0),
                            skill_chunk_total: Some(0),
                            active_chunk_workers: Some(0),
                            max_chunk_workers: Some(ai_concurrency),
                            message: Some(msg.clone()),
                            phase: Some("error".to_string()),
                        },
                    );
                    scan_errors.push((name, msg));
                }
            }
        }

        // ── B. Dispatch pending chunks ──────────────────────────────
        while !CANCEL_SCAN.load(Ordering::Relaxed) && active_chunk_workers < ai_concurrency {
            let Some(skill_idx) = select_next_skill_for_chunk(&executions) else {
                break;
            };
            let execution = &mut executions[skill_idx];
            let chunk = execution.prepared.chunks[execution.next_chunk_index].clone();
            execution.next_chunk_index += 1;
            execution.inflight_chunks += 1;
            active_chunk_workers += 1;

            let owner = execution.prepared.skill_name.clone();

            emit_scan_event(
                &window,
                SecurityScanPayload {
                    request_id: request_id.to_string(),
                    event: "progress".to_string(),
                    skill_name: Some(owner.clone()),
                    file_name: chunk.chunk_paths.first().cloned(),
                    result: None,
                    scanned: None,
                    total: None,
                    skill_file_scanned: Some(execution.prepared.files.len()),
                    skill_file_total: Some(execution.prepared.files.len()),
                    skill_chunk_completed: Some(execution.completed_chunks),
                    skill_chunk_total: Some(execution.prepared.chunks.len()),
                    active_chunk_workers: Some(active_chunk_workers),
                    max_chunk_workers: Some(ai_concurrency),
                    message: Some(format!("chunk {}/{}", chunk.chunk_num, chunk.total_chunks)),
                    phase: Some("ai-analyze".to_string()),
                },
            );

            let cfg = config.clone();
            let log_ctx = execution.prepared.log_ctx.clone();
            let ai_sem = ai_semaphore.clone();
            let owner_name = owner.clone();

            chunk_join_set.spawn(async move {
                let permit = ai_sem.acquire_owned().await.map_err(|e| e.to_string());
                let result = match permit {
                    Ok(permit) => {
                        let r = security_scan::analyze_prepared_chunk(
                            &cfg,
                            &owner_name,
                            &chunk,
                            &log_ctx,
                        )
                        .await
                        .map_err(|e| e.to_string());
                        drop(permit);
                        r
                    }
                    Err(err) => Err(err),
                };
                (
                    owner,
                    ChunkTaskOutcome {
                        skill_idx,
                        chunk,
                        result,
                    },
                )
            });
        }

        // ── C. Finalize zero-chunk skills ──────────────────────────
        let mut to_finalize: Vec<usize> = Vec::new();
        for (idx, exec) in executions.iter().enumerate() {
            if exec.prepared.chunks.is_empty() && exec.inflight_chunks == 0 {
                to_finalize.push(idx);
            }
        }
        for &idx in to_finalize.iter().rev() {
            let execution = executions.remove(idx);
            let skill_name = execution.prepared.skill_name.clone();
            let file_total = execution.prepared.files.len();
            let cfg = config.clone();
            let ai_sem = ai_semaphore.clone();
            let cancelled = CANCEL_SCAN.load(Ordering::Relaxed);

            let result = security_scan::finalize_prepared_skill::<fn(&str, Option<&str>)>(
                &cfg,
                execution.prepared,
                execution.fresh_file_results,
                execution.worker_failures,
                cancelled,
                ai_sem.as_ref(),
                None,
            )
            .await
            .map_err(|e| e.to_string());

            let scanned = scanned_count.fetch_add(1, Ordering::Relaxed) + 1;
            match result {
                Ok(r) => {
                    emit_scan_event(
                        &window,
                        SecurityScanPayload {
                            request_id: request_id.to_string(),
                            event: "skill-complete".to_string(),
                            skill_name: Some(skill_name),
                            file_name: None,
                            result: Some(r.clone()),
                            scanned: Some(scanned),
                            total: Some(total),
                            skill_file_scanned: Some(file_total),
                            skill_file_total: Some(file_total),
                            skill_chunk_completed: Some(0),
                            skill_chunk_total: Some(0),
                            active_chunk_workers: Some(active_chunk_workers),
                            max_chunk_workers: Some(ai_concurrency),
                            message: None,
                            phase: Some("done".to_string()),
                        },
                    );
                    scan_results.push(r);
                }
                Err(msg) => {
                    emit_scan_event(
                        &window,
                        SecurityScanPayload {
                            request_id: request_id.to_string(),
                            event: "error".to_string(),
                            skill_name: Some(skill_name.clone()),
                            file_name: None,
                            result: None,
                            scanned: Some(scanned),
                            total: Some(total),
                            skill_file_scanned: Some(file_total),
                            skill_file_total: Some(file_total),
                            skill_chunk_completed: Some(0),
                            skill_chunk_total: Some(0),
                            active_chunk_workers: Some(active_chunk_workers),
                            max_chunk_workers: Some(ai_concurrency),
                            message: Some(msg.clone()),
                            phase: Some("error".to_string()),
                        },
                    );
                    scan_errors.push((skill_name, msg));
                }
            }
        }

        // ── D. Exit conditions ─────────────────────────────────────
        if active_chunk_workers == 0 && executions.is_empty() {
            if skill_queue_idx >= needs_scan.len() {
                break;
            }
            continue;
        }
        if active_chunk_workers == 0 {
            continue;
        }

        // ── E. Wait for next chunk completion ──────────────────────
        let Some(joined) = chunk_join_set.join_next().await else {
            break;
        };
        let (owner_name, outcome) = joined.map_err(|e| e.to_string())?;
        active_chunk_workers = active_chunk_workers.saturating_sub(1);

        let Some(exec_idx) = executions
            .iter()
            .position(|e| e.prepared.skill_name == owner_name)
        else {
            continue;
        };

        let execution = &mut executions[exec_idx];
        execution.inflight_chunks = execution.inflight_chunks.saturating_sub(1);
        execution.completed_chunks += 1;

        match outcome.result {
            Ok(results) => {
                execution.fresh_file_results.extend(results);
            }
            Err(err_msg) => {
                // Emit chunk-error to frontend so user sees the actual API failure
                // (using chunk-error instead of error to avoid removing the skill from activeSkills)
                emit_scan_event(
                    &window,
                    SecurityScanPayload {
                        request_id: request_id.to_string(),
                        event: "chunk-error".to_string(),
                        skill_name: Some(owner_name.clone()),
                        file_name: outcome.chunk.chunk_paths.first().cloned(),
                        result: None,
                        scanned: None,
                        total: None,
                        skill_file_scanned: None,
                        skill_file_total: None,
                        skill_chunk_completed: Some(execution.completed_chunks),
                        skill_chunk_total: Some(execution.prepared.chunks.len()),
                        active_chunk_workers: Some(active_chunk_workers),
                        max_chunk_workers: Some(ai_concurrency),
                        message: Some(format!(
                            "Chunk {}/{} failed: {}",
                            outcome.chunk.chunk_num,
                            outcome.chunk.total_chunks,
                            err_msg
                        )),
                        phase: Some("error".to_string()),
                    },
                );
                for path in &outcome.chunk.chunk_paths {
                    execution.fresh_file_results.push(FileScanResult {
                        file_path: path.clone(),
                        role: FileRole::General,
                        findings: vec![],
                        file_risk: security_scan::RiskLevel::Low,
                        tokens_hint: 0,
                    });
                    execution.worker_failures.push((
                        path.clone(),
                        FileRole::General,
                        err_msg.clone(),
                    ));
                }
            }
        }

        emit_scan_event(
            &window,
            SecurityScanPayload {
                request_id: request_id.to_string(),
                event: "progress".to_string(),
                skill_name: Some(execution.prepared.skill_name.clone()),
                file_name: outcome.chunk.chunk_paths.first().cloned(),
                result: None,
                scanned: None,
                total: None,
                skill_file_scanned: Some(execution.prepared.files.len()),
                skill_file_total: Some(execution.prepared.files.len()),
                skill_chunk_completed: Some(execution.completed_chunks),
                skill_chunk_total: Some(execution.prepared.chunks.len()),
                active_chunk_workers: Some(active_chunk_workers),
                max_chunk_workers: Some(ai_concurrency),
                message: Some(format!(
                    "chunk {}/{}",
                    outcome.chunk.chunk_num, outcome.chunk.total_chunks
                )),
                phase: Some("ai-analyze".to_string()),
            },
        );

        // ── F. Finalize completed skill ────────────────────────────
        if executions[exec_idx].pending_chunks() == 0 && executions[exec_idx].inflight_chunks == 0 {
            let execution = executions.remove(exec_idx);
            let skill_name = execution.prepared.skill_name.clone();
            let completed_chunks = execution.completed_chunks;
            let total_chunks = execution.prepared.chunks.len();
            let file_total = execution.prepared.files.len();

            emit_scan_event(
                &window,
                SecurityScanPayload {
                    request_id: request_id.to_string(),
                    event: "progress".to_string(),
                    skill_name: Some(skill_name.clone()),
                    file_name: None,
                    result: None,
                    scanned: None,
                    total: None,
                    skill_file_scanned: Some(file_total),
                    skill_file_total: Some(file_total),
                    skill_chunk_completed: Some(completed_chunks),
                    skill_chunk_total: Some(total_chunks),
                    active_chunk_workers: Some(active_chunk_workers),
                    max_chunk_workers: Some(ai_concurrency),
                    message: Some("aggregate".to_string()),
                    phase: Some("aggregate".to_string()),
                },
            );

            let cfg = config.clone();
            let ai_sem = ai_semaphore.clone();
            let cancelled = CANCEL_SCAN.load(Ordering::Relaxed);

            let result = security_scan::finalize_prepared_skill::<fn(&str, Option<&str>)>(
                &cfg,
                execution.prepared,
                execution.fresh_file_results,
                execution.worker_failures,
                cancelled,
                ai_sem.as_ref(),
                None,
            )
            .await
            .map_err(|e| e.to_string());

            let scanned = scanned_count.fetch_add(1, Ordering::Relaxed) + 1;
            match result {
                Ok(r) => {
                    emit_scan_event(
                        &window,
                        SecurityScanPayload {
                            request_id: request_id.to_string(),
                            event: "skill-complete".to_string(),
                            skill_name: Some(skill_name),
                            file_name: None,
                            result: Some(r.clone()),
                            scanned: Some(scanned),
                            total: Some(total),
                            skill_file_scanned: Some(file_total),
                            skill_file_total: Some(file_total),
                            skill_chunk_completed: Some(completed_chunks),
                            skill_chunk_total: Some(total_chunks),
                            active_chunk_workers: Some(active_chunk_workers),
                            max_chunk_workers: Some(ai_concurrency),
                            message: None,
                            phase: Some("done".to_string()),
                        },
                    );
                    scan_results.push(r);
                }
                Err(msg) => {
                    emit_scan_event(
                        &window,
                        SecurityScanPayload {
                            request_id: request_id.to_string(),
                            event: "error".to_string(),
                            skill_name: Some(skill_name.clone()),
                            file_name: None,
                            result: None,
                            scanned: Some(scanned),
                            total: Some(total),
                            skill_file_scanned: Some(file_total),
                            skill_file_total: Some(file_total),
                            skill_chunk_completed: Some(completed_chunks),
                            skill_chunk_total: Some(total_chunks),
                            active_chunk_workers: Some(active_chunk_workers),
                            max_chunk_workers: Some(ai_concurrency),
                            message: Some(msg.clone()),
                            phase: Some("error".to_string()),
                        },
                    );
                    scan_errors.push((skill_name, msg));
                }
            }
        }
    }

    // Finalize remaining skills (cancelled mid-flight)
    let scan_cancelled = CANCEL_SCAN.load(Ordering::Relaxed);
    while let Some(execution) = executions.pop() {
        let skill_name = execution.prepared.skill_name.clone();
        let completed_chunks = execution.completed_chunks;
        let total_chunks = execution.prepared.chunks.len();
        let file_total = execution.prepared.files.len();
        let cfg = config.clone();
        let ai_sem = ai_semaphore.clone();

        let result = security_scan::finalize_prepared_skill::<fn(&str, Option<&str>)>(
            &cfg,
            execution.prepared,
            execution.fresh_file_results,
            execution.worker_failures,
            scan_cancelled,
            ai_sem.as_ref(),
            None,
        )
        .await
        .map_err(|e| e.to_string());

        let scanned = scanned_count.fetch_add(1, Ordering::Relaxed) + 1;
        match result {
            Ok(r) => {
                emit_scan_event(
                    &window,
                    SecurityScanPayload {
                        request_id: request_id.to_string(),
                        event: "skill-complete".to_string(),
                        skill_name: Some(skill_name),
                        file_name: None,
                        result: Some(r.clone()),
                        scanned: Some(scanned),
                        total: Some(total),
                        skill_file_scanned: Some(file_total),
                        skill_file_total: Some(file_total),
                        skill_chunk_completed: Some(completed_chunks),
                        skill_chunk_total: Some(total_chunks),
                        active_chunk_workers: Some(0),
                        max_chunk_workers: Some(ai_concurrency),
                        message: None,
                        phase: Some("done".to_string()),
                    },
                );
                scan_results.push(r);
            }
            Err(msg) => {
                emit_scan_event(
                    &window,
                    SecurityScanPayload {
                        request_id: request_id.to_string(),
                        event: "error".to_string(),
                        skill_name: Some(skill_name.clone()),
                        file_name: None,
                        result: None,
                        scanned: Some(scanned),
                        total: Some(total),
                        skill_file_scanned: Some(file_total),
                        skill_file_total: Some(file_total),
                        skill_chunk_completed: Some(completed_chunks),
                        skill_chunk_total: Some(total_chunks),
                        active_chunk_workers: Some(0),
                        max_chunk_workers: Some(ai_concurrency),
                        message: Some(msg.clone()),
                        phase: Some("error".to_string()),
                    },
                );
                scan_errors.push((skill_name, msg));
            }
        }
    }

    // Merge cached + scanned results
    let mut all_results = cached_results;
    all_results.extend(scan_results);
    let run_finished_at = chrono::Utc::now();
    let log_path = security_scan::persist_scan_run_log(
        &request_id_value,
        &requested_mode,
        resolved_mode.label(),
        force,
        run_started_at,
        run_finished_at,
        total,
        &cached_skill_names,
        &all_results,
        &scan_errors,
    )
    .ok()
    .map(|p| p.to_string_lossy().to_string());

    // Emit done
    emit_scan_event(
        &window,
        SecurityScanPayload {
            request_id: request_id.to_string(),
            event: "done".to_string(),
            skill_name: None,
            file_name: None,
            result: None,
            scanned: Some(total),
            total: Some(total),
            skill_file_scanned: None,
            skill_file_total: None,
            skill_chunk_completed: None,
            skill_chunk_total: None,
            active_chunk_workers: Some(0),
            max_chunk_workers: Some(ai_concurrency),
            message: log_path,
            phase: Some("done".to_string()),
        },
    );

    Ok(all_results)
}

/// Load all cached scan results (used by skill cards for badge display).
#[tauri::command]
pub async fn get_cached_scan_results() -> Result<Vec<SecurityScanResult>, String> {
    let hub_dir = crate::core::paths::hub_skills_dir();
    Ok(security_scan::load_all_cached()
        .into_iter()
        .filter(|result| {
            let skill_path = hub_dir.join(&result.skill_name);
            skill_path.is_dir() || skill_path.is_symlink()
        })
        .collect())
}

/// Clear the security scan cache.
#[tauri::command]
pub async fn clear_security_scan_cache() -> Result<(), String> {
    security_scan::clear_cache().map_err(|e| e.to_string())?;
    security_scan::clear_logs().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_security_scan_logs(
    limit: Option<usize>,
) -> Result<Vec<security_scan::SecurityScanLogEntry>, String> {
    let limit = limit.unwrap_or(30).clamp(1, 200);
    Ok(security_scan::list_scan_log_entries(limit))
}

#[tauri::command]
pub async fn get_security_scan_log_dir() -> Result<String, String> {
    Ok(security_scan::scan_logs_dir().to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::select_next_skill_index;

    #[test]
    fn scheduler_gives_first_lane_to_idle_skill_before_extra_lane() {
        let states = vec![(11, 1), (2, 0)];
        assert_eq!(select_next_skill_index(&states), Some(1));
    }

    #[test]
    fn scheduler_prefers_largest_backlog_once_each_skill_has_a_lane() {
        let states = vec![(12, 1), (5, 1), (2, 1)];
        assert_eq!(select_next_skill_index(&states), Some(0));
    }

    #[test]
    fn scheduler_returns_none_when_no_skill_has_pending_chunks() {
        let states = vec![(0, 0), (0, 1), (0, 0)];
        assert_eq!(select_next_skill_index(&states), None);
    }
}
