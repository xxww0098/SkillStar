//! Reusable security-scan core for SkillStar.

pub mod constants;
pub mod orchestrator;
pub mod policy;
pub mod smart_rules;
pub mod snippet;
pub mod static_patterns;
pub mod types;

pub mod scan;

pub use scan::{
    SecurityScanReportFormat, analyze_prepared_chunk, build_html_report, build_json_report,
    build_markdown_report, build_sarif_report, classify_files, clear_cache, clear_logs,
    collect_scannable_files, estimate_scan, export_sarif_report, export_scan_report,
    finalize_prepared_skill, get_scan_audit_detail, invalidate_skill_cache,
    list_scan_audit_summaries, list_scan_log_entries, load_all_cached, log_cached_skill_result,
    persist_scan_run_log, persist_scan_telemetry, prepare_skill_scan, scan_logs_dir,
    try_reuse_cached,
};

pub use policy::{get_policy, save_policy};
pub use static_patterns::static_pattern_scan;
pub use types::{
    AiFinding, AnalyzerExecutionSummary, EvidenceTrailEntry, EvidenceTrailKind, FileRole,
    FileScanResult, PreparedChunk, PreparedSkillScan, RiskLevel, ScanEstimate, ScanMode,
    ScannedFile, SecurityScanAuditDetail, SecurityScanAuditError, SecurityScanAuditFinding,
    SecurityScanAuditSkillDetail, SecurityScanAuditSummary, SecurityScanAuditTelemetrySnapshot,
    SecurityScanLogEntry, SecurityScanPolicy, SecurityScanResult, SecurityScanTelemetryEntry,
    StaticFinding,
};

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static std::sync::Mutex<()> {
    use std::sync::{Mutex, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(test)]
pub(crate) fn lock_test_env() -> std::sync::MutexGuard<'static, ()> {
    test_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
