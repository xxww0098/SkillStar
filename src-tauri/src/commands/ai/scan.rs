use crate::core::ai_provider;
use crate::core::security_scan::{
    self, FileRole, FileScanResult, PreparedChunk, PreparedSkillScan, ScanMode, ScannedFile,
    SecurityScanPolicy, SecurityScanReportFormat, SecurityScanResult,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tauri::Emitter;
use tokio::sync::{Semaphore as TokioSemaphore, mpsc};
use tracing::warn;

pub static CANCEL_SCAN: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub async fn cancel_security_scan() -> Result<(), String> {
    CANCEL_SCAN.store(true, Ordering::Relaxed);
    Ok(())
}

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

#[derive(Clone, Serialize, Default)]
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

impl SecurityScanPayload {
    fn new(request_id: &str, event: &str) -> Self {
        Self {
            request_id: request_id.to_string(),
            event: event.to_string(),
            ..Default::default()
        }
    }

    fn skill(mut self, name: impl Into<String>) -> Self {
        self.skill_name = Some(name.into());
        self
    }

    fn with_result(mut self, r: SecurityScanResult) -> Self {
        self.result = Some(r);
        self
    }

    fn progress_counts(mut self, scanned: usize, total: usize) -> Self {
        self.scanned = Some(scanned);
        self.total = Some(total);
        self
    }

    fn files(mut self, scanned: usize, total: usize) -> Self {
        self.skill_file_scanned = Some(scanned);
        self.skill_file_total = Some(total);
        self
    }

    fn chunks(mut self, completed: usize, total: usize) -> Self {
        self.skill_chunk_completed = Some(completed);
        self.skill_chunk_total = Some(total);
        self
    }

    fn workers(mut self, active: usize, max: usize) -> Self {
        self.active_chunk_workers = Some(active);
        self.max_chunk_workers = Some(max);
        self
    }

    fn msg(mut self, m: impl Into<String>) -> Self {
        self.message = Some(m.into());
        self
    }

    fn phase(mut self, p: &str) -> Self {
        self.phase = Some(p.to_string());
        self
    }
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

/// Result of a parallel skill preparation task.
enum PrepResult {
    Ok {
        name: String,
        prepared: Box<PreparedSkillScan>,
    },
    Err {
        name: String,
        error: String,
    },
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
    let config = ai_provider::load_config_async().await;
    let parsed_mode = parse_scan_mode(&mode);
    let effective_mode = effective_scan_mode(parsed_mode, &config);
    let resolved = ai_provider::resolve_scan_params(&config);
    let chunk_limit = resolved.chunk_char_limit;

    let hub_dir = crate::core::infra::paths::hub_skills_dir();
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

type PreCollectedFiles = HashMap<String, (Vec<ScannedFile>, String)>;

struct PreparedScanState {
    cached_results: Vec<SecurityScanResult>,
    cached_skill_names: Vec<String>,
    needs_scan: Vec<String>,
    pre_collected_files: PreCollectedFiles,
}

struct PrepareScanArgs<'a> {
    force: bool,
    target_names: &'a [String],
    hub_dir: &'a std::path::Path,
    resolved_mode: ScanMode,
    config: &'a ai_provider::AiConfig,
    window: &'a Arc<tauri::Window>,
    request_id: &'a Arc<String>,
    scanned_count: &'a Arc<AtomicUsize>,
    total: usize,
    ai_concurrency: usize,
}

fn prepare_scan(args: PrepareScanArgs<'_>) -> PreparedScanState {
    let mut cached_results: Vec<SecurityScanResult> = Vec::new();
    let mut needs_scan: Vec<String> = Vec::new();
    let mut pre_collected_files: PreCollectedFiles = HashMap::new();
    let mut cached_skill_names: Vec<String> = Vec::new();

    CANCEL_SCAN.store(false, Ordering::Relaxed);

    if !args.force {
        for name in args.target_names {
            let skill_dir = args.hub_dir.join(name);
            let real_dir = std::fs::canonicalize(&skill_dir).unwrap_or(skill_dir.clone());
            if !real_dir.is_dir() {
                continue;
            }

            let (files, content_hash) = security_scan::collect_scannable_files(&real_dir);
            if let Some(cached) = security_scan::try_reuse_cached(
                name,
                args.resolved_mode,
                Some(&content_hash),
                &args.config.target_language,
            ) {
                security_scan::log_cached_skill_result(name, Some(&content_hash), &cached);
                let scanned = args.scanned_count.fetch_add(1, Ordering::Relaxed) + 1;
                emit_scan_event(
                    args.window,
                    SecurityScanPayload::new(args.request_id, "skill-complete")
                        .skill(name.clone())
                        .with_result(cached.clone())
                        .progress_counts(scanned, args.total)
                        .files(cached.files_scanned, cached.files_scanned)
                        .chunks(cached.chunks_used, cached.chunks_used)
                        .workers(0, args.ai_concurrency)
                        .msg("cached")
                        .phase("done"),
                );
                cached_results.push(cached);
                cached_skill_names.push(name.clone());
                continue;
            }

            pre_collected_files.insert(name.clone(), (files, content_hash));
            needs_scan.push(name.clone());
        }
    } else {
        for name in args.target_names {
            let skill_dir = args.hub_dir.join(name);
            let real_dir = std::fs::canonicalize(&skill_dir).unwrap_or(skill_dir.clone());
            if real_dir.is_dir() {
                needs_scan.push(name.clone());
            }
        }
    }

    PreparedScanState {
        cached_results,
        cached_skill_names,
        needs_scan,
        pre_collected_files,
    }
}

struct AggregateScanArgs<'a> {
    window: &'a Arc<tauri::Window>,
    request_id: &'a Arc<String>,
    request_id_value: &'a str,
    requested_mode: &'a str,
    resolved_mode: ScanMode,
    force: bool,
    run_started_at: chrono::DateTime<chrono::Utc>,
    total: usize,
    ai_concurrency: usize,
    telemetry_enabled: bool,
    cached_skill_names: &'a [String],
    cached_results: Vec<SecurityScanResult>,
    scan_results: Vec<SecurityScanResult>,
    scan_errors: &'a [(String, String)],
}

fn aggregate_results(args: AggregateScanArgs<'_>) -> Vec<SecurityScanResult> {
    let mut all_results = args.cached_results;
    all_results.extend(args.scan_results);
    let run_finished_at = chrono::Utc::now();
    let log_path = security_scan::persist_scan_run_log(
        args.request_id_value,
        args.requested_mode,
        args.resolved_mode.label(),
        args.force,
        args.run_started_at,
        run_finished_at,
        args.total,
        args.cached_skill_names,
        &all_results,
        args.scan_errors,
    )
    .ok()
    .map(|p| p.to_string_lossy().to_string());

    if args.telemetry_enabled {
        if let Err(err) = security_scan::persist_scan_telemetry(
            args.request_id_value,
            args.requested_mode,
            args.resolved_mode.label(),
            args.force,
            args.run_started_at,
            run_finished_at,
            args.total,
            &all_results,
            args.scan_errors,
        ) {
            warn!(
                target: "security_scan",
                error = %err,
                "failed to persist scan telemetry"
            );
        }
    }

    emit_scan_event(
        args.window,
        SecurityScanPayload::new(args.request_id, "done")
            .progress_counts(args.total, args.total)
            .workers(0, args.ai_concurrency)
            .phase("done")
            .msg(log_path.unwrap_or_default()),
    );

    all_results
}

/// Batch security scan: up to 4 skills processed concurrently, files within
/// each skill analyzed concurrently via sub-agent workers.
async fn run_ai_scan_pipeline(
    window: tauri::Window,
    request_id: String,
    skill_names: Vec<String>,
    force: bool,
    mode: String,
) -> Result<Vec<SecurityScanResult>, String> {
    let run_started_at = chrono::Utc::now();
    let requested_mode = mode.clone();
    let request_id_value = request_id.clone();
    let config = Arc::new(ai_provider::load_config_async().await);
    let telemetry_enabled = config.security_scan_telemetry_enabled;

    // Resolve skill directories
    let hub_dir = crate::core::infra::paths::hub_skills_dir();
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

    // Preparation concurrency: parallelize file I/O + static scan across skills
    let prep_semaphore = Arc::new(TokioSemaphore::new(ai_concurrency + 2));
    let (prep_tx, mut prep_rx) = mpsc::channel::<PrepResult>(ai_concurrency + 2);

    // Shared state across concurrent tasks
    let window = Arc::new(window);
    let request_id = Arc::new(request_id);
    let hub_dir = Arc::new(hub_dir);
    let parsed_mode = parse_scan_mode(&mode);
    let resolved_mode = effective_scan_mode(parsed_mode, &config);
    let PreparedScanState {
        cached_results,
        cached_skill_names,
        needs_scan,
        mut pre_collected_files,
    } = prepare_scan(PrepareScanArgs {
        force,
        target_names: &target_names,
        hub_dir: hub_dir.as_ref(),
        resolved_mode,
        config: config.as_ref(),
        window: &window,
        request_id: &request_id,
        scanned_count: &scanned_count,
        total,
        ai_concurrency,
    });

    let mut scan_results: Vec<SecurityScanResult> = Vec::new();
    let mut scan_errors: Vec<(String, String)> = Vec::new();

    // --- Phase 2: Streaming pipeline (no batch barriers) ---
    // Architecture: prepare skills in parallel, feed chunks into a shared
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
    let mut pending_prep: usize = 0; // in-flight preparation tasks

    loop {
        if CANCEL_SCAN.load(Ordering::Relaxed) {
            break;
        }

        // ── A. Spawn parallel preparation tasks ──────────────────────
        // File I/O + static scan + chunk building are CPU/IO-bound and
        // independent per skill, so we run them in parallel using
        // tokio::spawn, bounded by prep_semaphore.
        while skill_queue_idx < needs_scan.len()
            && (executions.len() + pending_prep) < ai_concurrency + 1
            && !CANCEL_SCAN.load(Ordering::Relaxed)
        {
            let name = needs_scan[skill_queue_idx].clone();
            skill_queue_idx += 1;

            let skill_dir = hub_dir.join(&name);
            let real_dir = std::fs::canonicalize(&skill_dir).unwrap_or(skill_dir);

            let cfg = (*config).clone();
            let progress_window = window.clone();
            let r_id = request_id.clone();
            let sn = name.clone();
            let on_progress = move |stage: &str, file_name: Option<&str>| {
                emit_scan_event(
                    &progress_window,
                    SecurityScanPayload::new(
                        &r_id,
                        if file_name.is_some() {
                            "file-start"
                        } else {
                            "progress"
                        },
                    )
                    .skill(sn.clone())
                    .workers(0, ai_concurrency)
                    .msg(stage)
                    .phase(stage),
                );
            };

            let pre_collected = pre_collected_files.remove(&name);
            let tx = prep_tx.clone();
            let sem = prep_semaphore.clone();

            pending_prep += 1;
            tokio::spawn(async move {
                let _permit = sem.acquire().await;
                let result = security_scan::prepare_skill_scan(
                    &cfg,
                    &name,
                    &real_dir,
                    resolved_mode,
                    Some(&on_progress),
                    pre_collected,
                )
                .await;
                let prep = match result {
                    Ok(prepared) => PrepResult::Ok {
                        name,
                        prepared: Box::new(prepared),
                    },
                    Err(e) => PrepResult::Err {
                        name,
                        error: e.to_string(),
                    },
                };
                let _ = tx.send(prep).await;
            });
        }

        // ── A2. Drain any completed preparation results ─────────────
        while let Ok(prep) = prep_rx.try_recv() {
            pending_prep = pending_prep.saturating_sub(1);
            match prep {
                PrepResult::Ok { name, prepared } => {
                    let prepared = *prepared;
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
                        SecurityScanPayload::new(&request_id, "skill-start")
                            .skill(name.clone())
                            .progress_counts(scanned_count.load(Ordering::Relaxed), total)
                            .files(prepared.files.len(), prepared.files.len())
                            .chunks(0, chunk_total)
                            .workers(active_chunk_workers, ai_concurrency)
                            .phase(phase),
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
                PrepResult::Err { name, error } => {
                    let scanned = scanned_count.fetch_add(1, Ordering::Relaxed) + 1;
                    emit_scan_event(
                        &window,
                        SecurityScanPayload::new(&request_id, "error")
                            .skill(name.clone())
                            .progress_counts(scanned, total)
                            .files(0, 0)
                            .chunks(0, 0)
                            .workers(0, ai_concurrency)
                            .msg(error.clone())
                            .phase("error"),
                    );
                    scan_errors.push((name, error));
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
                SecurityScanPayload::new(&request_id, "progress")
                    .skill(owner.clone())
                    .files(
                        execution.prepared.files.len(),
                        execution.prepared.files.len(),
                    )
                    .chunks(execution.completed_chunks, execution.prepared.chunks.len())
                    .workers(active_chunk_workers, ai_concurrency)
                    .msg(format!("chunk {}/{}", chunk.chunk_num, chunk.total_chunks))
                    .phase("ai-analyze"),
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
                        SecurityScanPayload::new(&request_id, "skill-complete")
                            .skill(skill_name)
                            .with_result(r.clone())
                            .progress_counts(scanned, total)
                            .files(file_total, file_total)
                            .chunks(0, 0)
                            .workers(active_chunk_workers, ai_concurrency)
                            .phase("done"),
                    );
                    scan_results.push(r);
                }
                Err(msg) => {
                    emit_scan_event(
                        &window,
                        SecurityScanPayload::new(&request_id, "error")
                            .skill(skill_name.clone())
                            .progress_counts(scanned, total)
                            .files(file_total, file_total)
                            .chunks(0, 0)
                            .workers(active_chunk_workers, ai_concurrency)
                            .msg(msg.clone())
                            .phase("error"),
                    );
                    scan_errors.push((skill_name, msg));
                }
            }
        }

        // ── D. Exit conditions ─────────────────────────────────────
        if active_chunk_workers == 0 && executions.is_empty() && pending_prep == 0 {
            if skill_queue_idx >= needs_scan.len() {
                break;
            }
            continue;
        }
        if active_chunk_workers == 0 && pending_prep == 0 {
            continue;
        }

        // ── E. Wait for next chunk completion or prep result ──────
        // Use select! so we don't stall when a preparation task finishes
        // while all chunk workers are idle.
        let chunk_fut = chunk_join_set.join_next();
        let prep_fut = prep_rx.recv();

        tokio::select! {
            biased; // prioritize chunk completions (AI workers are expensive)

            Some(joined) = chunk_fut, if active_chunk_workers > 0 => {
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
                        emit_scan_event(
                            &window,
                            SecurityScanPayload::new(&request_id, "chunk-error")
                                .skill(owner_name.clone())
                                .chunks(execution.completed_chunks, execution.prepared.chunks.len())
                                .workers(active_chunk_workers, ai_concurrency)
                                .msg(format!(
                                    "Chunk {}/{} failed: {}",
                                    outcome.chunk.chunk_num, outcome.chunk.total_chunks, err_msg
                                ))
                                .phase("error"),
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
                    SecurityScanPayload::new(&request_id, "progress")
                        .skill(execution.prepared.skill_name.clone())
                        .files(execution.prepared.files.len(), execution.prepared.files.len())
                        .chunks(execution.completed_chunks, execution.prepared.chunks.len())
                        .workers(active_chunk_workers, ai_concurrency)
                        .msg(format!(
                            "chunk {}/{}",
                            outcome.chunk.chunk_num, outcome.chunk.total_chunks
                        ))
                        .phase("ai-analyze"),
                );

                // ── F. Finalize completed skill ────────────────────
                if executions[exec_idx].pending_chunks() == 0 && executions[exec_idx].inflight_chunks == 0 {
                    let execution = executions.remove(exec_idx);
                    let skill_name = execution.prepared.skill_name.clone();
                    let completed_chunks = execution.completed_chunks;
                    let total_chunks = execution.prepared.chunks.len();
                    let file_total = execution.prepared.files.len();

                    emit_scan_event(
                        &window,
                        SecurityScanPayload::new(&request_id, "progress")
                            .skill(skill_name.clone())
                            .files(file_total, file_total)
                            .chunks(completed_chunks, total_chunks)
                            .workers(active_chunk_workers, ai_concurrency)
                            .msg("aggregating...")
                            .phase("aggregate"),
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
                                SecurityScanPayload::new(&request_id, "skill-complete")
                                    .skill(skill_name)
                                    .with_result(r.clone())
                                    .progress_counts(scanned, total)
                                    .files(file_total, file_total)
                                    .chunks(completed_chunks, total_chunks)
                                    .workers(active_chunk_workers, ai_concurrency)
                                    .phase("done"),
                            );
                            scan_results.push(r);
                        }
                        Err(msg) => {
                            emit_scan_event(
                                &window,
                                SecurityScanPayload::new(&request_id, "error")
                                    .skill(skill_name.clone())
                                    .progress_counts(scanned, total)
                                    .files(file_total, file_total)
                                    .chunks(completed_chunks, total_chunks)
                                    .workers(active_chunk_workers, ai_concurrency)
                                    .msg(msg.clone())
                                    .phase("error"),
                            );
                            scan_errors.push((skill_name, msg));
                        }
                    }
                }
            }

            Some(prep) = prep_fut, if pending_prep > 0 => {
                pending_prep = pending_prep.saturating_sub(1);
                match prep {
                    PrepResult::Ok { name, prepared } => {
                        let prepared = *prepared;
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
                            SecurityScanPayload::new(&request_id, "skill-start")
                                .skill(name.clone())
                                .progress_counts(scanned_count.load(Ordering::Relaxed), total)
                                .files(prepared.files.len(), prepared.files.len())
                                .chunks(0, chunk_total)
                                .workers(active_chunk_workers, ai_concurrency)
                                .phase(phase),
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
                    PrepResult::Err { name, error } => {
                        let scanned = scanned_count.fetch_add(1, Ordering::Relaxed) + 1;
                        emit_scan_event(
                            &window,
                            SecurityScanPayload::new(&request_id, "error")
                                .skill(name.clone())
                                .progress_counts(scanned, total)
                                .files(0, 0)
                                .chunks(0, 0)
                                .workers(0, ai_concurrency)
                                .msg(error.clone())
                                .phase("error"),
                        );
                        scan_errors.push((name, error));
                    }
                }
            }

            else => {
                break;
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
                    SecurityScanPayload::new(&request_id, "skill-complete")
                        .skill(skill_name)
                        .with_result(r.clone())
                        .progress_counts(scanned, total)
                        .files(file_total, file_total)
                        .chunks(completed_chunks, total_chunks)
                        .workers(0, ai_concurrency)
                        .phase("done"),
                );
                scan_results.push(r);
            }
            Err(msg) => {
                emit_scan_event(
                    &window,
                    SecurityScanPayload::new(&request_id, "error")
                        .skill(skill_name.clone())
                        .progress_counts(scanned, total)
                        .files(file_total, file_total)
                        .chunks(completed_chunks, total_chunks)
                        .workers(0, ai_concurrency)
                        .msg(msg.clone())
                        .phase("error"),
                );
                scan_errors.push((skill_name, msg));
            }
        }
    }

    Ok(aggregate_results(AggregateScanArgs {
        window: &window,
        request_id: &request_id,
        request_id_value: &request_id_value,
        requested_mode: &requested_mode,
        resolved_mode,
        force,
        run_started_at,
        total,
        ai_concurrency,
        telemetry_enabled,
        cached_skill_names: &cached_skill_names,
        cached_results,
        scan_results,
        scan_errors: &scan_errors,
    }))
}

#[tauri::command]
pub async fn ai_security_scan(
    window: tauri::Window,
    request_id: String,
    skill_names: Vec<String>,
    force: bool,
    mode: String,
) -> Result<Vec<SecurityScanResult>, String> {
    run_ai_scan_pipeline(window, request_id, skill_names, force, mode).await
}

/// Load all cached scan results (used by skill cards for badge display).
#[tauri::command]
pub async fn get_cached_scan_results() -> Result<Vec<SecurityScanResult>, String> {
    let hub_dir = crate::core::infra::paths::hub_skills_dir();
    Ok(security_scan::load_all_cached()
        .into_iter()
        .filter(|result| {
            let skill_path = hub_dir.join(&result.skill_name);
            skill_path.is_dir() || crate::core::infra::fs_ops::is_link(&skill_path)
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

#[tauri::command]
pub async fn get_security_scan_policy() -> Result<SecurityScanPolicy, String> {
    Ok(security_scan::get_policy())
}

#[tauri::command]
pub async fn save_security_scan_policy(policy: SecurityScanPolicy) -> Result<(), String> {
    security_scan::save_policy(&policy).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn export_security_scan_sarif(
    skill_names: Option<Vec<String>>,
    request_label: Option<String>,
) -> Result<String, String> {
    let hub_dir = crate::core::infra::paths::hub_skills_dir();
    let mut results = security_scan::load_all_cached()
        .into_iter()
        .filter(|result| {
            let skill_path = hub_dir.join(&result.skill_name);
            skill_path.is_dir() || crate::core::infra::fs_ops::is_link(&skill_path)
        })
        .collect::<Vec<_>>();

    if let Some(names) = skill_names {
        if !names.is_empty() {
            let requested: std::collections::HashSet<String> = names.into_iter().collect();
            results.retain(|result| requested.contains(&result.skill_name));
        }
    }

    let path = security_scan::export_sarif_report(&results, request_label.as_deref())
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn export_security_scan_report(
    format: String,
    skill_names: Option<Vec<String>>,
    request_label: Option<String>,
) -> Result<String, String> {
    let hub_dir = crate::core::infra::paths::hub_skills_dir();
    let mut results = security_scan::load_all_cached()
        .into_iter()
        .filter(|result| {
            let skill_path = hub_dir.join(&result.skill_name);
            skill_path.is_dir() || crate::core::infra::fs_ops::is_link(&skill_path)
        })
        .collect::<Vec<_>>();

    if let Some(names) = skill_names {
        if !names.is_empty() {
            let requested: std::collections::HashSet<String> = names.into_iter().collect();
            results.retain(|result| requested.contains(&result.skill_name));
        }
    }

    let parsed_format = SecurityScanReportFormat::parse_loose(&format);
    let path = security_scan::export_scan_report(&results, parsed_format, request_label.as_deref())
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
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
