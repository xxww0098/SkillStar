//! Security scanning for AI skill folders.
//!
//! Three-tier architecture:
//! 1. **Static** — zero AI cost, regex pattern matching only
//! 2. **Smart** — triage-filtered files → chunk-batched AI analysis
//! 3. **Deep** — all files → chunk-batched AI analysis
//!
//! Prompts are loaded at compile time from `src-tauri/prompts/security/`.
//! Scan logs are written to `~/.skillstar/security_scan.log`.

use anyhow::{Context, Result};
use regex::Regex;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Semaphore;

use super::ai_provider::{AiConfig, chat_completion, chat_completion_capped};

// ── Compile-time prompt embedding ───────────────────────────────────

#[allow(dead_code)]
const SKILL_AGENT_PROMPT: &str = include_str!("../../../prompts/security/skill_agent.md");
#[allow(dead_code)]
const SCRIPT_AGENT_PROMPT: &str = include_str!("../../../prompts/security/script_agent.md");
#[allow(dead_code)]
const RESOURCE_AGENT_PROMPT: &str = include_str!("../../../prompts/security/resource_agent.md");
#[allow(dead_code)]
const GENERAL_AGENT_PROMPT: &str = include_str!("../../../prompts/security/general_agent.md");
const AGGREGATOR_PROMPT: &str = include_str!("../../../prompts/security/aggregator.md");
const CHUNK_BATCH_PROMPT: &str = include_str!("../../../prompts/security/chunk_batch.md");

// ── Constants ───────────────────────────────────────────────────────

const MAX_FILE_CHARS: usize = 8_000;
const CACHE_MAX_ENTRIES: usize = 200;
const MAX_RECURSION_DEPTH: usize = 10;
const SNIPPET_MAX_CHARS: usize = 200;
const CACHE_SCHEMA_VERSION: &str = "security-scan-v3";
const SCAN_LOG_ARCHIVE_MAX_ENTRIES: usize = 500;
const CHUNK_MAX_RETRIES: usize = 2;
const CHUNK_RETRY_DELAY_MS: u64 = 1500;

const FILE_CACHE_MAX_ENTRIES: usize = 5_000;

const SCANNABLE_EXTENSIONS: &[&str] = &[
    "md", "sh", "py", "js", "ts", "yaml", "yml", "json", "toml", "txt", "cfg", "ini", "bat", "ps1",
    "rb", "lua", "bash", "zsh", "fish", "pl", "r",
];

const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "__pycache__",
    ".next",
    "build",
    ".venv",
    "venv",
];

// ── Data Types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Safe,
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().trim() {
            "safe" | "none" => Self::Safe,
            "low" | "info" => Self::Low,
            "medium" | "moderate" => Self::Medium,
            "high" => Self::High,
            "critical" | "severe" => Self::Critical,
            _ => Self::Low, // Conservative: unknown values → Low, not Safe
        }
    }

    fn severity_ord(&self) -> u8 {
        match self {
            Self::Safe => 0,
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
            Self::Critical => 4,
        }
    }

    pub fn max(a: Self, b: Self) -> Self {
        if a.severity_ord() >= b.severity_ord() {
            a
        } else {
            b
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedFile {
    pub relative_path: String,
    pub content: String,
    pub size_bytes: usize,
    #[serde(skip)]
    pub content_digest: String,
}

impl ScannedFile {
    fn file_name(&self) -> &str {
        Path::new(&self.relative_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&self.relative_path)
    }

    fn extension(&self) -> &str {
        Path::new(&self.relative_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRole {
    Skill,    // SKILL.md + referenced .md files
    Script,   // Executable scripts
    Resource, // Config/data files
    General,  // Catch-all fallback
}

impl std::fmt::Display for FileRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_label())
    }
}

impl FileRole {
    pub fn as_label(&self) -> &'static str {
        match self {
            Self::Skill => "Skill",
            Self::Script => "Script",
            Self::Resource => "Resource",
            Self::General => "General",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanMode {
    Static,
    Smart,
    Deep,
}

impl ScanMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Smart => "smart",
            Self::Deep => "deep",
        }
    }

    pub fn requires_ai(&self) -> bool {
        matches!(self, Self::Smart | Self::Deep)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticFinding {
    pub file_path: String,
    pub line_number: usize,
    pub pattern_id: String,
    pub snippet: String,
    pub severity: RiskLevel,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiFinding {
    pub category: String,
    pub severity: RiskLevel,
    pub file_path: String,
    pub description: String,
    pub evidence: String,
    pub recommendation: String,
}

/// Per-file worker result (internal, not serialized to frontend)
#[derive(Debug, Clone)]
pub(crate) struct FileScanResult {
    pub file_path: String,
    pub role: FileRole,
    pub findings: Vec<AiFinding>,
    pub file_risk: RiskLevel,
    #[allow(dead_code)]
    pub tokens_hint: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedChunk {
    pub chunk_num: usize,
    pub total_chunks: usize,
    pub chunk_content: String,
    pub chunk_paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedSkillScan {
    pub skill_name: String,
    pub files: Vec<ScannedFile>,
    pub classifications: Vec<(FileRole, usize)>,
    pub static_findings: Vec<StaticFinding>,
    pub cached_file_results: Vec<FileScanResult>,
    pub cached_file_hits: usize,
    pub chunks: Vec<PreparedChunk>,
    pub content_hash: String,
    pub total_chars_analyzed: usize,
    pub actual_mode: ScanMode,
    pub run_ai: bool,
    pub ai_files_analyzed: usize,
    pub log_ctx: SkillScanLogCtx,
    pub scan_start: std::time::Instant,
}

/// Final per-skill result (cached + sent to frontend)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityScanResult {
    pub skill_name: String,
    pub scanned_at: String,
    pub tree_hash: Option<String>,
    #[serde(default = "default_scan_mode")]
    pub scan_mode: String,
    #[serde(default = "default_scanner_version")]
    pub scanner_version: String,
    #[serde(default = "default_target_language")]
    pub target_language: String,
    pub risk_level: RiskLevel,
    pub static_findings: Vec<StaticFinding>,
    pub ai_findings: Vec<AiFinding>,
    pub summary: String,
    pub files_scanned: usize,
    pub total_chars_analyzed: usize,
    #[serde(default)]
    pub incomplete: bool,
    #[serde(default)]
    pub ai_files_analyzed: usize,
    #[serde(default)]
    pub chunks_used: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanEstimate {
    pub total_files: usize,
    pub ai_eligible_files: usize,
    pub estimated_chunks: usize,
    pub estimated_api_calls: usize,
    pub estimated_total_chars: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecurityScanLogEntry {
    pub file_name: String,
    pub path: String,
    pub created_at: String,
    pub size_bytes: u64,
}

fn default_scan_mode() -> String {
    "static".to_string()
}

fn default_scanner_version() -> String {
    CACHE_SCHEMA_VERSION.to_string()
}

fn default_target_language() -> String {
    "zh-CN".to_string()
}

// ── Logging ─────────────────────────────────────────────────────────

fn log_path() -> PathBuf {
    super::paths::data_root().join("security_scan.log")
}

pub fn scan_logs_dir() -> PathBuf {
    super::paths::data_root().join("security_scan_logs")
}

static SCAN_LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static SCAN_LOG_SEQ: AtomicU64 = AtomicU64::new(1);

fn scan_log(msg: &str) {
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("[{}] {}\n", ts, msg);
    // Best-effort append; don't fail the scan if logging fails
    if let Some(parent) = log_path().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(_guard) = SCAN_LOG_LOCK.get_or_init(|| Mutex::new(())).lock() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path())
            .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SkillScanLogCtx {
    scan_id: u64,
    skill_name: String,
}

impl SkillScanLogCtx {
    fn new(skill_name: &str) -> Self {
        Self {
            scan_id: SCAN_LOG_SEQ.fetch_add(1, AtomicOrdering::Relaxed),
            skill_name: skill_name.to_string(),
        }
    }

    fn log(&self, stage: &str, message: impl AsRef<str>) {
        scan_log(&format!(
            "[security-scan][scan_id={:06}][skill={}][stage={}] {}",
            self.scan_id,
            self.skill_name,
            stage,
            message.as_ref()
        ));
    }
}

#[derive(Default)]
struct RiskTally {
    safe: usize,
    low: usize,
    medium: usize,
    high: usize,
    critical: usize,
}

impl RiskTally {
    fn add(&mut self, level: RiskLevel) {
        match level {
            RiskLevel::Safe => self.safe += 1,
            RiskLevel::Low => self.low += 1,
            RiskLevel::Medium => self.medium += 1,
            RiskLevel::High => self.high += 1,
            RiskLevel::Critical => self.critical += 1,
        }
    }

    fn compact(&self) -> String {
        format!(
            "critical={} high={} medium={} low={} safe={}",
            self.critical, self.high, self.medium, self.low, self.safe
        )
    }
}

fn risk_label(level: RiskLevel) -> &'static str {
    match level {
        RiskLevel::Safe => "safe",
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
        RiskLevel::Critical => "critical",
    }
}

fn flatten_for_log(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_for_log(input: &str, max_chars: usize) -> String {
    let compact = flatten_for_log(input);
    if compact.chars().count() <= max_chars {
        return compact.replace('"', "'");
    }
    let mut out = String::new();
    for (idx, ch) in compact.chars().enumerate() {
        if idx >= max_chars {
            break;
        }
        out.push(ch);
    }
    format!("{}...", out.replace('"', "'"))
}

fn short_hash(hash: &str) -> String {
    hash.chars().take(12).collect()
}

fn file_role_counts(classifications: &[(FileRole, usize)]) -> String {
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (role, _) in classifications {
        *counts.entry(role.as_label()).or_insert(0) += 1;
    }
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .into_iter()
        .map(|(role, count)| format!("{}={}", role, count))
        .collect::<Vec<_>>()
        .join(" ")
}

fn log_findings_breakdown(log_ctx: &SkillScanLogCtx, result: &SecurityScanResult) {
    let mut static_tally = RiskTally::default();
    for finding in &result.static_findings {
        static_tally.add(finding.severity);
    }
    let mut ai_tally = RiskTally::default();
    for finding in &result.ai_findings {
        ai_tally.add(finding.severity);
    }

    log_ctx.log(
        "result-severity",
        format!(
            "static=[{}] ai=[{}]",
            static_tally.compact(),
            ai_tally.compact()
        ),
    );

    for (idx, finding) in result.static_findings.iter().enumerate() {
        log_ctx.log(
            "static-finding",
            format!(
                "idx={} severity={} file={} line={} pattern={} desc=\"{}\" snippet=\"{}\"",
                idx + 1,
                risk_label(finding.severity),
                finding.file_path,
                finding.line_number,
                finding.pattern_id,
                truncate_for_log(&finding.description, 200),
                truncate_for_log(&finding.snippet, 180)
            ),
        );
    }

    for (idx, finding) in result.ai_findings.iter().enumerate() {
        log_ctx.log(
            "ai-finding",
            format!(
                "idx={} severity={} file={} category={} desc=\"{}\" evidence=\"{}\" recommendation=\"{}\"",
                idx + 1,
                risk_label(finding.severity),
                finding.file_path,
                truncate_for_log(&finding.category, 80),
                truncate_for_log(&finding.description, 220),
                truncate_for_log(&finding.evidence, 180),
                truncate_for_log(&finding.recommendation, 180)
            ),
        );
    }

    log_ctx.log(
        "result-summary",
        format!("summary=\"{}\"", truncate_for_log(&result.summary, 320)),
    );
}

fn log_live_scan_details(
    log_ctx: &SkillScanLogCtx,
    result: &SecurityScanResult,
    classifications: &[(FileRole, usize)],
    file_results: &[FileScanResult],
    worker_failures: &[(String, FileRole, String)],
    content_hash: &str,
    run_ai: bool,
    ai_enabled: bool,
    elapsed: std::time::Duration,
) {
    log_ctx.log(
        "result",
        format!(
            "risk={} elapsed_ms={} files_scanned={} total_chars={} static_findings={} ai_findings={} role_counts=\"{}\" worker_success={} worker_failures={} run_ai={} ai_enabled={} hash={}",
            risk_label(result.risk_level),
            elapsed.as_millis(),
            result.files_scanned,
            result.total_chars_analyzed,
            result.static_findings.len(),
            result.ai_findings.len(),
            file_role_counts(classifications),
            file_results.len(),
            worker_failures.len(),
            run_ai,
            ai_enabled,
            short_hash(content_hash)
        ),
    );

    let mut file_risk_tally = RiskTally::default();
    for file_result in file_results {
        file_risk_tally.add(file_result.file_risk);
    }
    log_ctx.log(
        "result-file-risk",
        format!("distribution=[{}]", file_risk_tally.compact()),
    );

    let mut ordered_results: Vec<&FileScanResult> = file_results.iter().collect();
    ordered_results.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    for file_result in ordered_results {
        log_ctx.log(
            "file-result",
            format!(
                "file={} role={} risk={} findings={} tokens_hint={}",
                file_result.file_path,
                file_result.role.as_label(),
                risk_label(file_result.file_risk),
                file_result.findings.len(),
                file_result.tokens_hint
            ),
        );
    }

    for (file_path, role, err) in worker_failures {
        log_ctx.log(
            "file-error",
            format!(
                "file={} role={} error=\"{}\"",
                file_path,
                role.as_label(),
                truncate_for_log(err, 260)
            ),
        );
    }

    log_findings_breakdown(log_ctx, result);
}

pub fn log_cached_skill_result(
    skill_name: &str,
    content_hash: Option<&str>,
    result: &SecurityScanResult,
) {
    let log_ctx = SkillScanLogCtx::new(skill_name);
    let hash = content_hash
        .map(short_hash)
        .or_else(|| result.tree_hash.as_deref().map(short_hash))
        .unwrap_or_else(|| "-".to_string());

    log_ctx.log(
        "cached-result",
        format!(
            "mode={} incomplete={} risk={} files_scanned={} total_chars={} static_findings={} ai_findings={} hash={} scanned_at={}",
            result.scan_mode,
            result.incomplete,
            risk_label(result.risk_level),
            result.files_scanned,
            result.total_chars_analyzed,
            result.static_findings.len(),
            result.ai_findings.len(),
            hash,
            result.scanned_at
        ),
    );
    log_findings_breakdown(&log_ctx, result);
}

fn sanitize_path_token(raw: &str) -> String {
    let trimmed = raw.trim();
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
            if out.len() >= 48 {
                break;
            }
        }
    }
    if out.is_empty() {
        "scan".to_string()
    } else {
        out
    }
}

fn prune_old_scan_logs() {
    let dir = scan_logs_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    let mut files: Vec<(PathBuf, std::time::SystemTime)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let modified = entry
                .metadata()
                .ok()
                .and_then(|meta| meta.modified().ok())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            Some((path, modified))
        })
        .collect();

    files.sort_by(|a, b| b.1.cmp(&a.1));
    for (path, _) in files.into_iter().skip(SCAN_LOG_ARCHIVE_MAX_ENTRIES) {
        let _ = std::fs::remove_file(path);
    }
}

pub fn persist_scan_run_log(
    request_id: &str,
    requested_mode: &str,
    effective_mode: &str,
    force: bool,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: chrono::DateTime<chrono::Utc>,
    total_targets: usize,
    cached_skill_names: &[String],
    results: &[SecurityScanResult],
    errors: &[(String, String)],
) -> Result<PathBuf> {
    let dir = scan_logs_dir();
    std::fs::create_dir_all(&dir).context("Failed to create security scan logs directory")?;

    let ts_label = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    let req_label = sanitize_path_token(request_id);
    let file_name = format!("scan-{}-{}.log", ts_label, req_label);
    let path = dir.join(file_name);

    let mut lines: Vec<String> = Vec::new();
    lines.push("SkillStar Security Scan Report".to_string());
    lines.push("=".repeat(48));
    lines.push(format!("request_id: {}", request_id));
    lines.push(format!("requested_mode: {}", requested_mode));
    lines.push(format!("effective_mode: {}", effective_mode));
    lines.push(format!("force: {}", force));
    lines.push(format!("started_at: {}", started_at.to_rfc3339()));
    lines.push(format!("finished_at: {}", finished_at.to_rfc3339()));
    lines.push(format!(
        "duration_ms: {}",
        (finished_at - started_at).num_milliseconds().max(0)
    ));
    lines.push(format!("targets_total: {}", total_targets));
    lines.push(format!("cached_hits: {}", cached_skill_names.len()));
    lines.push(format!("completed_results: {}", results.len()));
    lines.push(format!("errors: {}", errors.len()));
    lines.push(String::new());

    if !cached_skill_names.is_empty() {
        lines.push("Cached Skills".to_string());
        lines.push("-".repeat(24));
        for name in cached_skill_names {
            lines.push(format!("- {}", name));
        }
        lines.push(String::new());
    }

    if !errors.is_empty() {
        lines.push("Scan Errors".to_string());
        lines.push("-".repeat(24));
        for (skill_name, message) in errors {
            lines.push(format!(
                "- skill={} error=\"{}\"",
                skill_name,
                truncate_for_log(message, 320)
            ));
        }
        lines.push(String::new());
    }

    if results.is_empty() {
        lines.push("No scan results captured.".to_string());
    } else {
        let mut sorted_results = results.to_vec();
        sorted_results.sort_by(|a, b| {
            b.risk_level
                .severity_ord()
                .cmp(&a.risk_level.severity_ord())
                .then_with(|| a.skill_name.cmp(&b.skill_name))
        });

        for result in &sorted_results {
            lines.push(format!("Skill: {}", result.skill_name));
            lines.push(format!("  scanned_at: {}", result.scanned_at));
            lines.push(format!("  risk: {}", risk_label(result.risk_level)));
            lines.push(format!("  scan_mode: {}", result.scan_mode));
            lines.push(format!("  scanner_version: {}", result.scanner_version));
            lines.push(format!("  incomplete: {}", result.incomplete));
            lines.push(format!(
                "  files_scanned: {}  total_chars: {}",
                result.files_scanned, result.total_chars_analyzed
            ));
            lines.push(format!(
                "  findings: static={} ai={}",
                result.static_findings.len(),
                result.ai_findings.len()
            ));
            if let Some(hash) = &result.tree_hash {
                lines.push(format!("  tree_hash: {}", short_hash(hash)));
            }
            lines.push(format!(
                "  summary: {}",
                truncate_for_log(&result.summary, 360)
            ));

            if !result.static_findings.is_empty() {
                lines.push("  static_findings:".to_string());
                for finding in &result.static_findings {
                    lines.push(format!(
                        "    - [{}] {}:{} {} ({})",
                        risk_label(finding.severity),
                        finding.file_path,
                        finding.line_number,
                        truncate_for_log(&finding.description, 220),
                        finding.pattern_id
                    ));
                }
            }

            if !result.ai_findings.is_empty() {
                lines.push("  ai_findings:".to_string());
                for finding in &result.ai_findings {
                    lines.push(format!(
                        "    - [{}] {} {}: {}",
                        risk_label(finding.severity),
                        finding.file_path,
                        truncate_for_log(&finding.category, 80),
                        truncate_for_log(&finding.description, 240)
                    ));
                    if !finding.evidence.trim().is_empty() {
                        lines.push(format!(
                            "      evidence: {}",
                            truncate_for_log(&finding.evidence, 220)
                        ));
                    }
                    if !finding.recommendation.trim().is_empty() {
                        lines.push(format!(
                            "      recommendation: {}",
                            truncate_for_log(&finding.recommendation, 220)
                        ));
                    }
                }
            }

            lines.push(String::new());
        }
    }

    std::fs::write(&path, lines.join("\n")).context("Failed to write security scan report")?;
    prune_old_scan_logs();
    Ok(path)
}

pub fn list_scan_log_entries(limit: usize) -> Vec<SecurityScanLogEntry> {
    let dir = scan_logs_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return vec![],
    };

    let mut logs: Vec<(PathBuf, String, std::time::SystemTime, u64)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let file_name = entry.file_name().to_string_lossy().to_string();
            if !file_name.starts_with("scan-") || !file_name.ends_with(".log") {
                return None;
            }
            let meta = entry.metadata().ok()?;
            let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            Some((path, file_name, modified, meta.len()))
        })
        .collect();

    logs.sort_by(|a, b| b.2.cmp(&a.2));
    logs.into_iter()
        .take(limit)
        .map(|(path, file_name, modified, size_bytes)| {
            let modified_dt: chrono::DateTime<chrono::Utc> = modified.into();
            SecurityScanLogEntry {
                file_name,
                path: path.to_string_lossy().to_string(),
                created_at: modified_dt.to_rfc3339(),
                size_bytes,
            }
        })
        .collect()
}

// ── File Collection ─────────────────────────────────────────────────

/// Recursively collect scannable text files from a skill directory.
/// Returns `(files, content_hash)` — the hash is a SHA-256 digest of
/// all file contents sorted by relative path, used for cache validation.
pub fn collect_scannable_files(skill_dir: &Path) -> (Vec<ScannedFile>, String) {
    let mut files = Vec::new();
    collect_recursive(skill_dir, skill_dir, &mut files, 0);
    let content_hash = compute_content_hash(&files);
    (files, content_hash)
}

/// Compute a composite SHA-256 hash from all collected files.
/// Files are sorted by relative path to ensure deterministic ordering.
fn compute_content_hash(files: &[ScannedFile]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut paths: Vec<&ScannedFile> = files.iter().collect();
    paths.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    for file in paths {
        hasher.update(file.relative_path.as_bytes());
        hasher.update(b"\0");
        hasher.update(file.content_digest.as_bytes());
        hasher.update(b"\0");
    }
    let result = hasher.finalize();
    result
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

fn collect_recursive(root: &Path, dir: &Path, out: &mut Vec<ScannedFile>, depth: usize) {
    if depth > MAX_RECURSION_DEPTH {
        scan_log(&format!(
            "  [COLLECT] Max recursion depth reached at {}",
            dir.display()
        ));
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Never follow symlinks; they may point outside the skill root.
        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            collect_recursive(root, &path, out, depth + 1);
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if !SCANNABLE_EXTENSIONS.contains(&ext.as_str()) {
            // Check if file has a shebang even without recognised extension
            if let Ok(content) = std::fs::read_to_string(&path) {
                if content.starts_with("#!") && content.len() < MAX_FILE_CHARS * 2 {
                    let rel = path
                        .strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string();
                    let truncated = truncate_content(&content);
                    let size = truncated.len();
                    out.push(ScannedFile {
                        relative_path: rel,
                        content: truncated,
                        size_bytes: size,
                        content_digest: digest_text(&content),
                    });
                }
            }
            continue;
        }

        if let Ok(content) = std::fs::read_to_string(&path) {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            let truncated = truncate_content(&content);
            let size = truncated.len();
            out.push(ScannedFile {
                relative_path: rel,
                content: truncated,
                size_bytes: size,
                content_digest: digest_text(&content),
            });
        }
    }
}

fn digest_text(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    result
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

fn truncate_content(content: &str) -> String {
    if content.len() <= MAX_FILE_CHARS {
        content.to_string()
    } else {
        let mut end = MAX_FILE_CHARS;
        // Don't cut in the middle of a UTF-8 character
        while !content.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        format!(
            "{}\n\n[... truncated at {} chars ...]",
            &content[..end],
            MAX_FILE_CHARS
        )
    }
}

/// Safely truncate a line snippet to `max_chars` without panicking on multi-byte UTF-8.
fn safe_snippet(line: &str, max_chars: usize) -> String {
    if line.len() <= max_chars {
        return line.to_string();
    }
    // Find the last char boundary at or before max_chars
    let end = line
        .char_indices()
        .take_while(|(i, _)| *i <= max_chars)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    format!("{}...", &line[..end])
}

// ── File Classification ─────────────────────────────────────────────

/// Classify all files, using two-pass logic:
/// 1. Find SKILL.md → extract @referenced .md files
/// 2. Route each file to the appropriate agent role
pub fn classify_files(files: &[ScannedFile]) -> Vec<(FileRole, usize)> {
    static MD_REF_RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"@([\w\-./]+\.md)").unwrap());

    // Pass 1: build the set of "instruction .md" file paths
    let mut instruction_mds: HashSet<String> = HashSet::new();
    instruction_mds.insert("SKILL.md".to_string());

    if let Some(skill_md) = files.iter().find(|f| f.file_name() == "SKILL.md") {
        let re = &*MD_REF_RE;
        for cap in re.captures_iter(&skill_md.content) {
            if let Some(m) = cap.get(1) {
                instruction_mds.insert(m.as_str().to_string());
            }
        }
    }

    // Pass 2: classify each file
    files
        .iter()
        .enumerate()
        .map(|(idx, file)| {
            let role = classify_single(file, &instruction_mds);
            (role, idx)
        })
        .collect()
}

fn classify_single(file: &ScannedFile, instruction_mds: &HashSet<String>) -> FileRole {
    // Check if this file (by relative path or filename) is in the instruction set
    if instruction_mds.contains(&file.relative_path) || instruction_mds.contains(file.file_name()) {
        return FileRole::Skill;
    }

    // Scripts directory → always Script
    let path_lower = file.relative_path.to_lowercase();
    if path_lower.starts_with("scripts/")
        || path_lower.starts_with("bin/")
        || path_lower.starts_with("script/")
    {
        return FileRole::Script;
    }

    // By extension
    match file.extension().to_lowercase().as_str() {
        "sh" | "bash" | "zsh" | "fish" | "py" | "js" | "ts" | "bat" | "ps1" | "rb" | "lua"
        | "pl" | "r" => FileRole::Script,
        "json" | "yaml" | "yml" | "toml" | "txt" | "cfg" | "ini" => FileRole::Resource,
        "md" => FileRole::General, // non-SKILL.md markdown
        _ => {
            // Shebang detection
            if file.content.starts_with("#!") {
                FileRole::Script
            } else {
                FileRole::General
            }
        }
    }
}

#[allow(dead_code)]
fn base_prompt_for_role(role: FileRole) -> &'static str {
    match role {
        FileRole::Skill => SKILL_AGENT_PROMPT,
        FileRole::Script => SCRIPT_AGENT_PROMPT,
        FileRole::Resource => RESOURCE_AGENT_PROMPT,
        FileRole::General => GENERAL_AGENT_PROMPT,
    }
}

fn language_display_name(code: &str) -> &str {
    match code {
        "zh-CN" => "Simplified Chinese",
        "zh-TW" => "Traditional Chinese",
        "ja" => "Japanese",
        "ko" => "Korean",
        "en" => "English",
        other => other,
    }
}

#[allow(dead_code)]
fn build_role_prompt(role: FileRole, target_lang: &str) -> String {
    base_prompt_for_role(role).replace("{{TARGET_LANGUAGE}}", language_display_name(target_lang))
}

// ── Smart Scan Triage ───────────────────────────────────────────────

/// Determine whether a file needs AI analysis in Smart Scan mode.
/// Returns true if the file contains suspicious signals worth examining.
fn needs_ai_analysis(file: &ScannedFile, role: FileRole) -> bool {
    // Rule 1: SKILL.md and referenced instruction docs are always high-risk
    if role == FileRole::Skill {
        return true;
    }

    // Rule 2: All executable scripts need semantic review
    if role == FileRole::Script {
        return true;
    }

    // Rule 3: Content heuristic signal scanning
    let content_lower = file.content.to_lowercase();

    // Network behavior signals
    let network_signals = [
        "http://",
        "https://",
        "curl",
        "wget",
        "fetch(",
        "requests.",
        "urllib",
        "httpx",
        "axios",
        "net/http",
        "reqwest",
        "aiohttp",
    ];

    // Process execution signals
    let exec_signals = [
        "exec(",
        "eval(",
        "spawn(",
        "system(",
        "subprocess",
        "child_process",
        "os.popen",
        "runtime.getruntime",
        "processbuilder",
    ];

    // File system write signals
    let fs_write_signals = [
        "fs.write",
        "writefile",
        ">>",
        "> /",
        "open(",
        "with open",
        "file.write",
        "std::fs::write",
    ];

    // Encoding/obfuscation signals
    let obfuscation_signals = [
        "atob(",
        "btoa(",
        "buffer.from(",
        "base64",
        "decode(",
        "encode(",
        "\\x",
        "\\u00",
    ];

    // Environment variable / secret sniffing
    let secret_signals = [
        "process.env",
        "os.environ",
        "env::var",
        "api_key",
        "secret",
        "token",
        "password",
        "private_key",
        ".env",
    ];

    let all_signals: Vec<&[&str]> = vec![
        &network_signals,
        &exec_signals,
        &fs_write_signals,
        &obfuscation_signals,
        &secret_signals,
    ];

    for group in &all_signals {
        if group.iter().any(|sig| content_lower.contains(sig)) {
            return true;
        }
    }

    // Rule 4+5: Resource/General files with no signal hit are safe to skip
    false
}

// ── Unified Chunk Engine ────────────────────────────────────────────

/// Pack files into chunks for batched AI analysis.
/// Files are never split across chunks; oversized files get their own chunk.
fn build_chunks(
    files: &[ScannedFile],
    classifications: &[(FileRole, usize)],
    chunk_char_limit: usize,
) -> Vec<(String, Vec<String>)> {
    let mut chunks: Vec<(String, Vec<String>)> = Vec::new();
    let mut current_chunk = String::new();
    let mut current_paths: Vec<String> = Vec::new();

    for (role, idx) in classifications {
        let file = &files[*idx];
        let file_block = format!(
            "--- FILE: {} (role: {}) ---\n{}\n--- END FILE ---\n\n",
            file.relative_path,
            role.as_label(),
            file.content
        );

        // If current chunk would overflow, finalize it
        if !current_chunk.is_empty() && current_chunk.len() + file_block.len() > chunk_char_limit {
            chunks.push((
                std::mem::take(&mut current_chunk),
                std::mem::take(&mut current_paths),
            ));
        }

        current_chunk.push_str(&file_block);
        current_paths.push(file.relative_path.clone());
    }

    if !current_chunk.is_empty() {
        chunks.push((current_chunk, current_paths));
    }

    chunks
}

/// Estimate scan workload without calling AI.
/// `estimated_api_calls` includes the per-skill aggregator call when AI is used.
pub fn estimate_scan(
    files: &[ScannedFile],
    classifications: &[(FileRole, usize)],
    scan_mode: ScanMode,
    chunk_char_limit: usize,
) -> ScanEstimate {
    let ai_eligible: Vec<(FileRole, usize)> = match scan_mode {
        ScanMode::Static => vec![],
        ScanMode::Smart => classifications
            .iter()
            .filter(|(role, idx)| needs_ai_analysis(&files[*idx], *role))
            .cloned()
            .collect(),
        ScanMode::Deep => classifications.to_vec(),
    };

    let estimated_total_chars: usize = ai_eligible
        .iter()
        .map(|(_, idx)| files[*idx].content.len())
        .sum();

    let estimated_chunks = if ai_eligible.is_empty() {
        0
    } else {
        build_chunks(files, &ai_eligible, chunk_char_limit).len()
    };

    let estimated_api_calls = if estimated_chunks > 0 {
        estimated_chunks + 1
    } else {
        0
    };

    ScanEstimate {
        total_files: files.len(),
        ai_eligible_files: ai_eligible.len(),
        estimated_chunks,
        estimated_api_calls,
        estimated_total_chars,
    }
}

/// Build the system prompt for a chunk batch analysis.
fn build_chunk_prompt(
    skill_name: &str,
    chunk_num: usize,
    total_chunks: usize,
    target_lang: &str,
) -> String {
    CHUNK_BATCH_PROMPT
        .replace("{{SKILL_NAME}}", skill_name)
        .replace("{{CHUNK_NUM}}", &chunk_num.to_string())
        .replace("{{TOTAL_CHUNKS}}", &total_chunks.to_string())
        .replace("{{TARGET_LANGUAGE}}", language_display_name(target_lang))
}

/// Parse the AI response for a chunk, validating paths against expected files.
fn parse_chunk_response(
    response: &str,
    expected_paths: &[String],
    log_ctx: &SkillScanLogCtx,
) -> Result<Vec<FileScanResult>> {
    let json_str = extract_json(response);
    let parsed: serde_json::Value = serde_json::from_str(&json_str)?;

    let files_arr = parsed
        .get("files")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("AI response missing 'files' array"))?;

    let mut results: Vec<FileScanResult> = Vec::new();
    let mut seen_paths: HashSet<String> = HashSet::new();

    for item in files_arr {
        let path = match item.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => continue,
        };

        // Validate: path must be in our expected list
        if !expected_paths.contains(&path.to_string()) {
            log_ctx.log(
                "chunk-parse-warn",
                format!("AI returned unexpected path: {}", path),
            );
            continue;
        }

        seen_paths.insert(path.to_string());

        let file_risk = item
            .get("file_risk")
            .and_then(|v| v.as_str())
            .map(RiskLevel::from_str_loose)
            .unwrap_or(RiskLevel::Low);

        let findings: Vec<AiFinding> = item
            .get("findings")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|f| {
                        Some(AiFinding {
                            category: f.get("category")?.as_str()?.to_string(),
                            severity: f
                                .get("severity")
                                .and_then(|v| v.as_str())
                                .map(RiskLevel::from_str_loose)
                                .unwrap_or(RiskLevel::Low),
                            file_path: path.to_string(),
                            description: f.get("description")?.as_str()?.to_string(),
                            evidence: f
                                .get("evidence")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            recommendation: f
                                .get("recommendation")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        results.push(FileScanResult {
            file_path: path.to_string(),
            role: FileRole::General, // Role not critical in chunk results
            findings,
            file_risk,
            tokens_hint: 0,
        });
    }

    // Fill in missing paths with conservative Low risk
    for expected in expected_paths {
        if !seen_paths.contains(expected) {
            log_ctx.log(
                "chunk-parse-missing",
                format!("AI did not return analysis for: {}", expected),
            );
            results.push(FileScanResult {
                file_path: expected.clone(),
                role: FileRole::General,
                findings: vec![],
                file_risk: RiskLevel::Low, // Conservative: unanalyzed ≠ safe
                tokens_hint: 0,
            });
        }
    }

    Ok(results)
}

/// Check whether an AI API error is deterministic (will never succeed on retry).
/// These include context window limits, invalid request params, auth errors, etc.
fn is_deterministic_api_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_lowercase();
    // HTTP 400 class errors with specific API rejection reasons
    if msg.contains("400 bad request") || msg.contains("400 —") {
        // Context window exceeded — input is too large, retry is pointless
        if msg.contains("context window exceeds limit")
            || msg.contains("context_length_exceeded")
            || msg.contains("maximum context length")
            || msg.contains("too many tokens")
            || msg.contains("token limit")
        {
            return true;
        }
        // Generic invalid request — structural issue with the payload
        if msg.contains("invalid_request_error") || msg.contains("invalid request") {
            return true;
        }
    }
    // Authentication errors — retrying won't fix bad credentials
    if msg.contains("401") || msg.contains("403") || msg.contains("authentication") {
        return true;
    }
    // Model not found
    if msg.contains("404") && msg.contains("model") {
        return true;
    }
    false
}

/// Analyze a single chunk with retry logic and exponential backoff.
/// Deterministic errors (context window exceeded, invalid request) bail immediately
/// without wasting time on retries.
async fn analyze_chunk_with_retry(
    config: &AiConfig,
    chunk_content: &str,
    expected_paths: &[String],
    skill_name: &str,
    chunk_num: usize,
    total_chunks: usize,
    log_ctx: &SkillScanLogCtx,
) -> Result<Vec<FileScanResult>> {
    let mut last_error = None;
    let system_prompt =
        build_chunk_prompt(skill_name, chunk_num, total_chunks, &config.target_language);

    for attempt in 0..=CHUNK_MAX_RETRIES {
        if attempt > 0 {
            let delay = CHUNK_RETRY_DELAY_MS * (attempt as u64);
            log_ctx.log(
                "chunk-retry",
                format!(
                    "chunk={}/{} attempt={} delay={}ms",
                    chunk_num,
                    total_chunks,
                    attempt + 1,
                    delay
                ),
            );
            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
        }

        let resolved = super::ai_provider::resolve_scan_params(config);
        match chat_completion_capped(
            config,
            &system_prompt,
            chunk_content,
            resolved.scan_max_response_tokens,
        )
        .await
        {
            Ok(response) => {
                log_ctx.log(
                    "chunk-response",
                    format!(
                        "chunk={}/{} chars={} preview=\"{}\"",
                        chunk_num,
                        total_chunks,
                        response.len(),
                        truncate_for_log(&response, 200)
                    ),
                );
                match parse_chunk_response(&response, expected_paths, log_ctx) {
                    Ok(results) => return Ok(results),
                    Err(parse_err) => {
                        log_ctx.log(
                            "chunk-parse-fail",
                            format!("chunk={}/{} error={}", chunk_num, total_chunks, parse_err),
                        );
                        last_error = Some(parse_err);
                        continue;
                    }
                }
            }
            Err(api_err) => {
                log_ctx.log(
                    "chunk-api-fail",
                    format!("chunk={}/{} error={}", chunk_num, total_chunks, api_err),
                );
                // Fast-fail on deterministic errors — retrying is guaranteed useless
                if is_deterministic_api_error(&api_err) {
                    log_ctx.log(
                        "chunk-skip-retry",
                        format!(
                            "chunk={}/{} reason=deterministic_error (no retry)",
                            chunk_num, total_chunks
                        ),
                    );
                    return Err(api_err);
                }
                last_error = Some(api_err);
                continue;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Chunk analysis failed after retries")))
}

// ── Static Pattern Scan ─────────────────────────────────────────────

struct PatternDef {
    id: &'static str,
    regex: &'static str,
    severity: RiskLevel,
    description: &'static str,
}

const STATIC_PATTERNS: &[PatternDef] = &[
    PatternDef {
        id: "curl_pipe_sh",
        regex: r"curl\s+[^\|]+\|\s*(sh|bash|zsh)",
        severity: RiskLevel::Critical,
        description: "Remote script piping: curl output piped to shell",
    },
    PatternDef {
        id: "wget_pipe_sh",
        regex: r"wget\s+[^\|]+\|\s*(sh|bash|zsh)",
        severity: RiskLevel::Critical,
        description: "Remote script piping: wget output piped to shell",
    },
    PatternDef {
        id: "base64_decode_exec",
        regex: r"base64\s+(-d|--decode)\s*\|",
        severity: RiskLevel::High,
        description: "Base64 decode piped to execution",
    },
    PatternDef {
        id: "eval_fetch",
        regex: r"eval\s*\(\s*(fetch|require|import)\s*\(",
        severity: RiskLevel::Critical,
        description: "Dynamic code execution from remote source",
    },
    PatternDef {
        id: "exec_requests",
        regex: r"exec\s*\(\s*requests\.(get|post)",
        severity: RiskLevel::Critical,
        description: "Python exec() with HTTP request",
    },
    PatternDef {
        id: "sensitive_ssh",
        regex: r"~/\.ssh/|~/.ssh/|\.ssh/id_|\.ssh/authorized_keys|\.ssh/config",
        severity: RiskLevel::High,
        description: "Access to SSH keys or config",
    },
    PatternDef {
        id: "sensitive_aws",
        regex: r"~/\.aws/|~/.aws/|\.aws/credentials|\.aws/config",
        severity: RiskLevel::High,
        description: "Access to AWS credentials",
    },
    PatternDef {
        id: "sensitive_env",
        regex: r"(?i)(cat|read|source|load)\s+.*\.env\b",
        severity: RiskLevel::Medium,
        description: "Reading .env file (may contain secrets)",
    },
    PatternDef {
        id: "sensitive_etc_passwd",
        regex: r"/etc/passwd|/etc/shadow",
        severity: RiskLevel::High,
        description: "Access to system password files",
    },
    PatternDef {
        id: "sensitive_gnupg",
        regex: r"~/\.gnupg/|~/.gnupg/",
        severity: RiskLevel::High,
        description: "Access to GPG keys",
    },
    PatternDef {
        id: "npm_global_install",
        regex: r"npm\s+install\s+(-g|--global)",
        severity: RiskLevel::Medium,
        description: "Global npm package installation",
    },
    PatternDef {
        id: "pip_install",
        regex: r"pip3?\s+install\s",
        severity: RiskLevel::Low,
        description: "Python package installation",
    },
    PatternDef {
        id: "unicode_bidi",
        regex: r"[\u{202A}-\u{202E}\u{2066}-\u{2069}]",
        severity: RiskLevel::High,
        description: "Unicode bidirectional control character (potential text spoofing)",
    },
    PatternDef {
        id: "reverse_shell",
        regex: r"(?i)(nc|ncat|netcat)\s+(-e|--exec|-c)",
        severity: RiskLevel::Critical,
        description: "Potential reverse shell via netcat",
    },
    PatternDef {
        id: "bash_reverse",
        regex: r"bash\s+-i\s+>&\s*/dev/tcp/",
        severity: RiskLevel::Critical,
        description: "Bash reverse shell via /dev/tcp",
    },
    PatternDef {
        id: "modify_shell_rc",
        regex: r">>?\s*~/?\.(bashrc|zshrc|profile|bash_profile)",
        severity: RiskLevel::High,
        description: "Modifying shell startup config for persistence",
    },
    PatternDef {
        id: "cron_persistence",
        regex: r"crontab\s+(-e|-l|-r)|/etc/cron",
        severity: RiskLevel::High,
        description: "Cron job manipulation for persistence",
    },
];

/// Run static pattern matching on all files (zero AI cost).
/// All regex patterns (including base64) are compiled once via LazyLock.
pub fn static_pattern_scan(files: &[ScannedFile]) -> Vec<StaticFinding> {
    static COMPILED_PATTERNS: std::sync::LazyLock<Vec<(&'static PatternDef, Regex)>> =
        std::sync::LazyLock::new(|| {
            STATIC_PATTERNS
                .iter()
                .filter_map(|p| Regex::new(p.regex).ok().map(|re| (p, re)))
                .collect()
        });
    static B64_RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"[A-Za-z0-9+/]{100,}={0,3}").unwrap());

    let compiled = &*COMPILED_PATTERNS;
    let b64_re = &*B64_RE;

    let mut findings = Vec::new();

    // Single pass: check both static patterns and base64 on each line
    for file in files {
        for (line_number, line) in file.content.lines().enumerate() {
            for (pattern, re) in compiled {
                if re.is_match(line) {
                    let snippet = safe_snippet(line, SNIPPET_MAX_CHARS);
                    findings.push(StaticFinding {
                        file_path: file.relative_path.clone(),
                        line_number: line_number + 1,
                        pattern_id: pattern.id.to_string(),
                        snippet,
                        severity: pattern.severity,
                        description: pattern.description.to_string(),
                    });
                }
            }
            // Base64 check in the same pass (was a separate iteration before)
            if b64_re.is_match(line) {
                findings.push(StaticFinding {
                    file_path: file.relative_path.clone(),
                    line_number: line_number + 1,
                    pattern_id: "long_base64".to_string(),
                    snippet: safe_snippet(line, SNIPPET_MAX_CHARS),
                    severity: RiskLevel::Medium,
                    description: "Long base64-encoded string (may conceal payload)".to_string(),
                });
            }
        }
    }

    findings
}

// ── AI Worker (per-file analysis — retained for tests + fallback) ───

/// Analyze a single file using the role-appropriate AI prompt.
/// Each call creates a completely fresh AI context.
#[allow(dead_code)]
async fn analyze_single_file(
    config: &AiConfig,
    file: &ScannedFile,
    role: FileRole,
    log_ctx: &SkillScanLogCtx,
) -> Result<FileScanResult> {
    let system_prompt = build_role_prompt(role, &config.target_language);
    let user_content = format!(
        "File: `{}`\nRole classification: {}\n\n```\n{}\n```",
        file.relative_path, role, file.content
    );

    log_ctx.log(
        "worker-start",
        format!(
            "file={} role={} chars={}",
            file.relative_path,
            role,
            file.content.len()
        ),
    );

    let response = chat_completion(config, &system_prompt, &user_content).await?;

    log_ctx.log(
        "worker-response",
        format!(
            "file={} chars={} preview=\"{}\"",
            file.relative_path,
            response.len(),
            truncate_for_log(&response, 320)
        ),
    );

    parse_file_scan_result(&file.relative_path, role, &response, log_ctx)
}

#[allow(dead_code)]
fn parse_file_scan_result(
    file_path: &str,
    role: FileRole,
    response: &str,
    log_ctx: &SkillScanLogCtx,
) -> Result<FileScanResult> {
    // Extract JSON from the response (may be wrapped in markdown fences)
    let json_str = extract_json(response);

    let parsed: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            log_ctx.log(
                "worker-parse-error",
                format!(
                    "file={} error=\"{}\"",
                    file_path,
                    truncate_for_log(&e.to_string(), 220)
                ),
            );
            return Err(anyhow::anyhow!(
                "AI returned invalid JSON for '{}': {}",
                file_path,
                e
            ));
        }
    };

    let file_risk = parsed
        .get("file_risk")
        .and_then(|v| v.as_str())
        .map(RiskLevel::from_str_loose)
        .unwrap_or(RiskLevel::Low);

    let findings: Vec<AiFinding> = parsed
        .get("findings")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    Some(AiFinding {
                        category: item.get("category")?.as_str()?.to_string(),
                        severity: item
                            .get("severity")
                            .and_then(|v| v.as_str())
                            .map(RiskLevel::from_str_loose)
                            .unwrap_or(RiskLevel::Low),
                        file_path: file_path.to_string(),
                        description: item.get("description")?.as_str()?.to_string(),
                        evidence: item
                            .get("evidence")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        recommendation: item
                            .get("recommendation")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(FileScanResult {
        file_path: file_path.to_string(),
        role,
        findings,
        file_risk,
        tokens_hint: json_str.len(),
    })
}

// ── AI Aggregator ───────────────────────────────────────────────────

/// Aggregate all worker findings + static findings into a final assessment.
async fn aggregate_findings(
    config: &AiConfig,
    skill_name: &str,
    static_findings: &[StaticFinding],
    file_results: &[FileScanResult],
    log_ctx: &SkillScanLogCtx,
    ai_semaphore: &Semaphore,
) -> Result<(RiskLevel, String)> {
    // Build the user content with all findings
    let mut content = format!("# Skill: {}\n\n", skill_name);

    if !static_findings.is_empty() {
        content.push_str("## Static Pattern Scan Findings\n\n");
        for f in static_findings {
            content.push_str(&format!(
                "- [{}] {} (line {}) in `{}`: {}\n",
                format!("{:?}", f.severity),
                f.pattern_id,
                f.line_number,
                f.file_path,
                f.description,
            ));
        }
        content.push('\n');
    }

    content.push_str("## AI File Analysis Results\n\n");
    for result in file_results {
        content.push_str(&format!(
            "### `{}` (role: {}, risk: {:?})\n",
            result.file_path, result.role, result.file_risk
        ));
        if result.findings.is_empty() {
            content.push_str("No issues found.\n\n");
        } else {
            for f in &result.findings {
                content.push_str(&format!(
                    "- [{:?}] {}: {}\n  Evidence: {}\n",
                    f.severity, f.category, f.description, f.evidence,
                ));
            }
            content.push('\n');
        }
    }

    let lang_display = language_display_name(&config.target_language);

    let system_prompt = AGGREGATOR_PROMPT
        .replace("{{SKILL_NAME}}", skill_name)
        .replace("{{TARGET_LANGUAGE}}", lang_display);

    log_ctx.log(
        "aggregator-start",
        format!(
            "payload_chars={} static_findings={} ai_file_results={}",
            content.len(),
            static_findings.len(),
            file_results.len()
        ),
    );

    let _permit = ai_semaphore
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!("AI semaphore error: {}", e))?;

    let resolved = super::ai_provider::resolve_scan_params(config);
    let agg_max_tokens = resolved.scan_max_response_tokens.min(4096);
    let response = chat_completion_capped(config, &system_prompt, &content, agg_max_tokens).await?;

    log_ctx.log(
        "aggregator-response",
        format!(
            "chars={} preview=\"{}\"",
            response.len(),
            truncate_for_log(&response, 360)
        ),
    );

    let json_str = extract_json(&response);
    let parsed: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            log_ctx.log(
                "aggregator-parse-error",
                format!("error=\"{}\"", truncate_for_log(&e.to_string(), 220)),
            );
            return Err(anyhow::anyhow!("AI summary returned invalid JSON: {}", e));
        }
    };

    let risk_level = parsed
        .get("risk_level")
        .and_then(|v| v.as_str())
        .map(RiskLevel::from_str_loose)
        .unwrap_or(RiskLevel::Low);

    let summary = parsed
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("Scan complete.")
        .to_string();

    log_ctx.log(
        "aggregator-result",
        format!(
            "risk={} summary=\"{}\"",
            risk_label(risk_level),
            truncate_for_log(&summary, 260)
        ),
    );

    Ok((risk_level, summary))
}

// ── Main Scan Orchestrator ──────────────────────────────────────────

/// Prepare a skill scan up to, but not including, chunk AI execution.
pub(crate) async fn prepare_skill_scan<F>(
    config: &AiConfig,
    skill_name: &str,
    skill_dir: &Path,
    scan_mode: ScanMode,
    on_progress: Option<&F>,
    pre_collected: Option<(Vec<ScannedFile>, String)>,
) -> Result<PreparedSkillScan>
where
    F: Fn(&str, Option<&str>),
{
    let scan_start = std::time::Instant::now();
    let log_ctx = SkillScanLogCtx::new(skill_name);
    let actual_mode = if scan_mode.requires_ai() && config.enabled {
        scan_mode
    } else {
        ScanMode::Static
    };
    let run_ai = actual_mode.requires_ai();
    log_ctx.log(
        "start",
        format!(
            "dir={} requested_mode={} actual_mode={} ai_enabled={}",
            skill_dir.display(),
            scan_mode.label(),
            actual_mode.label(),
            config.enabled
        ),
    );

    // ── 1. Collect files + compute content hash ─────────────────────
    // Reuse pre-collected data if available (avoids double I/O when
    // the caller already collected files for cache checking).
    let (files, content_hash) = pre_collected.unwrap_or_else(|| collect_scannable_files(skill_dir));
    let total_chars: usize = files.iter().map(|f| f.content.len()).sum();
    log_ctx.log(
        "collect",
        format!(
            "files={} total_chars={} hash={}",
            files.len(),
            total_chars,
            short_hash(&content_hash)
        ),
    );
    if let Some(cb) = on_progress {
        cb("collect", None);
    }

    let classifications = classify_files(&files);
    for (role, idx) in &classifications {
        log_ctx.log(
            "classify",
            format!("file={} role={}", files[*idx].relative_path, role),
        );
    }
    log_ctx.log(
        "classify-summary",
        format!("role_counts=\"{}\"", file_role_counts(&classifications)),
    );

    // ── 3. Static pattern scan ──────────────────────────────────────
    if let Some(cb) = on_progress {
        cb("static", None);
    }
    let static_findings = static_pattern_scan(&files);
    log_ctx.log("static", format!("findings={}", static_findings.len()));

    // ── 4. AI analysis (chunk-based for Smart/Deep) ─────────────────
    let mut cached_file_results: Vec<FileScanResult> = Vec::new();
    let mut cached_file_hits: usize = 0;
    let mut ai_files_analyzed: usize = 0;
    let mut chunks: Vec<PreparedChunk> = Vec::new();

    if run_ai && config.enabled {
        // Determine which files go to AI
        let ai_eligible: Vec<(FileRole, usize)> = match actual_mode {
            ScanMode::Smart => {
                if let Some(cb) = on_progress {
                    cb("triage", None);
                }
                let eligible: Vec<(FileRole, usize)> = classifications
                    .iter()
                    .filter(|(role, idx)| needs_ai_analysis(&files[*idx], *role))
                    .cloned()
                    .collect();
                let skipped = classifications.len() - eligible.len();
                log_ctx.log(
                    "triage",
                    format!(
                        "eligible={} skipped={} total={}",
                        eligible.len(),
                        skipped,
                        classifications.len()
                    ),
                );
                eligible
            }
            ScanMode::Deep => {
                log_ctx.log("deep-mode", format!("all_files={}", classifications.len()));
                classifications.clone()
            }
            ScanMode::Static => vec![], // unreachable due to run_ai check
        };

        ai_files_analyzed = ai_eligible.len();

        if ai_eligible.is_empty() {
            log_ctx.log("ai-skip", "No files eligible for AI analysis after triage");
        } else {
            // Check file-level cache before sending to AI
            let mode_label = actual_mode.label();
            let mode_key = cache_scan_mode_key(mode_label, &config.target_language);
            let (cached_hits, uncached_classifications) = partition_cached_files(
                &files,
                &ai_eligible,
                &mode_key,
                &config.target_language,
                &log_ctx,
            );

            // Merge cached results immediately
            cached_file_hits = cached_hits.len();
            cached_file_results.extend(cached_hits);

            if uncached_classifications.is_empty() {
                log_ctx.log(
                    "file-cache-complete",
                    "All eligible files resolved from cache",
                );
            } else {
                // Build chunks only for uncached files
                let resolved = super::ai_provider::resolve_scan_params(config);
                let chunk_limit = resolved.chunk_char_limit;
                let raw_chunks = build_chunks(&files, &uncached_classifications, chunk_limit);
                let total_chunks = raw_chunks.len();
                chunks = raw_chunks
                    .into_iter()
                    .enumerate()
                    .map(|(chunk_idx, (chunk_content, chunk_paths))| PreparedChunk {
                        chunk_num: chunk_idx + 1,
                        total_chunks,
                        chunk_content,
                        chunk_paths,
                    })
                    .collect();
                log_ctx.log(
                    "chunks-built",
                    format!(
                        "chunks={} chunk_limit={} uncached_files={} (cached={})",
                        chunks.len(),
                        chunk_limit,
                        uncached_classifications.len(),
                        ai_eligible.len() - uncached_classifications.len()
                    ),
                );
            }
        }
    } else {
        let reason = if !scan_mode.requires_ai() {
            "mode=static"
        } else {
            "ai_provider_disabled"
        };
        log_ctx.log("ai-skip", format!("reason={}", reason));
    }

    Ok(PreparedSkillScan {
        skill_name: skill_name.to_string(),
        files,
        classifications,
        static_findings,
        cached_file_results,
        cached_file_hits,
        chunks,
        content_hash,
        total_chars_analyzed: total_chars,
        actual_mode,
        run_ai,
        ai_files_analyzed,
        log_ctx,
        scan_start,
    })
}

pub(crate) async fn analyze_prepared_chunk(
    config: &AiConfig,
    skill_name: &str,
    chunk: &PreparedChunk,
    log_ctx: &SkillScanLogCtx,
) -> Result<Vec<FileScanResult>> {
    log_ctx.log(
        "chunk-start",
        format!(
            "chunk={}/{} files={} chars={}",
            chunk.chunk_num,
            chunk.total_chunks,
            chunk.chunk_paths.len(),
            chunk.chunk_content.len()
        ),
    );

    match analyze_chunk_with_retry(
        config,
        &chunk.chunk_content,
        &chunk.chunk_paths,
        skill_name,
        chunk.chunk_num,
        chunk.total_chunks,
        log_ctx,
    )
    .await
    {
        Ok(results) => {
            log_ctx.log(
                "chunk-done",
                format!(
                    "chunk={}/{} results={}",
                    chunk.chunk_num,
                    chunk.total_chunks,
                    results.len()
                ),
            );
            Ok(results)
        }
        Err(err) => {
            log_ctx.log(
                "chunk-failed",
                format!(
                    "chunk={}/{} error=\"{}\"",
                    chunk.chunk_num,
                    chunk.total_chunks,
                    truncate_for_log(&err.to_string(), 260)
                ),
            );
            Err(err)
        }
    }
}

pub(crate) async fn finalize_prepared_skill<F>(
    config: &AiConfig,
    prepared: PreparedSkillScan,
    fresh_file_results: Vec<FileScanResult>,
    worker_failures: Vec<(String, FileRole, String)>,
    scan_cancelled: bool,
    ai_semaphore: &Semaphore,
    on_progress: Option<&F>,
) -> Result<SecurityScanResult>
where
    F: Fn(&str, Option<&str>),
{
    let PreparedSkillScan {
        skill_name,
        files,
        classifications,
        static_findings,
        cached_file_results,
        cached_file_hits,
        chunks,
        content_hash,
        total_chars_analyzed,
        actual_mode,
        run_ai,
        ai_files_analyzed,
        log_ctx,
        scan_start,
    } = prepared;

    if files.is_empty() {
        log_ctx.log("skip", "No scannable files found");
        let result = SecurityScanResult {
            skill_name: skill_name.to_string(),
            scanned_at: chrono::Utc::now().to_rfc3339(),
            tree_hash: Some(content_hash.clone()),
            scan_mode: actual_mode.label().to_string(),
            scanner_version: CACHE_SCHEMA_VERSION.to_string(),
            target_language: config.target_language.clone(),
            risk_level: RiskLevel::Safe,
            static_findings: vec![],
            ai_findings: vec![],
            summary: "No scannable files found.".to_string(),
            files_scanned: 0,
            total_chars_analyzed: 0,
            incomplete: false,
            ai_files_analyzed: 0,
            chunks_used: 0,
        };
        save_to_cache(&result)?;
        let elapsed = scan_start.elapsed();
        log_live_scan_details(
            &log_ctx,
            &result,
            &[],
            &[],
            &[],
            &content_hash,
            run_ai,
            config.enabled,
            elapsed,
        );
        log_ctx.log("complete", format!("elapsed_ms={}", elapsed.as_millis()));
        return Ok(result);
    }

    let mut file_results = cached_file_results;
    let failed_paths: HashSet<&str> = worker_failures
        .iter()
        .map(|(path, _, _)| path.as_str())
        .collect();
    let cacheable_results: Vec<FileScanResult> = fresh_file_results
        .iter()
        .filter(|result| !failed_paths.contains(result.file_path.as_str()))
        .cloned()
        .collect();
    if !cacheable_results.is_empty() {
        let mode_key = cache_scan_mode_key(actual_mode.label(), &config.target_language);
        save_file_scan_results(
            &files,
            &classifications,
            &cacheable_results,
            &mode_key,
            &config.target_language,
        );
    }
    file_results.extend(fresh_file_results);

    if !worker_failures.is_empty() {
        log_ctx.log(
            "chunks-partial-failure",
            format!(
                "failed_chunks={} total_chunks={}",
                worker_failures.len(),
                chunks.len()
            ),
        );
    }

    log_ctx.log(
        "ai-complete",
        format!(
            "file_results={} failures={} chunks_processed={} file_cache_hits={}",
            file_results.len(),
            worker_failures.len(),
            chunks.len(),
            cached_file_hits
        ),
    );

    let ai_findings: Vec<AiFinding> = file_results
        .iter()
        .flat_map(|r| r.findings.clone())
        .collect();

    let analysis_incomplete =
        run_ai && config.enabled && (!worker_failures.is_empty() || scan_cancelled);

    let (final_risk, summary, incomplete) = if scan_cancelled {
        let risk = RiskLevel::max(
            compute_fallback_risk(&static_findings, &file_results),
            RiskLevel::Low,
        );
        (
            risk,
            "Scan was cancelled before all AI chunks completed. Results are partial and may miss issues."
                .to_string(),
            true,
        )
    } else if analysis_incomplete {
        let risk = RiskLevel::max(
            compute_fallback_risk(&static_findings, &file_results),
            RiskLevel::Low,
        );
        (
            risk,
            format!(
                "AI analysis was incomplete for {} file(s). Results may miss issues; manual review is recommended.",
                worker_failures.len()
            ),
            true,
        )
    } else if run_ai && config.enabled && (!ai_findings.is_empty() || !static_findings.is_empty()) {
        // Determine if we can skip the aggregator API call entirely
        let all_ai_safe = file_results
            .iter()
            .all(|r| r.file_risk.severity_ord() == 0 && r.findings.is_empty());
        let fallback_risk = compute_fallback_risk(&static_findings, &file_results);

        if all_ai_safe && static_findings.is_empty() {
            // All files came back safe with no findings — no point calling aggregator
            log_ctx.log("aggregator-skip", "reason=all_safe");
            (RiskLevel::Safe, "No issues found.".to_string(), false)
        } else if all_ai_safe && fallback_risk.severity_ord() <= 1 {
            // AI says everything is safe but there are minor static findings (Low)
            // Skip the aggregator — use a local summary instead
            log_ctx.log(
                "aggregator-skip",
                format!(
                    "reason=low_static_only static_findings={}",
                    static_findings.len()
                ),
            );
            (
                fallback_risk,
                format!(
                    "Static scan found {} pattern match(es). AI analysis found no additional issues.",
                    static_findings.len()
                ),
                false,
            )
        } else if chunks.len() <= 1 && static_findings.is_empty() {
            // Single chunk + no static findings → the chunk result is the full picture
            log_ctx.log("aggregator-skip", "reason=single_chunk_no_static");
            (fallback_risk, "Scan complete.".to_string(), false)
        } else {
            // Real aggregation needed
            if let Some(cb) = on_progress {
                cb("aggregate", None);
            }
            match aggregate_findings(
                config,
                &skill_name,
                &static_findings,
                &file_results,
                &log_ctx,
                ai_semaphore,
            )
            .await
            {
                Ok((risk, sum)) => (risk, sum, false),
                Err(e) => {
                    log_ctx.log(
                        "aggregator-error",
                        format!("error=\"{}\"", truncate_for_log(&e.to_string(), 260)),
                    );
                    let risk = RiskLevel::max(fallback_risk, RiskLevel::Low);
                    (
                        risk,
                        "AI summary generation failed after file analysis. Results may be incomplete; manual review is recommended.".to_string(),
                        true,
                    )
                }
            }
        }
    } else if !static_findings.is_empty() {
        let risk = static_findings
            .iter()
            .fold(RiskLevel::Safe, |acc, f| RiskLevel::max(acc, f.severity));
        (
            risk,
            format!(
                "Static scan found {} pattern match(es). AI analysis not configured.",
                static_findings.len()
            ),
            false,
        )
    } else {
        (RiskLevel::Safe, "No issues found.".to_string(), false)
    };

    let result = SecurityScanResult {
        skill_name: skill_name.to_string(),
        scanned_at: chrono::Utc::now().to_rfc3339(),
        tree_hash: Some(content_hash.clone()),
        scan_mode: actual_mode.label().to_string(),
        scanner_version: CACHE_SCHEMA_VERSION.to_string(),
        target_language: config.target_language.clone(),
        risk_level: final_risk,
        static_findings,
        ai_findings,
        summary,
        files_scanned: files.len(),
        total_chars_analyzed,
        incomplete,
        ai_files_analyzed,
        chunks_used: chunks.len(),
    };

    if result.incomplete {
        log_ctx.log("cache-skip", "result_incomplete=true");
    } else if let Err(e) = save_to_cache(&result) {
        log_ctx.log(
            "cache-error",
            format!("error=\"{}\"", truncate_for_log(&e.to_string(), 260)),
        );
        return Err(e);
    }

    let elapsed = scan_start.elapsed();
    log_live_scan_details(
        &log_ctx,
        &result,
        &classifications,
        &file_results,
        &worker_failures,
        result.tree_hash.as_deref().unwrap_or(&content_hash),
        run_ai,
        config.enabled,
        elapsed,
    );
    log_ctx.log(
        "complete",
        format!(
            "risk={} elapsed_ms={} mode={} ai_files={} chunks={}",
            risk_label(result.risk_level),
            elapsed.as_millis(),
            actual_mode.label(),
            ai_files_analyzed,
            result.chunks_used
        ),
    );

    Ok(result)
}

/// Scan a single skill folder end-to-end.
pub async fn scan_single_skill<F>(
    config: &AiConfig,
    skill_name: &str,
    skill_dir: &Path,
    scan_mode: ScanMode,
    ai_semaphore: Arc<Semaphore>,
    on_progress: Option<&F>,
) -> Result<SecurityScanResult>
where
    F: Fn(&str, Option<&str>),
{
    let prepared =
        prepare_skill_scan(config, skill_name, skill_dir, scan_mode, on_progress, None).await?;
    let mut fresh_file_results: Vec<FileScanResult> = Vec::new();
    let mut worker_failures: Vec<(String, FileRole, String)> = Vec::new();
    let mut scan_cancelled = false;

    if !prepared.chunks.is_empty() {
        if let Some(cb) = on_progress {
            cb("ai-analyze", None);
        }

        let mut join_set: tokio::task::JoinSet<(
            PreparedChunk,
            Result<Vec<FileScanResult>, String>,
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
                let result = analyze_prepared_chunk(&cfg, &skill_name, &chunk, &log_ctx)
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
                        fresh_file_results.push(FileScanResult {
                            file_path: path.clone(),
                            role: FileRole::General,
                            findings: vec![],
                            file_risk: RiskLevel::Low,
                            tokens_hint: 0,
                        });
                        worker_failures.push((path.clone(), FileRole::General, err_msg.clone()));
                    }
                }
            }
        }
    }

    finalize_prepared_skill(
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

fn compute_fallback_risk(
    static_findings: &[StaticFinding],
    file_results: &[FileScanResult],
) -> RiskLevel {
    let static_max = static_findings
        .iter()
        .fold(RiskLevel::Safe, |acc, f| RiskLevel::max(acc, f.severity));
    let ai_max = file_results
        .iter()
        .fold(RiskLevel::Safe, |acc, r| RiskLevel::max(acc, r.file_risk));
    RiskLevel::max(static_max, ai_max)
}

// ── Cache ───────────────────────────────────────────────────────────

fn db_path() -> PathBuf {
    super::paths::data_root().join("security_scan.db")
}

/// Cached SQLite connection for the security scan database.
///
/// The connection is opened once on first use and reused across all
/// cache reads/writes, avoiding the overhead of repeated open +
/// `CREATE TABLE IF NOT EXISTS` calls during batch scans.
static SCAN_DB: std::sync::LazyLock<Mutex<Option<Connection>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));

fn get_db() -> Result<std::sync::MutexGuard<'static, Option<Connection>>> {
    let mut guard = SCAN_DB
        .lock()
        .map_err(|e| anyhow::anyhow!("scan DB mutex poisoned: {}", e))?;
    if guard.is_none() {
        let conn = open_and_migrate_db()?;
        *guard = Some(conn);
    }
    Ok(guard)
}

/// Macro to borrow `&Connection` from a `MutexGuard<Option<Connection>>`.
/// Keeps borrowing ergonomic and avoids repeating the unwrap.
macro_rules! conn_ref {
    ($guard:expr) => {
        $guard
            .as_ref()
            .expect("scan db connection missing after get_db")
    };
}

/// Drop the cached connection so the next `get_db()` reopens at the
/// current `db_path()`.  Used by tests that override `SKILLSTAR_DATA_DIR`.
#[cfg(test)]
fn reset_scan_db() {
    if let Ok(mut guard) = SCAN_DB.lock() {
        *guard = None;
    }
}

/// Open the SQLite database, create tables, and run one-time migrations.
fn open_and_migrate_db() -> Result<Connection> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create db directory")?;
    }
    let conn = Connection::open(&path)?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS scan_cache_v2 (
            skill_name TEXT NOT NULL,
            scan_mode TEXT NOT NULL,
            scanner_version TEXT NOT NULL,
            tree_hash TEXT,
            scanned_at TEXT NOT NULL,
            json_data TEXT NOT NULL,
            PRIMARY KEY (skill_name, scan_mode, scanner_version)
        )",
        (),
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS file_scan_cache (
            content_digest TEXT NOT NULL,
            scan_mode TEXT NOT NULL,
            scanner_version TEXT NOT NULL,
            relative_path TEXT NOT NULL,
            file_risk TEXT NOT NULL,
            findings_json TEXT NOT NULL,
            cached_at TEXT NOT NULL,
            PRIMARY KEY (content_digest, scan_mode, scanner_version)
        )",
        (),
    )?;

    // ── One-time migrations (P3 + P6) ──────────────────────────────
    // Drop legacy v1 table if it lingered from an older schema.
    let _ = conn.execute("DROP TABLE IF EXISTS scan_cache", []);
    // Prune rows from obsolete scanner versions so they don't accumulate.
    let _ = conn.execute(
        "DELETE FROM scan_cache_v2 WHERE scanner_version <> ?1",
        [CACHE_SCHEMA_VERSION],
    );

    Ok(conn)
}

/// Standalone init for tests and `clear_cache` — opens a fresh connection
/// without going through the cached path (avoids poisoned-mutex issues
/// when tests set `SKILLSTAR_DATA_DIR`).
fn init_db() -> Result<Connection> {
    open_and_migrate_db()
}

fn normalize_target_language_for_cache(target_lang: &str) -> String {
    let trimmed = target_lang.trim();
    if trimmed.is_empty() {
        "en".to_string()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

fn cache_scan_mode_key(scan_mode: &str, target_lang: &str) -> String {
    format!(
        "{}::{}",
        scan_mode,
        normalize_target_language_for_cache(target_lang)
    )
}

fn split_cache_scan_mode_key(mode_key: &str) -> (&str, Option<&str>) {
    mode_key
        .split_once("::")
        .map_or((mode_key, None), |(mode, lang)| (mode, Some(lang)))
}

pub fn load_all_cached() -> Vec<SecurityScanResult> {
    let guard = match get_db() {
        Ok(g) => g,
        Err(_) => return vec![],
    };
    let conn = conn_ref!(guard);
    let mut stmt = match conn.prepare(
        "SELECT json_data FROM scan_cache_v2
         WHERE scanner_version = ?1
         ORDER BY scanned_at DESC",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = stmt.query_map([CACHE_SCHEMA_VERSION], |row| row.get::<_, String>(0));
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    if let Ok(mapped) = rows {
        for json_str in mapped.flatten() {
            if let Ok(parsed_result) = serde_json::from_str::<SecurityScanResult>(&json_str) {
                if seen.insert(parsed_result.skill_name.clone()) {
                    results.push(parsed_result);
                }
            }
        }
    }
    results
}

/// Load a cached result for a skill and scan mode, validating the full-content hash.
pub fn load_cached_result(
    skill_name: &str,
    scan_mode_key: &str,
    current_tree_hash: Option<&str>,
) -> Option<SecurityScanResult> {
    let guard = get_db().ok()?;
    let conn = conn_ref!(guard);
    let mut stmt = conn
        .prepare(
            "SELECT tree_hash, json_data FROM scan_cache_v2
             WHERE skill_name = ?1 AND scan_mode = ?2 AND scanner_version = ?3",
        )
        .ok()?;

    let mut rows = stmt
        .query((skill_name, scan_mode_key, CACHE_SCHEMA_VERSION))
        .ok()?;
    let row = rows.next().ok().flatten()?;

    let cached_hash: Option<String> = row.get(0).ok();
    let json_data: String = row.get(1).ok()?;

    // Validate tree_hash if both sides have one
    if let (Some(cached_h), Some(current_h)) = (cached_hash, current_tree_hash) {
        if cached_h != current_h {
            return None; // Hash changed → invalidate
        }
    }

    // Note: TTL validation removed.
    // Secure SHA-256 content hashes natively guarantee that the content hasn't changed.

    serde_json::from_str(&json_data).ok()
}

pub fn try_reuse_cached(
    skill_name: &str,
    requested_mode: ScanMode,
    current_tree_hash: Option<&str>,
    target_language: &str,
) -> Option<SecurityScanResult> {
    let requested_mode_key = cache_scan_mode_key(requested_mode.label(), target_language);
    if let Some(exact) = load_cached_result(skill_name, &requested_mode_key, current_tree_hash) {
        return Some(exact);
    }

    if requested_mode == ScanMode::Smart {
        let deep_mode_key = cache_scan_mode_key(ScanMode::Deep.label(), target_language);
        return load_cached_result(skill_name, &deep_mode_key, current_tree_hash);
    }

    None
}

pub fn save_to_cache(result: &SecurityScanResult) -> Result<()> {
    if result.incomplete {
        return Ok(());
    }

    let guard = get_db()?;
    let conn = conn_ref!(guard);
    let json_data = serde_json::to_string(result)?;
    let scan_mode_key = cache_scan_mode_key(&result.scan_mode, &result.target_language);

    conn.execute(
        "INSERT OR REPLACE INTO scan_cache_v2
         (skill_name, scan_mode, scanner_version, tree_hash, scanned_at, json_data)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (
            &result.skill_name,
            &scan_mode_key,
            &result.scanner_version,
            &result.tree_hash,
            &result.scanned_at,
            &json_data,
        ),
    )?;

    conn.execute(
        "DELETE FROM scan_cache_v2
         WHERE rowid NOT IN (
             SELECT rowid FROM scan_cache_v2
             ORDER BY scanned_at DESC LIMIT ?1
         )",
        [CACHE_MAX_ENTRIES as i64],
    )?;

    Ok(())
}

pub fn clear_cache() -> Result<()> {
    // Delete legacy json file if exists
    let legacy_path = super::paths::data_root().join("security_scan_cache.json");
    if legacy_path.exists() {
        let _ = std::fs::remove_file(legacy_path);
    }

    // Use init_db() (not get_db) so tests with overridden SKILLSTAR_DATA_DIR
    // don't fight with the cached connection from a different env.
    let conn = init_db()?;
    conn.execute("DELETE FROM scan_cache_v2", [])?;
    let _ = conn.execute("DELETE FROM file_scan_cache", []);
    Ok(())
}

pub fn clear_logs() -> Result<()> {
    let runtime_log = log_path();
    if runtime_log.exists() {
        std::fs::remove_file(&runtime_log).with_context(|| {
            format!(
                "Failed to remove security scan runtime log: {}",
                runtime_log.display()
            )
        })?;
    }

    let archive_dir = scan_logs_dir();
    if archive_dir.exists() {
        std::fs::remove_dir_all(&archive_dir).with_context(|| {
            format!(
                "Failed to remove security scan logs directory: {}",
                archive_dir.display()
            )
        })?;
    }
    std::fs::create_dir_all(&archive_dir).with_context(|| {
        format!(
            "Failed to recreate security scan logs directory: {}",
            archive_dir.display()
        )
    })?;
    Ok(())
}

/// Remove a single skill's cached security scan result.
///
/// Called when a skill is updated or reinstalled so its security badge
/// resets to "unscanned" until the user runs a new scan.
pub fn invalidate_skill_cache(skill_name: &str) {
    if let Ok(guard) = get_db() {
        if let Some(conn) = guard.as_ref() {
            let _ = conn.execute(
                "DELETE FROM scan_cache_v2 WHERE skill_name = ?1",
                [skill_name],
            );
        }
    }
}

// ── File-Level Incremental Cache ────────────────────────────────────

/// Scan mode compatibility: Deep cache results can satisfy Smart requests,
/// but not vice versa. Static cache never satisfies AI modes.
fn scan_mode_compatible(cached_mode: &str, requested_mode: &str) -> bool {
    let (cached_base_mode, cached_lang) = split_cache_scan_mode_key(cached_mode);
    let (requested_base_mode, requested_lang) = split_cache_scan_mode_key(requested_mode);

    // Language-scoped cache: only reuse entries from the same target language.
    if let Some(req_lang) = requested_lang {
        if cached_lang != Some(req_lang) {
            return false;
        }
    } else if cached_lang.is_some() {
        return false;
    }

    match requested_base_mode {
        "deep" => cached_base_mode == "deep",
        "smart" => cached_base_mode == "smart" || cached_base_mode == "deep",
        "static" => cached_base_mode == "static",
        _ => cached_base_mode == requested_base_mode,
    }
}

fn file_cache_digest_key(content_digest: &str, role: FileRole, target_lang: &str) -> String {
    format!(
        "{}::{}::{}",
        role.as_label().to_lowercase(),
        normalize_target_language_for_cache(target_lang),
        content_digest
    )
}

/// Load a single file's cached result by its content digest.
fn load_cached_file_result(
    conn: &Connection,
    content_digest: &str,
    role: FileRole,
    scan_mode: &str,
    target_lang: &str,
) -> Option<FileScanResult> {
    let cache_key = file_cache_digest_key(content_digest, role, target_lang);
    // Try exact mode first, then compatible mode (deep satisfies smart)
    let mut stmt = conn
        .prepare(
            "SELECT scan_mode, relative_path, file_risk, findings_json FROM file_scan_cache
             WHERE content_digest = ?1 AND scanner_version = ?2
             ORDER BY CASE scan_mode WHEN 'deep' THEN 0 WHEN 'smart' THEN 1 ELSE 2 END
             LIMIT 10",
        )
        .ok()?;

    let mut rows = stmt
        .query(rusqlite::params![cache_key, CACHE_SCHEMA_VERSION])
        .ok()?;

    while let Ok(Some(row)) = rows.next() {
        let cached_mode: String = row.get(0).ok()?;
        if !scan_mode_compatible(&cached_mode, scan_mode) {
            continue;
        }
        let relative_path: String = row.get(1).ok()?;
        let file_risk_str: String = row.get(2).ok()?;
        let findings_json: String = row.get(3).ok()?;

        let file_risk = RiskLevel::from_str_loose(&file_risk_str);
        let findings: Vec<AiFinding> = serde_json::from_str(&findings_json).unwrap_or_default();

        // LRU touch removed: write-time cached_at is sufficient for
        // eviction ordering.  Removing this UPDATE eliminates a write I/O
        // on every cache read hit.

        return Some(FileScanResult {
            file_path: relative_path,
            role: FileRole::General,
            findings,
            file_risk,
            tokens_hint: 0,
        });
    }

    None
}

/// Save per-file results from a chunk analysis back to the file cache.
fn save_file_scan_results(
    files: &[ScannedFile],
    classifications: &[(FileRole, usize)],
    results: &[FileScanResult],
    scan_mode: &str,
    target_lang: &str,
) {
    let guard = match get_db() {
        Ok(g) => g,
        Err(_) => return,
    };
    let conn = conn_ref!(guard);

    // Wrap all inserts + eviction in a single transaction to avoid N
    // individual fsyncs (was 30x slower for skills with many files).
    let _ = conn.execute_batch("BEGIN");

    let now = chrono::Utc::now().to_rfc3339();
    let file_role_map: HashMap<String, FileRole> = classifications
        .iter()
        .map(|(role, idx)| (files[*idx].relative_path.clone(), *role))
        .collect();

    for result in results {
        // Find the matching ScannedFile to get the content_digest
        let digest = match files.iter().find(|f| f.relative_path == result.file_path) {
            Some(f) => &f.content_digest,
            None => continue,
        };
        let role = file_role_map
            .get(&result.file_path)
            .copied()
            .unwrap_or(FileRole::General);
        let cache_key = file_cache_digest_key(digest, role, target_lang);

        let findings_json =
            serde_json::to_string(&result.findings).unwrap_or_else(|_| "[]".to_string());
        let risk_str = risk_label(result.file_risk);

        let _ = conn.execute(
            "INSERT OR REPLACE INTO file_scan_cache
             (content_digest, scan_mode, scanner_version, relative_path, file_risk, findings_json, cached_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                cache_key,
                scan_mode,
                CACHE_SCHEMA_VERSION,
                &result.file_path,
                risk_str,
                &findings_json,
                &now,
            ],
        );
    }

    // LRU eviction: keep only the most recent FILE_CACHE_MAX_ENTRIES
    let _ = conn.execute(
        "DELETE FROM file_scan_cache
         WHERE rowid NOT IN (
             SELECT rowid FROM file_scan_cache
             ORDER BY cached_at DESC LIMIT ?1
         )",
        [FILE_CACHE_MAX_ENTRIES as i64],
    );

    let _ = conn.execute_batch("COMMIT");
}

/// Partition files into (cached_results, needs_scan_classifications).
/// Returns file results that were found in cache and the classifications
/// of files that still need AI analysis.
fn partition_cached_files(
    files: &[ScannedFile],
    classifications: &[(FileRole, usize)],
    scan_mode: &str,
    target_lang: &str,
    log_ctx: &SkillScanLogCtx,
) -> (Vec<FileScanResult>, Vec<(FileRole, usize)>) {
    let guard = match get_db() {
        Ok(g) => g,
        Err(_) => return (vec![], classifications.to_vec()),
    };
    let conn = conn_ref!(guard);

    let mut cached_results: Vec<FileScanResult> = Vec::new();
    let mut needs_scan: Vec<(FileRole, usize)> = Vec::new();

    for (role, idx) in classifications {
        let file = &files[*idx];
        if let Some(mut cached) =
            load_cached_file_result(&conn, &file.content_digest, *role, scan_mode, target_lang)
        {
            // Update the path in case the file moved but content is identical
            cached.file_path = file.relative_path.clone();
            for finding in &mut cached.findings {
                finding.file_path = file.relative_path.clone();
            }
            cached.role = *role;
            log_ctx.log(
                "file-cache-hit",
                format!(
                    "file={} digest={}",
                    file.relative_path,
                    short_hash(&file.content_digest)
                ),
            );
            cached_results.push(cached);
        } else {
            needs_scan.push((*role, *idx));
        }
    }

    log_ctx.log(
        "file-cache-partition",
        format!(
            "cached={} needs_scan={} total={}",
            cached_results.len(),
            needs_scan.len(),
            classifications.len()
        ),
    );

    (cached_results, needs_scan)
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Extract a JSON object from a response that may have markdown fences.
fn extract_json(response: &str) -> String {
    let trimmed = response.trim();

    // Try to find JSON within code fences
    if let Some(start) = trimmed.find("```json") {
        let after = &trimmed[start + 7..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        // Skip the optional language identifier on the same line
        let after = if let Some(nl) = after.find('\n') {
            &after[nl + 1..]
        } else {
            after
        };
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }

    // Try to find bare JSON object
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return trimmed[start..=end].to_string();
            }
        }
    }

    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::MutexGuard;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock drift")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("skillstar-{}-{}", prefix, stamp));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    struct TestDataRoot {
        _guard: MutexGuard<'static, ()>,
        _dir: TempDir,
    }

    impl TestDataRoot {
        fn new(prefix: &str) -> Self {
            let guard = crate::core::test_env_lock().lock().expect("lock test env");
            let dir = TempDir::new(prefix);
            unsafe {
                std::env::set_var("SKILLSTAR_DATA_DIR", dir.path());
            }
            // Reset the cached DB connection so it reopens at the new data dir
            reset_scan_db();
            clear_cache().expect("clear test cache");
            Self {
                _guard: guard,
                _dir: dir,
            }
        }
    }

    impl Drop for TestDataRoot {
        fn drop(&mut self) {
            let _ = clear_cache();
            // Reset the cached DB connection before restoring the env var
            reset_scan_db();
            unsafe {
                std::env::remove_var("SKILLSTAR_DATA_DIR");
            }
        }
    }

    fn sample_result(skill_name: &str, scan_mode: &str) -> SecurityScanResult {
        SecurityScanResult {
            skill_name: skill_name.to_string(),
            scanned_at: chrono::Utc::now().to_rfc3339(),
            tree_hash: Some(format!("hash-{}", skill_name)),
            scan_mode: scan_mode.to_string(),
            scanner_version: CACHE_SCHEMA_VERSION.to_string(),
            target_language: "zh-CN".to_string(),
            risk_level: RiskLevel::Low,
            static_findings: vec![],
            ai_findings: vec![],
            summary: "test".to_string(),
            files_scanned: 1,
            total_chars_analyzed: 10,
            incomplete: false,
            ai_files_analyzed: 0,
            chunks_used: 0,
        }
    }

    #[test]
    fn content_hash_uses_full_file_contents_not_truncated_snippet() {
        let temp_dir = TempDir::new("security-scan-hash");
        let skill_md = temp_dir.path().join("SKILL.md");
        let prefix = "A".repeat(MAX_FILE_CHARS);

        fs::write(&skill_md, format!("{}tail-one", prefix)).expect("write first version");
        let (_, first_hash) = collect_scannable_files(temp_dir.path());

        fs::write(&skill_md, format!("{}tail-two", prefix)).expect("write second version");
        let (_, second_hash) = collect_scannable_files(temp_dir.path());

        assert_ne!(first_hash, second_hash);
    }

    #[test]
    fn invalid_ai_json_is_reported_as_error() {
        let log_ctx = SkillScanLogCtx::new("demo-skill");
        let err = parse_file_scan_result("scripts/test.sh", FileRole::Script, "not-json", &log_ctx)
            .expect_err("invalid AI JSON should fail");
        assert!(err.to_string().contains("invalid JSON"));
    }

    #[test]
    fn cache_is_scoped_by_scan_mode() {
        let _data_root = TestDataRoot::new("security-scan-cache-mode");
        save_to_cache(&sample_result("demo-skill", "static")).expect("save static cache");

        assert!(
            load_cached_result(
                "demo-skill",
                &cache_scan_mode_key("static", "zh-CN"),
                Some("hash-demo-skill"),
            )
            .is_some()
        );
        assert!(
            load_cached_result(
                "demo-skill",
                &cache_scan_mode_key("smart", "zh-CN"),
                Some("hash-demo-skill"),
            )
            .is_none()
        );
    }

    #[test]
    fn incomplete_results_are_not_cached() {
        let _data_root = TestDataRoot::new("security-scan-incomplete");
        let mut result = sample_result("demo-incomplete", "smart");
        result.incomplete = true;

        save_to_cache(&result).expect("skip incomplete cache write");

        assert!(
            load_cached_result(
                "demo-incomplete",
                &cache_scan_mode_key("smart", "zh-CN"),
                Some("hash-demo-incomplete"),
            )
            .is_none()
        );
    }

    #[test]
    fn smart_mode_can_reuse_deep_skill_cache() {
        let _data_root = TestDataRoot::new("security-scan-smart-deep-reuse");
        save_to_cache(&sample_result("demo-deep", "deep")).expect("save deep cache");

        let reused = try_reuse_cached(
            "demo-deep",
            ScanMode::Smart,
            Some("hash-demo-deep"),
            "zh-CN",
        );
        assert!(reused.is_some(), "smart mode should reuse deep cache");
        assert_eq!(reused.unwrap().scan_mode, "deep");
    }

    #[test]
    fn cache_is_isolated_by_target_language() {
        let _data_root = TestDataRoot::new("security-scan-language-isolation");
        let mut result = sample_result("demo-lang", "smart");
        result.target_language = "en".to_string();
        save_to_cache(&result).expect("save english cache");

        let zh_reused = try_reuse_cached(
            "demo-lang",
            ScanMode::Smart,
            Some("hash-demo-lang"),
            "zh-CN",
        );
        assert!(
            zh_reused.is_none(),
            "different target language should not reuse cached result"
        );

        let en_reused =
            try_reuse_cached("demo-lang", ScanMode::Smart, Some("hash-demo-lang"), "en");
        assert!(
            en_reused.is_some(),
            "same target language should reuse cache"
        );
    }

    #[test]
    fn file_cache_stores_and_retrieves_by_digest() {
        let _data_root = TestDataRoot::new("file-cache-basic");

        let files = vec![ScannedFile {
            relative_path: "SKILL.md".to_string(),
            content: "test content".to_string(),
            size_bytes: 12,
            content_digest: "digest-abc123".to_string(),
        }];

        let results = vec![FileScanResult {
            file_path: "SKILL.md".to_string(),
            role: FileRole::Skill,
            findings: vec![],
            file_risk: RiskLevel::Safe,
            tokens_hint: 0,
        }];
        let classifications = vec![(FileRole::Skill, 0)];

        // Save to file cache
        save_file_scan_results(&files, &classifications, &results, "smart", "zh-CN");

        // Load back by digest
        let conn = init_db().expect("init db");
        let cached =
            load_cached_file_result(&conn, "digest-abc123", FileRole::Skill, "smart", "zh-CN");
        assert!(cached.is_some(), "should find cached file result");
        assert_eq!(cached.unwrap().file_risk, RiskLevel::Safe);

        // Same digest, wrong mode → no hit
        let no_hit =
            load_cached_file_result(&conn, "digest-abc123", FileRole::Skill, "static", "zh-CN");
        assert!(no_hit.is_none(), "static mode should not find smart cache");
    }

    #[test]
    fn file_cache_deep_satisfies_smart() {
        let _data_root = TestDataRoot::new("file-cache-downgrade");

        let files = vec![ScannedFile {
            relative_path: "scripts/run.sh".to_string(),
            content: "#!/bin/bash\necho hello".to_string(),
            size_bytes: 25,
            content_digest: "digest-deep-001".to_string(),
        }];

        let results = vec![FileScanResult {
            file_path: "scripts/run.sh".to_string(),
            role: FileRole::Script,
            findings: vec![],
            file_risk: RiskLevel::Low,
            tokens_hint: 0,
        }];
        let classifications = vec![(FileRole::Script, 0)];

        // Save as "deep" mode
        save_file_scan_results(&files, &classifications, &results, "deep", "zh-CN");

        // Request as "smart" → should hit (deep satisfies smart)
        let conn = init_db().expect("init db");
        let cached =
            load_cached_file_result(&conn, "digest-deep-001", FileRole::Script, "smart", "zh-CN");
        assert!(cached.is_some(), "deep cache should satisfy smart request");

        // Request as "deep" → should also hit
        let cached_deep =
            load_cached_file_result(&conn, "digest-deep-001", FileRole::Script, "deep", "zh-CN");
        assert!(
            cached_deep.is_some(),
            "deep cache should satisfy deep request"
        );
    }

    #[test]
    fn file_cache_is_isolated_by_file_role() {
        let _data_root = TestDataRoot::new("file-cache-role-key");

        let files = vec![ScannedFile {
            relative_path: "SKILL.md".to_string(),
            content: "role test".to_string(),
            size_bytes: 9,
            content_digest: "digest-role-001".to_string(),
        }];
        let classifications = vec![(FileRole::Skill, 0)];
        let results = vec![FileScanResult {
            file_path: "SKILL.md".to_string(),
            role: FileRole::Skill,
            findings: vec![],
            file_risk: RiskLevel::Low,
            tokens_hint: 0,
        }];

        save_file_scan_results(&files, &classifications, &results, "smart", "zh-CN");

        let conn = init_db().expect("init db");
        let hit =
            load_cached_file_result(&conn, "digest-role-001", FileRole::Skill, "smart", "zh-CN");
        assert!(hit.is_some(), "same role should hit cache");

        let miss =
            load_cached_file_result(&conn, "digest-role-001", FileRole::Script, "smart", "zh-CN");
        assert!(miss.is_none(), "different role should miss cache");
    }

    #[test]
    fn clear_logs_removes_runtime_and_archived_logs() {
        let _data_root = TestDataRoot::new("security-scan-clear-logs");

        let runtime_log = log_path();
        let archive_dir = scan_logs_dir();
        fs::create_dir_all(&archive_dir).expect("create archive dir");

        fs::write(&runtime_log, "runtime log").expect("write runtime log");
        let archived = archive_dir.join("scan-20260331-000000-test.log");
        fs::write(&archived, "archived log").expect("write archived log");

        clear_logs().expect("clear logs");

        assert!(
            !runtime_log.exists(),
            "runtime log should be removed by clear_logs"
        );
        assert!(
            archive_dir.exists(),
            "scan log directory should be recreated by clear_logs"
        );
        assert!(
            list_scan_log_entries(10).is_empty(),
            "archived scan logs should be removed"
        );
    }

    #[test]
    fn partition_cache_hit_rewrites_finding_file_path() {
        let _data_root = TestDataRoot::new("file-cache-path-rewrite");

        let old_files = vec![ScannedFile {
            relative_path: "docs/old.md".to_string(),
            content: "same content".to_string(),
            size_bytes: 12,
            content_digest: "digest-path-001".to_string(),
        }];
        let old_classifications = vec![(FileRole::Resource, 0)];
        let old_results = vec![FileScanResult {
            file_path: "docs/old.md".to_string(),
            role: FileRole::Resource,
            findings: vec![AiFinding {
                category: "data".to_string(),
                severity: RiskLevel::Medium,
                file_path: "docs/old.md".to_string(),
                description: "cached finding".to_string(),
                evidence: "evidence".to_string(),
                recommendation: "fix".to_string(),
            }],
            file_risk: RiskLevel::Medium,
            tokens_hint: 0,
        }];
        save_file_scan_results(
            &old_files,
            &old_classifications,
            &old_results,
            "smart",
            "zh-CN",
        );

        let renamed_files = vec![ScannedFile {
            relative_path: "docs/new.md".to_string(),
            content: "same content".to_string(),
            size_bytes: 12,
            content_digest: "digest-path-001".to_string(),
        }];
        let renamed_classifications = vec![(FileRole::Resource, 0)];
        let log_ctx = SkillScanLogCtx::new("demo");
        let (cached, needs_scan) = partition_cached_files(
            &renamed_files,
            &renamed_classifications,
            "smart",
            "zh-CN",
            &log_ctx,
        );

        assert!(
            needs_scan.is_empty(),
            "renamed file should still hit by digest"
        );
        assert_eq!(cached.len(), 1, "expected one cached result");
        assert_eq!(cached[0].file_path, "docs/new.md");
        assert_eq!(cached[0].findings.len(), 1);
        assert_eq!(cached[0].findings[0].file_path, "docs/new.md");
    }

    #[test]
    fn scan_mode_compatibility_rules() {
        // Deep satisfies deep and smart, but not static
        assert!(scan_mode_compatible("deep", "deep"));
        assert!(scan_mode_compatible("deep", "smart"));
        assert!(!scan_mode_compatible("deep", "static"));

        // Smart satisfies smart, not deep
        assert!(scan_mode_compatible("smart", "smart"));
        assert!(!scan_mode_compatible("smart", "deep"));

        // Static only satisfies static
        assert!(scan_mode_compatible("static", "static"));
        assert!(!scan_mode_compatible("static", "smart"));
    }
}
