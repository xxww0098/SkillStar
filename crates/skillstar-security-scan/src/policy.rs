//! Security scan policy file I/O and resolution.

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};

use crate::constants::DEFAULT_SECURITY_SCAN_POLICY_YAML;
use crate::types::{
    ResolvedSecurityScanPolicy, RiskLevel, SecurityScanPolicy, StaticFinding, clamp_confidence,
    default_confidence_for_severity,
};

fn policy_path() -> std::path::PathBuf {
    skillstar_infra::paths::security_scan_policy_path()
}

pub(crate) fn normalize_rule_id(rule_id: &str) -> String {
    rule_id.trim().to_lowercase()
}

pub(crate) fn parse_policy_preset(raw: &str) -> &'static str {
    match raw.trim().to_lowercase().as_str() {
        "strict" => "strict",
        "permissive" => "permissive",
        _ => "balanced",
    }
}

fn parse_min_severity(raw: &str) -> RiskLevel {
    RiskLevel::from_str_loose(raw)
}

fn preset_ignore_rules(preset: &str) -> Vec<&'static str> {
    match preset {
        "strict" => vec![],
        "balanced" => vec!["pip_install"],
        "permissive" => vec![
            "pip_install",
            "npm_global_install",
            "sensitive_env",
            "long_base64",
        ],
        _ => vec![],
    }
}

fn preset_enabled_analyzers(preset: &str) -> Vec<&'static str> {
    match preset {
        "strict" => vec![
            "pattern",
            "doc_consistency",
            "secrets",
            "semantic",
            "dynamic",
            "semgrep",
            "trivy",
            "osv",
            "grype",
            "gitleaks",
            "shellcheck",
            "bandit",
            "sbom",
            "virustotal",
        ],
        "permissive" => vec!["pattern"],
        _ => vec![
            "pattern",
            "doc_consistency",
            "secrets",
            "semantic",
            "gitleaks",
        ],
    }
}

fn preset_severity_threshold(preset: &str) -> RiskLevel {
    match preset {
        "strict" => RiskLevel::Low,
        "permissive" => RiskLevel::Medium,
        _ => RiskLevel::Low,
    }
}

pub(crate) fn resolve_enabled_analyzers(policy: &SecurityScanPolicy) -> HashSet<String> {
    let preset = parse_policy_preset(&policy.preset);
    let configured = if policy.enabled_analyzers.is_empty() {
        preset_enabled_analyzers(preset)
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    } else {
        policy.enabled_analyzers.clone()
    };

    configured
        .into_iter()
        .map(|id| normalize_rule_id(&id))
        .filter(|id| !id.is_empty())
        .collect()
}

fn parse_policy_from_yaml(yaml: &str) -> Option<SecurityScanPolicy> {
    serde_yaml::from_str::<SecurityScanPolicy>(yaml).ok()
}

pub(crate) fn default_policy() -> SecurityScanPolicy {
    parse_policy_from_yaml(DEFAULT_SECURITY_SCAN_POLICY_YAML).unwrap_or_else(|| {
        SecurityScanPolicy {
            preset: default_policy_preset(),
            severity_threshold: default_policy_severity_threshold(),
            enabled_analyzers: vec![],
            ignore_rules: vec![],
            rule_overrides: HashMap::new(),
        }
    })
}

fn default_policy_preset() -> String {
    "balanced".to_string()
}

fn default_policy_severity_threshold() -> String {
    "low".to_string()
}

pub(crate) fn resolve_policy(policy: &SecurityScanPolicy) -> ResolvedSecurityScanPolicy {
    let preset = parse_policy_preset(&policy.preset);
    let mut ignore_rules: HashSet<String> = preset_ignore_rules(preset)
        .into_iter()
        .map(normalize_rule_id)
        .collect();
    for rule in &policy.ignore_rules {
        ignore_rules.insert(normalize_rule_id(rule));
    }
    let mut overrides = HashMap::new();
    for (rule, override_cfg) in &policy.rule_overrides {
        overrides.insert(normalize_rule_id(rule), override_cfg.clone());
    }
    let min_severity = if policy.severity_threshold.trim().is_empty() {
        preset_severity_threshold(&preset)
    } else {
        parse_min_severity(&policy.severity_threshold)
    };

    ResolvedSecurityScanPolicy {
        min_severity,
        ignore_rules,
        rule_overrides: overrides,
    }
}

pub(crate) fn load_effective_policy() -> ResolvedSecurityScanPolicy {
    let base = default_policy();
    let path = policy_path();
    if let Ok(raw) = std::fs::read_to_string(path) {
        if let Some(custom) = parse_policy_from_yaml(&raw) {
            return resolve_policy(&custom);
        }
    }
    resolve_policy(&base)
}

pub fn get_policy() -> SecurityScanPolicy {
    let path = policy_path();
    if let Ok(raw) = std::fs::read_to_string(path) {
        if let Some(custom) = parse_policy_from_yaml(&raw) {
            return custom;
        }
    }
    default_policy()
}

pub fn save_policy(policy: &SecurityScanPolicy) -> Result<()> {
    if let Some(parent) = policy_path().parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create policy directory: {}", parent.display()))?;
    }

    let normalized = SecurityScanPolicy {
        preset: parse_policy_preset(&policy.preset).to_string(),
        severity_threshold: if policy.severity_threshold.trim().is_empty() {
            default_policy_severity_threshold()
        } else {
            policy.severity_threshold.trim().to_string()
        },
        enabled_analyzers: policy
            .enabled_analyzers
            .iter()
            .map(|id| normalize_rule_id(id))
            .filter(|id| !id.is_empty())
            .collect(),
        ignore_rules: policy
            .ignore_rules
            .iter()
            .map(|id| normalize_rule_id(id))
            .collect(),
        rule_overrides: policy
            .rule_overrides
            .iter()
            .map(|(k, v)| (normalize_rule_id(k), v.clone()))
            .collect(),
    };

    let yaml = serde_yaml::to_string(&normalized).context("Failed to serialize scan policy")?;
    std::fs::write(policy_path(), yaml).context("Failed to write scan policy file")?;
    Ok(())
}

pub(crate) fn resolve_rule_severity(
    rule_id: &str,
    default_severity: RiskLevel,
    policy: &ResolvedSecurityScanPolicy,
) -> Option<RiskLevel> {
    let normalized = normalize_rule_id(rule_id);
    if policy.ignore_rules.contains(&normalized) {
        return None;
    }

    if let Some(override_cfg) = policy.rule_overrides.get(&normalized) {
        if matches!(override_cfg.enabled, Some(false)) {
            return None;
        }
        if let Some(ref severity) = override_cfg.severity {
            return Some(RiskLevel::from_str_loose(severity));
        }
    }

    Some(default_severity)
}

pub(crate) fn apply_policy_to_static_finding(
    mut finding: StaticFinding,
    policy: &ResolvedSecurityScanPolicy,
) -> Option<StaticFinding> {
    let severity = resolve_rule_severity(&finding.pattern_id, finding.severity, policy)?;
    if severity.severity_ord() < policy.min_severity.severity_ord() {
        return None;
    }
    finding.severity = severity;
    finding.confidence = clamp_confidence(
        finding
            .confidence
            .max(default_confidence_for_severity(severity)),
    );
    Some(finding)
}
