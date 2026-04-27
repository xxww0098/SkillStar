//! Core security scan data types and serde defaults.

use crate::constants::CACHE_SCHEMA_VERSION;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

// ── Detection Taxonomy ────────────────────────────────────────────────

/// High-level detection family that categorizes what class of issue a finding represents.
/// This is the primary taxonomy axis for organizing and routing findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DetectionFamily {
    /// Pattern-based detection via regex, AST rules, or static analysis heuristics.
    #[default]
    Pattern,
    /// Capability or permission-related detection (e.g., dangerous API usage, shell access).
    Capability,
    /// Secret or credential leakage detection (API keys, tokens, passwords in code).
    Secrets,
    /// Semantic flow detection (data dependency analysis, taint tracking, control-flow).
    SemanticFlow,
    /// Dynamic analysis findings (runtime behavior, sandbox observations).
    Dynamic,
    /// External tool findings (semgrep, trivy, grype, gitleaks, shellcheck, bandit, etc.).
    ExternalTool,
    /// Policy or compliance violations (license issues, structure requirements).
    Policy,
    /// Fallback for detections that do not fit any other family.
    Other,
}

impl DetectionFamily {
    /// Returns a short label suitable for display.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pattern => "pattern",
            Self::Capability => "capability",
            Self::Secrets => "secrets",
            Self::SemanticFlow => "semantic-flow",
            Self::Dynamic => "dynamic",
            Self::ExternalTool => "external-tool",
            Self::Policy => "policy",
            Self::Other => "other",
        }
    }
}

/// Kind of detection within a family, providing finer-grained categorization
/// that is meaningful for both static and AI-powered findings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct DetectionKind {
    /// The detection family this kind belongs to.
    #[serde(default)]
    pub family: DetectionFamily,
    /// The specific kind name within that family.
    /// Examples: "regex-match", "dangerous-permission", "hardcoded-secret",
    /// "taint-propagation", "sandbox-escape", "semgrep-find", "missing-license".
    pub kind: String,
}

impl DetectionKind {
    /// Creates a new detection kind with the given family and kind string.
    pub fn new(family: DetectionFamily, kind: impl Into<String>) -> Self {
        Self {
            family,
            kind: kind.into(),
        }
    }

    /// Shorthand for a pattern-based detection kind.
    pub fn pattern(kind: impl Into<String>) -> Self {
        Self::new(DetectionFamily::Pattern, kind)
    }

    /// Shorthand for a secrets detection kind.
    pub fn secrets(kind: impl Into<String>) -> Self {
        Self::new(DetectionFamily::Secrets, kind)
    }

    /// Shorthand for a capability detection kind.
    pub fn capability(kind: impl Into<String>) -> Self {
        Self::new(DetectionFamily::Capability, kind)
    }

    /// Shorthand for a semantic-flow detection kind.
    pub fn semantic_flow(kind: impl Into<String>) -> Self {
        Self::new(DetectionFamily::SemanticFlow, kind)
    }

    /// Shorthand for a dynamic analysis detection kind.
    pub fn dynamic(kind: impl Into<String>) -> Self {
        Self::new(DetectionFamily::Dynamic, kind)
    }

    /// Shorthand for an external-tool detection kind.
    pub fn external_tool(kind: impl Into<String>) -> Self {
        Self::new(DetectionFamily::ExternalTool, kind)
    }

    /// Shorthand for a policy detection kind.
    pub fn policy(kind: impl Into<String>) -> Self {
        Self::new(DetectionFamily::Policy, kind)
    }

    /// Shorthand for an other detection kind.
    pub fn other(kind: impl Into<String>) -> Self {
        Self::new(DetectionFamily::Other, kind)
    }
}

/// Taxonomy metadata attached to every finding (static or AI).
/// This provides a consistent classification layer without breaking existing serde consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DetectionTaxonomy {
    /// The detection family and kind. Optional so existing findings deserialize cleanly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detection_kind: Option<DetectionKind>,
    /// Optional free-form tags for additional classification.
    /// Examples: "owasp", "cve-2024", "exec", "network", "file-access".
    #[serde(default)]
    pub tags: Vec<String>,
}

impl DetectionTaxonomy {
    /// Creates a taxonomy with just a detection kind and no tags.
    pub fn with_kind(kind: DetectionKind) -> Self {
        Self {
            detection_kind: Some(kind),
            tags: Vec::new(),
        }
    }

    /// Creates a taxonomy with a detection kind and additional tags.
    pub fn with_kind_and_tags(kind: DetectionKind, tags: impl IntoIterator<Item = String>) -> Self {
        Self {
            detection_kind: Some(kind),
            tags: tags.into_iter().collect(),
        }
    }

    /// Returns true if both detection_kind and tags are absent/empty.
    pub fn is_empty(&self) -> bool {
        self.detection_kind.is_none() && self.tags.is_empty()
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StaticFinding {
    pub file_path: String,
    pub line_number: usize,
    pub pattern_id: String,
    pub snippet: String,
    pub severity: RiskLevel,
    #[serde(default = "default_static_finding_confidence")]
    pub confidence: f32,
    pub description: String,
    /// Optional taxonomy metadata for classification. Absent when deserialized from
    /// legacy data; present for new findings produced by taxonomy-aware detectors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taxonomy: Option<DetectionTaxonomy>,
}

impl Default for StaticFinding {
    fn default() -> Self {
        Self {
            file_path: String::new(),
            line_number: 0,
            pattern_id: String::new(),
            snippet: String::new(),
            severity: RiskLevel::Safe,
            confidence: default_static_finding_confidence(),
            description: String::new(),
            taxonomy: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiFinding {
    pub category: String,
    pub severity: RiskLevel,
    #[serde(default = "default_ai_finding_confidence")]
    pub confidence: f32,
    pub file_path: String,
    pub description: String,
    pub evidence: String,
    pub recommendation: String,
    /// Optional taxonomy metadata for classification. Absent when deserialized from
    /// legacy data; present for new findings produced by taxonomy-aware detectors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taxonomy: Option<DetectionTaxonomy>,
}

impl Default for AiFinding {
    fn default() -> Self {
        Self {
            category: String::new(),
            severity: RiskLevel::Safe,
            confidence: default_ai_finding_confidence(),
            file_path: String::new(),
            description: String::new(),
            evidence: String::new(),
            recommendation: String::new(),
            taxonomy: None,
        }
    }
}

/// Per-file worker result (internal, not serialized to frontend)
#[derive(Debug, Clone)]
pub struct FileScanResult {
    pub file_path: String,
    pub role: FileRole,
    pub findings: Vec<AiFinding>,
    pub file_risk: RiskLevel,
    #[allow(dead_code)]
    pub tokens_hint: usize,
}

#[derive(Debug, Clone)]
pub struct PreparedChunk {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceTrailKind {
    #[default]
    StaticFinding,
    AiFinding,
    AnalyzerSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct EvidenceTrailEntry {
    #[serde(default)]
    pub kind: EvidenceTrailKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_number: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<RiskLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excerpts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taxonomy: Option<DetectionTaxonomy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analyzer_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analyzer_findings: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analyzer_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PreparedSkillScan {
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
    pub log_ctx: crate::scan::SkillScanLogCtx,
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
    #[serde(default)]
    pub evidence_trail: Vec<EvidenceTrailEntry>,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SecurityScanAuditError {
    pub skill_name: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SecurityScanAuditFinding {
    pub finding_type: String,
    pub risk_level: Option<RiskLevel>,
    pub confidence: Option<f32>,
    pub file_path: Option<String>,
    pub line_number: Option<usize>,
    pub label: Option<String>,
    pub description: String,
    pub evidence: Option<String>,
    pub recommendation: Option<String>,
    pub raw_line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SecurityScanAuditSkillDetail {
    pub skill_name: String,
    pub scanned_at: Option<String>,
    pub risk_level: Option<RiskLevel>,
    pub risk_score: Option<f32>,
    pub confidence_score: Option<f32>,
    pub meta_deduped_count: usize,
    pub meta_consensus_count: usize,
    pub scan_mode: Option<String>,
    pub scanner_version: Option<String>,
    pub incomplete: Option<bool>,
    pub files_scanned: Option<usize>,
    pub total_chars_analyzed: Option<usize>,
    pub static_findings_count: usize,
    pub ai_findings_count: usize,
    pub tree_hash: Option<String>,
    pub summary: Option<String>,
    #[serde(default)]
    pub findings: Vec<SecurityScanAuditFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SecurityScanAuditTelemetrySnapshot {
    pub recorded_at: String,
    pub request_hash: String,
    pub requested_mode: String,
    pub effective_mode: String,
    pub force: bool,
    pub duration_ms: i64,
    pub targets_total: usize,
    pub results_total: usize,
    pub pass_count: usize,
    pub pass_rate: f32,
    pub incomplete_count: usize,
    pub error_count: usize,
    pub risk_distribution: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SecurityScanAuditSummary {
    pub file_name: String,
    pub path: String,
    pub created_at: String,
    pub size_bytes: u64,
    pub request_id: Option<String>,
    pub request_hash: Option<String>,
    pub requested_mode: Option<String>,
    pub effective_mode: Option<String>,
    pub force: Option<bool>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub targets_total: Option<usize>,
    pub cached_hits: usize,
    pub completed_results: usize,
    pub error_count: usize,
    pub skill_count: usize,
    pub incomplete_count: usize,
    pub highest_risk: Option<RiskLevel>,
    pub parse_warnings: usize,
    pub telemetry: Option<SecurityScanAuditTelemetrySnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SecurityScanAuditDetail {
    pub summary: SecurityScanAuditSummary,
    #[serde(default)]
    pub cached_skills: Vec<String>,
    #[serde(default)]
    pub errors: Vec<SecurityScanAuditError>,
    #[serde(default)]
    pub skills: Vec<SecurityScanAuditSkillDetail>,
    #[serde(default)]
    pub parse_warnings: Vec<String>,
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

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_family_serde_roundtrip() {
        for family in [
            DetectionFamily::Pattern,
            DetectionFamily::Capability,
            DetectionFamily::Secrets,
            DetectionFamily::SemanticFlow,
            DetectionFamily::Dynamic,
            DetectionFamily::ExternalTool,
            DetectionFamily::Policy,
            DetectionFamily::Other,
        ] {
            let json = serde_json::to_string(&family).unwrap();
            let back: DetectionFamily = serde_json::from_str(&json).unwrap();
            assert_eq!(family, back);
        }
    }

    #[test]
    fn detection_family_label() {
        assert_eq!(DetectionFamily::Pattern.label(), "pattern");
        assert_eq!(DetectionFamily::Secrets.label(), "secrets");
        assert_eq!(DetectionFamily::SemanticFlow.label(), "semantic-flow");
        assert_eq!(DetectionFamily::ExternalTool.label(), "external-tool");
    }

    #[test]
    fn detection_kind_serde_roundtrip() {
        let kind = DetectionKind::secrets("hardcoded-api-key");
        let json = serde_json::to_string(&kind).unwrap();
        let back: DetectionKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, back);
        assert_eq!(back.family, DetectionFamily::Secrets);
        assert_eq!(back.kind, "hardcoded-api-key");
    }

    #[test]
    fn detection_kind_shorthand_constructors() {
        let p = DetectionKind::pattern("regex-match");
        assert_eq!(p.family, DetectionFamily::Pattern);

        let s = DetectionKind::secrets("credential");
        assert_eq!(s.family, DetectionFamily::Secrets);

        let c = DetectionKind::capability("shell-exec");
        assert_eq!(c.family, DetectionFamily::Capability);

        let sf = DetectionKind::semantic_flow("taint-propagation");
        assert_eq!(sf.family, DetectionFamily::SemanticFlow);

        let d = DetectionKind::dynamic("sandbox-escape");
        assert_eq!(d.family, DetectionFamily::Dynamic);

        let e = DetectionKind::external_tool("semgrep-find");
        assert_eq!(e.family, DetectionFamily::ExternalTool);

        let pol = DetectionKind::policy("missing-license");
        assert_eq!(pol.family, DetectionFamily::Policy);

        let o = DetectionKind::other("unknown");
        assert_eq!(o.family, DetectionFamily::Other);
    }

    #[test]
    fn detection_taxonomy_is_empty() {
        let empty = DetectionTaxonomy::default();
        assert!(empty.is_empty());

        let with_kind = DetectionTaxonomy::with_kind(DetectionKind::pattern("test"));
        assert!(!with_kind.is_empty());

        let with_tags = DetectionTaxonomy {
            detection_kind: None,
            tags: vec!["owasp".to_string()],
        };
        assert!(!with_tags.is_empty());
    }

    #[test]
    fn detection_taxonomy_serde_roundtrip() {
        let taxonomy = DetectionTaxonomy::with_kind_and_tags(
            DetectionKind::secrets("aws-key"),
            vec!["cve-2024".to_string(), "cloud".to_string()],
        );
        let json = serde_json::to_string(&taxonomy).unwrap();
        let back: DetectionTaxonomy = serde_json::from_str(&json).unwrap();
        assert_eq!(taxonomy, back);
        assert_eq!(
            back.detection_kind.unwrap().family,
            DetectionFamily::Secrets
        );
        assert_eq!(back.tags, vec!["cve-2024", "cloud"]);
    }

    #[test]
    fn static_finding_roundtrip_with_taxonomy() {
        let taxonomy = DetectionTaxonomy::with_kind_and_tags(
            DetectionKind::secrets("hardcoded-secret"),
            vec!["owasp".to_string()],
        );
        let finding = StaticFinding {
            file_path: "src/main.rs".to_string(),
            line_number: 42,
            pattern_id: "secret-api-key".to_string(),
            snippet: "const API_KEY = \"sk-abc123\"".to_string(),
            severity: RiskLevel::High,
            confidence: 0.85,
            description: "Hardcoded API key detected".to_string(),
            taxonomy: Some(taxonomy),
        };
        let json = serde_json::to_string(&finding).unwrap();
        let back: StaticFinding = serde_json::from_str(&json).unwrap();
        assert_eq!(finding.file_path, back.file_path);
        assert_eq!(finding.line_number, back.line_number);
        assert_eq!(finding.pattern_id, back.pattern_id);
        assert_eq!(finding.severity, back.severity);
        assert_eq!(finding.confidence, back.confidence);
        assert!(back.taxonomy.is_some());
        let inner = back.taxonomy.unwrap();
        assert!(!inner.is_empty());
        assert_eq!(
            inner.detection_kind.unwrap().family,
            DetectionFamily::Secrets
        );
    }

    #[test]
    fn static_finding_roundtrip_without_taxonomy() {
        let json = r#"{
            "file_path": "src/main.rs",
            "line_number": 10,
            "pattern_id": "dangerous-perm",
            "snippet": "shell: true",
            "severity": "Medium",
            "confidence": 0.75,
            "description": "Dangerous permission"
        }"#;
        let finding: StaticFinding = serde_json::from_str(json).unwrap();
        assert_eq!(finding.file_path, "src/main.rs");
        assert!(finding.taxonomy.is_none());
        let out = serde_json::to_string(&finding).unwrap();
        let back: StaticFinding = serde_json::from_str(&out).unwrap();
        assert_eq!(finding, back);
    }

    #[test]
    fn ai_finding_roundtrip_with_taxonomy() {
        let taxonomy = DetectionTaxonomy::with_kind_and_tags(
            DetectionKind::capability("command-injection"),
            vec!["owasp".to_string(), "exec".to_string()],
        );
        let finding = AiFinding {
            category: "Security Risk".to_string(),
            severity: RiskLevel::Critical,
            confidence: 0.92,
            file_path: "SKILL.md".to_string(),
            description: "Potential command injection".to_string(),
            evidence: "User input concatenated into shell command".to_string(),
            recommendation: "Use parameterized APIs instead".to_string(),
            taxonomy: Some(taxonomy),
        };
        let json = serde_json::to_string(&finding).unwrap();
        let back: AiFinding = serde_json::from_str(&json).unwrap();
        assert_eq!(finding.category, back.category);
        assert_eq!(finding.severity, back.severity);
        assert!(back.taxonomy.is_some());
        let inner = back.taxonomy.unwrap();
        assert!(!inner.is_empty());
        assert_eq!(
            inner.detection_kind.unwrap().family,
            DetectionFamily::Capability
        );
        assert_eq!(inner.tags, vec!["owasp", "exec"]);
    }

    #[test]
    fn ai_finding_roundtrip_without_taxonomy() {
        let json = r#"{
            "category": "Data Leak",
            "severity": "High",
            "confidence": 0.88,
            "file_path": "src/utils.rs",
            "description": "Exposes internal state",
            "evidence": "returns private field",
            "recommendation": "Return clone orArc instead"
        }"#;
        let finding: AiFinding = serde_json::from_str(json).unwrap();
        assert_eq!(finding.category, "Data Leak");
        assert!(finding.taxonomy.is_none());
        let out = serde_json::to_string(&finding).unwrap();
        let back: AiFinding = serde_json::from_str(&out).unwrap();
        assert_eq!(finding, back);
    }

    #[test]
    fn taxonomy_skips_none_in_json() {
        let finding = StaticFinding {
            file_path: "test.rs".to_string(),
            line_number: 1,
            pattern_id: "test".to_string(),
            snippet: "test".to_string(),
            severity: RiskLevel::Low,
            confidence: 0.5,
            description: "test".to_string(),
            taxonomy: None,
        };
        let json = serde_json::to_string(&finding).unwrap();
        assert!(!json.contains("taxonomy"));
        assert!(json.contains("\"confidence\":0.5"));
    }

    #[test]
    fn evidence_trail_entry_roundtrip() {
        let entry = EvidenceTrailEntry {
            kind: EvidenceTrailKind::StaticFinding,
            file_path: Some("scripts/run.sh".to_string()),
            line_number: Some(12),
            source_id: Some("curl_pipe_sh".to_string()),
            severity: Some(RiskLevel::High),
            confidence: Some(0.91),
            summary: Some("Remote script piping detected".to_string()),
            excerpts: vec![
                "curl https://example.com | sh".to_string(),
                "Avoid piping remote content into a shell".to_string(),
            ],
            taxonomy: Some(DetectionTaxonomy::with_kind(DetectionKind::capability(
                "command-execution",
            ))),
            analyzer_status: None,
            analyzer_findings: None,
            analyzer_error: None,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let back: EvidenceTrailEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, back);
    }

    #[test]
    fn security_scan_result_roundtrip_with_evidence_trail() {
        let result = SecurityScanResult {
            skill_name: "demo-skill".to_string(),
            scanned_at: "2026-04-23T00:00:00Z".to_string(),
            tree_hash: Some("hash-demo".to_string()),
            scan_mode: "smart".to_string(),
            scanner_version: "1".to_string(),
            target_language: "zh-CN".to_string(),
            risk_level: RiskLevel::High,
            risk_score: 7.5,
            confidence_score: 0.83,
            meta_deduped_count: 1,
            meta_consensus_count: 1,
            analyzer_executions: vec![AnalyzerExecutionSummary {
                id: "pattern".to_string(),
                status: "ok".to_string(),
                findings: 2,
                error: None,
            }],
            evidence_trail: vec![EvidenceTrailEntry {
                kind: EvidenceTrailKind::AnalyzerSummary,
                file_path: None,
                line_number: None,
                source_id: Some("pattern".to_string()),
                severity: None,
                confidence: None,
                summary: Some("Analyzer completed".to_string()),
                excerpts: vec!["status=ok".to_string()],
                taxonomy: None,
                analyzer_status: Some("ok".to_string()),
                analyzer_findings: Some(2),
                analyzer_error: None,
            }],
            static_findings: vec![],
            ai_findings: vec![],
            summary: "scan summary".to_string(),
            files_scanned: 1,
            total_chars_analyzed: 42,
            incomplete: false,
            ai_files_analyzed: 1,
            chunks_used: 1,
        };

        let json = serde_json::to_string(&result).unwrap();
        let back: SecurityScanResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.evidence_trail, result.evidence_trail);
        assert_eq!(back.analyzer_executions.len(), 1);
    }

    #[test]
    fn legacy_security_scan_result_without_evidence_trail_deserializes() {
        let json = r#"{
            "skill_name": "legacy-skill",
            "scanned_at": "2026-04-23T00:00:00Z",
            "tree_hash": "hash-legacy",
            "scan_mode": "static",
            "scanner_version": "1",
            "target_language": "zh-CN",
            "risk_level": "Low",
            "risk_score": 2.5,
            "confidence_score": 0.75,
            "meta_deduped_count": 0,
            "meta_consensus_count": 0,
            "analyzer_executions": [],
            "static_findings": [],
            "ai_findings": [],
            "summary": "legacy summary",
            "files_scanned": 1,
            "total_chars_analyzed": 10,
            "incomplete": false,
            "ai_files_analyzed": 0,
            "chunks_used": 0
        }"#;

        let result: SecurityScanResult = serde_json::from_str(json).unwrap();
        assert!(result.evidence_trail.is_empty());
        assert_eq!(result.skill_name, "legacy-skill");
    }

    #[test]
    fn security_scan_audit_detail_roundtrip() {
        let detail = SecurityScanAuditDetail {
            summary: SecurityScanAuditSummary {
                file_name: "scan-1.log".to_string(),
                path: "/tmp/scan-1.log".to_string(),
                created_at: "2026-04-23T12:00:00Z".to_string(),
                size_bytes: 128,
                request_id: Some("req-1".to_string()),
                request_hash: Some("abc123".to_string()),
                requested_mode: Some("smart".to_string()),
                effective_mode: Some("static".to_string()),
                force: Some(false),
                started_at: Some("2026-04-23T11:59:00Z".to_string()),
                finished_at: Some("2026-04-23T12:00:00Z".to_string()),
                duration_ms: Some(60_000),
                targets_total: Some(2),
                cached_hits: 1,
                completed_results: 1,
                error_count: 1,
                skill_count: 1,
                incomplete_count: 0,
                highest_risk: Some(RiskLevel::High),
                parse_warnings: 0,
                telemetry: Some(SecurityScanAuditTelemetrySnapshot {
                    recorded_at: "2026-04-23T12:00:00Z".to_string(),
                    request_hash: "abc123".to_string(),
                    requested_mode: "smart".to_string(),
                    effective_mode: "static".to_string(),
                    force: false,
                    duration_ms: 60_000,
                    targets_total: 2,
                    results_total: 1,
                    pass_count: 1,
                    pass_rate: 0.5,
                    incomplete_count: 0,
                    error_count: 1,
                    risk_distribution: BTreeMap::from([
                        ("low".to_string(), 0),
                        ("high".to_string(), 1),
                    ]),
                }),
            },
            cached_skills: vec!["cached-skill".to_string()],
            errors: vec![SecurityScanAuditError {
                skill_name: "broken-skill".to_string(),
                message: "timeout".to_string(),
            }],
            skills: vec![SecurityScanAuditSkillDetail {
                skill_name: "demo-skill".to_string(),
                scanned_at: Some("2026-04-23T12:00:00Z".to_string()),
                risk_level: Some(RiskLevel::High),
                risk_score: Some(7.1),
                confidence_score: Some(0.88),
                meta_deduped_count: 2,
                meta_consensus_count: 1,
                scan_mode: Some("smart".to_string()),
                scanner_version: Some("42".to_string()),
                incomplete: Some(false),
                files_scanned: Some(4),
                total_chars_analyzed: Some(1200),
                static_findings_count: 1,
                ai_findings_count: 1,
                tree_hash: Some("abc123def456".to_string()),
                summary: Some("Potential remote execution".to_string()),
                findings: vec![SecurityScanAuditFinding {
                    finding_type: "static".to_string(),
                    risk_level: Some(RiskLevel::High),
                    confidence: Some(0.91),
                    file_path: Some("scripts/run.sh".to_string()),
                    line_number: Some(12),
                    label: Some("curl_pipe_sh".to_string()),
                    description: "Remote script piping detected".to_string(),
                    evidence: None,
                    recommendation: None,
                    raw_line: "- [high|conf=0.91] scripts/run.sh:12 Remote script piping detected (curl_pipe_sh)".to_string(),
                }],
            }],
            parse_warnings: vec!["warning".to_string()],
        };

        let json = serde_json::to_string(&detail).unwrap();
        let back: SecurityScanAuditDetail = serde_json::from_str(&json).unwrap();
        assert_eq!(detail, back);
    }
}
