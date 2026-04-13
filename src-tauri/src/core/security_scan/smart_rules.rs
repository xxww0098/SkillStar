use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::{FileRole, ScannedFile};
use crate::core::infra::paths;

const DEFAULT_SMART_RULES_YAML: &str = include_str!("security_smart_rules_default.yaml");

#[derive(Debug, Clone)]
pub(crate) struct SmartRuleMatch {
    pub id: String,
    pub confidence: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct SmartTriageDecision {
    pub should_analyze: bool,
    pub confidence: f32,
    pub matched_rules: Vec<SmartRuleMatch>,
}

impl SmartTriageDecision {
    fn skip() -> Self {
        Self {
            should_analyze: false,
            confidence: 0.0,
            matched_rules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct SmartRuleSet {
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f32,
    #[serde(default)]
    pub rules: Vec<SmartRuleDef>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct SmartRuleDef {
    pub id: String,
    pub kind: String,
    #[serde(default = "default_rule_enabled")]
    pub enabled: bool,
    #[serde(default = "default_rule_confidence")]
    pub confidence: f32,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub any_signals: Vec<String>,
    #[serde(default)]
    pub path_prefixes: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub file_name_contains: Vec<String>,
    #[serde(default)]
    pub min_size_bytes: Option<usize>,
}

fn default_rule_enabled() -> bool {
    true
}

fn default_rule_confidence() -> f32 {
    0.7
}

fn default_min_confidence() -> f32 {
    0.45
}

fn role_matches(roles: &[String], role: FileRole) -> bool {
    if roles.is_empty() {
        return true;
    }
    let role_label = role.as_label().to_lowercase();
    roles
        .iter()
        .map(|item| item.trim().to_lowercase())
        .any(|item| item == "any" || item == role_label)
}

trait SmartRule: Send + Sync {
    fn id(&self) -> &str;
    fn confidence(&self) -> f32;
    fn matches(&self, file: &ScannedFile, role: FileRole) -> bool;
}

struct AlwaysRoleRule {
    id: String,
    confidence: f32,
    roles: Vec<String>,
}

impl SmartRule for AlwaysRoleRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn confidence(&self) -> f32 {
        self.confidence
    }

    fn matches(&self, _file: &ScannedFile, role: FileRole) -> bool {
        role_matches(&self.roles, role)
    }
}

struct ContentContainsAnyRule {
    id: String,
    confidence: f32,
    roles: Vec<String>,
    signals: Vec<String>,
}

impl SmartRule for ContentContainsAnyRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn confidence(&self) -> f32 {
        self.confidence
    }

    fn matches(&self, file: &ScannedFile, role: FileRole) -> bool {
        if !role_matches(&self.roles, role) {
            return false;
        }
        let content = file.content.to_lowercase();
        self.signals.iter().any(|signal| content.contains(signal))
    }
}

struct PathPrefixRule {
    id: String,
    confidence: f32,
    roles: Vec<String>,
    prefixes: Vec<String>,
}

impl SmartRule for PathPrefixRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn confidence(&self) -> f32 {
        self.confidence
    }

    fn matches(&self, file: &ScannedFile, role: FileRole) -> bool {
        if !role_matches(&self.roles, role) {
            return false;
        }
        let path = file.relative_path.to_lowercase();
        self.prefixes.iter().any(|prefix| path.starts_with(prefix))
    }
}

struct ExtensionRule {
    id: String,
    confidence: f32,
    roles: Vec<String>,
    extensions: Vec<String>,
}

impl SmartRule for ExtensionRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn confidence(&self) -> f32 {
        self.confidence
    }

    fn matches(&self, file: &ScannedFile, role: FileRole) -> bool {
        if !role_matches(&self.roles, role) {
            return false;
        }
        let ext = file.extension().to_lowercase();
        self.extensions.iter().any(|item| item == &ext)
    }
}

struct FileNameContainsRule {
    id: String,
    confidence: f32,
    roles: Vec<String>,
    terms: Vec<String>,
}

impl SmartRule for FileNameContainsRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn confidence(&self) -> f32 {
        self.confidence
    }

    fn matches(&self, file: &ScannedFile, role: FileRole) -> bool {
        if !role_matches(&self.roles, role) {
            return false;
        }
        let file_name = file.file_name().to_lowercase();
        self.terms.iter().any(|term| file_name.contains(term))
    }
}

struct MinSizeRule {
    id: String,
    confidence: f32,
    roles: Vec<String>,
    min_size_bytes: usize,
}

impl SmartRule for MinSizeRule {
    fn id(&self) -> &str {
        &self.id
    }

    fn confidence(&self) -> f32 {
        self.confidence
    }

    fn matches(&self, file: &ScannedFile, role: FileRole) -> bool {
        role_matches(&self.roles, role) && file.size_bytes >= self.min_size_bytes
    }
}

fn build_rule(def: &SmartRuleDef) -> Option<Box<dyn SmartRule>> {
    if !def.enabled {
        return None;
    }

    let id = def.id.trim().to_lowercase();
    if id.is_empty() {
        return None;
    }

    let confidence = def.confidence.clamp(0.05, 1.0);
    let roles: Vec<String> = def
        .roles
        .iter()
        .map(|item| item.trim().to_lowercase())
        .filter(|item| !item.is_empty())
        .collect();

    match def.kind.trim().to_lowercase().as_str() {
        "always_role" => Some(Box::new(AlwaysRoleRule {
            id,
            confidence,
            roles,
        })),
        "content_contains_any" => {
            let signals = def
                .any_signals
                .iter()
                .map(|item| item.trim().to_lowercase())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>();
            if signals.is_empty() {
                return None;
            }
            Some(Box::new(ContentContainsAnyRule {
                id,
                confidence,
                roles,
                signals,
            }))
        }
        "path_prefix" => {
            let prefixes = def
                .path_prefixes
                .iter()
                .map(|item| item.trim().to_lowercase())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>();
            if prefixes.is_empty() {
                return None;
            }
            Some(Box::new(PathPrefixRule {
                id,
                confidence,
                roles,
                prefixes,
            }))
        }
        "extension_in" => {
            let extensions = def
                .extensions
                .iter()
                .map(|item| item.trim().trim_start_matches('.').to_lowercase())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>();
            if extensions.is_empty() {
                return None;
            }
            Some(Box::new(ExtensionRule {
                id,
                confidence,
                roles,
                extensions,
            }))
        }
        "file_name_contains" => {
            let terms = def
                .file_name_contains
                .iter()
                .map(|item| item.trim().to_lowercase())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>();
            if terms.is_empty() {
                return None;
            }
            Some(Box::new(FileNameContainsRule {
                id,
                confidence,
                roles,
                terms,
            }))
        }
        "min_file_size" => Some(Box::new(MinSizeRule {
            id,
            confidence,
            roles,
            min_size_bytes: def.min_size_bytes.unwrap_or(0),
        })),
        _ => None,
    }
}

pub(crate) struct SmartRuleEngine {
    min_confidence: f32,
    rules: Vec<Box<dyn SmartRule>>,
}

impl SmartRuleEngine {
    pub(crate) fn evaluate(&self, file: &ScannedFile, role: FileRole) -> SmartTriageDecision {
        if self.rules.is_empty() {
            return SmartTriageDecision::skip();
        }

        let mut matched_rules = Vec::new();
        let mut max_conf = 0.0_f32;

        for rule in &self.rules {
            if !rule.matches(file, role) {
                continue;
            }
            let confidence = rule.confidence().clamp(0.05, 1.0);
            max_conf = max_conf.max(confidence);
            matched_rules.push(SmartRuleMatch {
                id: rule.id().to_string(),
                confidence,
            });
        }

        if matched_rules.is_empty() {
            return SmartTriageDecision::skip();
        }

        SmartTriageDecision {
            should_analyze: max_conf >= self.min_confidence,
            confidence: max_conf,
            matched_rules,
        }
    }
}

fn rules_path() -> PathBuf {
    paths::security_scan_smart_rules_path()
}

fn parse_rule_set(raw: &str) -> Option<SmartRuleSet> {
    serde_yaml::from_str::<SmartRuleSet>(raw).ok()
}

fn default_rule_set() -> SmartRuleSet {
    parse_rule_set(DEFAULT_SMART_RULES_YAML).unwrap_or_else(|| SmartRuleSet {
        min_confidence: default_min_confidence(),
        rules: Vec::new(),
    })
}

fn effective_rule_set() -> SmartRuleSet {
    let fallback = default_rule_set();
    if let Ok(raw) = std::fs::read_to_string(rules_path()) {
        if let Some(custom) = parse_rule_set(&raw) {
            return custom;
        }
    }
    fallback
}

pub(crate) fn load_engine() -> SmartRuleEngine {
    let rule_set = effective_rule_set();
    let mut rules = Vec::new();
    for def in &rule_set.rules {
        if let Some(rule) = build_rule(def) {
            rules.push(rule);
        }
    }
    SmartRuleEngine {
        min_confidence: rule_set.min_confidence.clamp(0.05, 1.0),
        rules,
    }
}
