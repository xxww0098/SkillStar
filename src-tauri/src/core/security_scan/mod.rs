pub use skillstar_security_scan::*;

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Scan a single skill folder end-to-end.
pub async fn scan_single_skill<F>(
    config: &skillstar_ai::ai_provider::AiConfig,
    skill_name: &str,
    skill_dir: &Path,
    scan_mode: skillstar_security_scan::ScanMode,
    ai_semaphore: Arc<Semaphore>,
    on_progress: Option<&F>,
) -> Result<skillstar_security_scan::SecurityScanResult>
where
    F: Fn(&str, Option<&str>),
{
    let prepared = skillstar_security_scan::prepare_skill_scan(
        config,
        skill_name,
        skill_dir,
        scan_mode,
        on_progress,
        None,
    )
    .await?;
    let mut fresh_file_results: Vec<skillstar_security_scan::FileScanResult> = Vec::new();
    let mut worker_failures: Vec<(String, skillstar_security_scan::FileRole, String)> = Vec::new();
    let mut scan_cancelled = false;

    if !prepared.chunks.is_empty() {
        if let Some(cb) = on_progress {
            cb("ai-analyze", None);
        }

        let mut join_set: tokio::task::JoinSet<(
            skillstar_security_scan::PreparedChunk,
            Result<Vec<skillstar_security_scan::FileScanResult>, String>,
        )> = tokio::task::JoinSet::new();

        for chunk in prepared.chunks.clone() {
            if crate::commands::ai::CANCEL_SCAN.load(std::sync::atomic::Ordering::Relaxed) {
                scan_cancelled = true;
                break;
            }

            let cfg = config.clone();
            let skill_name = prepared.skill_name.clone();
            let log_ctx = prepared.log_ctx.clone();
            let ai_semaphore = ai_semaphore.clone();

            join_set.spawn(async move {
                let permit = match ai_semaphore
                    .acquire_owned()
                    .await
                    .map_err(|e| anyhow::anyhow!("AI semaphore error: {}", e))
                    .map_err(|e| e.to_string())
                {
                    Ok(permit) => permit,
                    Err(err) => return (chunk, Err(err)),
                };
                let result = skillstar_security_scan::analyze_prepared_chunk(
                    &cfg,
                    &skill_name,
                    &chunk,
                    &log_ctx,
                )
                .await
                .map_err(|e| e.to_string());
                drop(permit);
                (chunk, result)
            });
        }

        while let Some(joined) = join_set.join_next().await {
            if crate::commands::ai::CANCEL_SCAN.load(std::sync::atomic::Ordering::Relaxed) {
                join_set.abort_all();
                scan_cancelled = true;
            }

            let (chunk, outcome) = match joined {
                Ok(value) => value,
                Err(err) if scan_cancelled && err.is_cancelled() => continue,
                Err(err) => return Err(anyhow::anyhow!("Join error: {}", err)),
            };
            match outcome {
                Ok(results) => {
                    fresh_file_results.extend(results);
                }
                Err(err_msg) => {
                    for path in &chunk.chunk_paths {
                        fresh_file_results.push(skillstar_security_scan::FileScanResult {
                            file_path: path.clone(),
                            role: skillstar_security_scan::FileRole::General,
                            findings: vec![],
                            file_risk: skillstar_security_scan::RiskLevel::Low,
                            tokens_hint: 0,
                        });
                        worker_failures.push((
                            path.clone(),
                            skillstar_security_scan::FileRole::General,
                            err_msg.clone(),
                        ));
                    }
                }
            }
        }
    }

    skillstar_security_scan::finalize_prepared_skill(
        config,
        prepared,
        fresh_file_results,
        worker_failures,
        scan_cancelled,
        ai_semaphore.as_ref(),
        on_progress,
    )
    .await
}
