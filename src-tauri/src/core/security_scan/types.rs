//! Core security scan data types and serde defaults.

use super::constants::CACHE_SCHEMA_VERSION;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

// ── Confidence (used by policy, static patterns, orchestration) ─────

pub(crate) fn default_confidence_score() -> f32 {
    0.5
}

pub(crate) fn clamp_confidence(value: f32) -> f32 {
    if !value.is_finite() {
        return default_confidence_score();
    }
    value.clamp(0.0, 1.0)
}

pub(crate) fn default_confidence_for_severity(level: RiskLevel) -> f32 {
    match level {
        RiskLevel::Safe => 0.60,
        RiskLevel::Low => 0.68,
        RiskLevel::Medium => 0.75,
        RiskLevel::High => 0.84,
        RiskLevel::Critical => 0.92,
    }
}

pub(crate) fn parse_confidence_from_json(value: Option<&serde_json::Value>, fallback: f32) -> f32 {
    value
        .and_then(|v| v.as_f64())
        .map(|v| clamp_confidence(v as f32))
        .unwrap_or_else(|| clamp_confidence(fallback))
}

pub(crate) fn default_static_finding_confidence() -> f32 {
    0.78
}

pub(crate) fn default_ai_finding_confidence() -> f32 {
    0.72
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

fn default_policy_preset() -> String {
    "balanced".to_string()
}

fn default_policy_severity_threshold() -> String {
    "low".to_string()
}

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
    pub(crate) fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().trim() {
            "safe" | "none" => Self::Safe,
            "low" | "info" => Self::Low,
            "medium" | "moderate" => Self::Medium,
            "high" => Self::High,
            "critical" | "severe" => Self::Critical,
            _ => Self::Low, // Conservative: unknown values → Low, not Safe
        }
    }

    pub(crate) fn severity_ord(&self) -> u8 {
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
    pub(crate) fn file_name(&self) -> &str {
        Path::new(&self.relative_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&self.relative_path)
    }

    pub(crate) fn extension(&self) -> &str {
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
    #[serde(default = "default_static_finding_confidence")]
    pub confidence: f32,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiFinding {
    pub category: String,
    pub severity: RiskLevel,
    #[serde(default = "default_ai_finding_confidence")]
    pub confidence: f32,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzerExecutionSummary {
    pub id: String,
    pub status: String,
    pub findings: usize,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedSkillScan {
    pub skill_name: String,
    pub files: Vec<ScannedFile>,
    pub classifications: Vec<(FileRole, usize)>,
    pub static_findings: Vec<StaticFinding>,
    pub analyzer_executions: Vec<AnalyzerExecutionSummary>,
    pub cached_file_results: Vec<FileScanResult>,
    pub cached_file_hits: usize,
    pub chunks: Vec<PreparedChunk>,
    pub content_hash: String,
    pub total_chars_analyzed: usize,
    pub actual_mode: ScanMode,
    pub run_ai: bool,
    pub ai_files_analyzed: usize,
    pub log_ctx: super::SkillScanLogCtx,
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
    #[serde(default)]
    pub risk_score: f32,
    #[serde(default = "default_confidence_score")]
    pub confidence_score: f32,
    #[serde(default)]
    pub meta_deduped_count: usize,
    #[serde(default)]
    pub meta_consensus_count: usize,
    #[serde(default)]
    pub analyzer_executions: Vec<AnalyzerExecutionSummary>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityScanTelemetryEntry {
    pub recorded_at: String,
    pub request_hash: String,
    pub requested_mode: String,
    pub effective_mode: String,
    pub force: bool,
    pub duration_ms: i64,
    pub targets_total: usize,
    pub results_total: usize,
    pub pass_count: usize,
    /// 0.0~1.0 ratio based on total targets in this run.
    pub pass_rate: f32,
    pub incomplete_count: usize,
    pub error_count: usize,
    pub risk_distribution: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityScanPolicy {
    #[serde(default = "default_policy_preset")]
    pub preset: String,
    #[serde(default = "default_policy_severity_threshold")]
    pub severity_threshold: String,
    #[serde(default)]
    pub enabled_analyzers: Vec<String>,
    #[serde(default)]
    pub ignore_rules: Vec<String>,
    #[serde(default)]
    pub rule_overrides: HashMap<String, SecurityScanRuleOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityScanRuleOverride {
    pub enabled: Option<bool>,
    pub severity: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedSecurityScanPolicy {
    pub(crate) min_severity: RiskLevel,
    pub(crate) ignore_rules: HashSet<String>,
    pub(crate) rule_overrides: HashMap<String, SecurityScanRuleOverride>,
}
