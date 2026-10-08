use std::collections::BTreeSet;

use thiserror::Error;

use crate::marketplace::{MarketplaceManifest, PluginSource};

/// 官方 `add` 流程拒绝的保留市场名；生成端同样拒绝，避免产出装不进去的市场。
pub const RESERVED_MARKETPLACE_NAMES: &[&str] = &["claude-plugins-official"];

const MAX_NAME_CHARS: usize = 64;

#[derive(Debug, Error)]
pub enum MarketplaceError {
    #[error("invalid {what} `{value}`: {reason}")]
    InvalidName {
        what: &'static str,
        value: String,
        reason: &'static str,
    },
    #[error("reserved marketplace name `{0}`; pick another name")]
    ReservedName(String),
    #[error("invalid source path `{value}`: {reason}")]
    InvalidSourcePath { value: String, reason: &'static str },
    #[error("duplicate plugin entry name `{0}`")]
    DuplicateEntry(String),
    #[error("plugin source paths overlap: `{0}` is inside `{1}` or vice versa")]
    OverlappingSourcePaths(String, String),
    #[error("plugin entry `{entry}` has a manifest name mismatch: `{manifest}`")]
    EntryNameMismatch { entry: String, manifest: String },
    #[error("plugin source must be a relative path for export, got `{0}`")]
    NotRelativeSource(String),
    #[error("duplicate skill directory name `{0}` in one plugin payload")]
    DuplicateSkillDir(String),
    #[error("symlinks are not supported in exported skill content: {0}")]
    UnsupportedSymlink(String),
    #[error("copy limit exceeded at {path}: {reason}")]
    CopyLimit { path: String, reason: &'static str },
    #[error("output directory is not empty: {0}")]
    OutputNotEmpty(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// 市场名与插件条目名共用的保守字符集：小写字母、数字与连字符，首尾为字母数字。
/// SkillStar 侧把任意技能名净化成该形态后才进 schema；解析外部清单同样按此校验。
pub fn validate_entry_name(name: &str) -> Result<(), MarketplaceError> {
    check_name(name, "plugin entry name")
}

pub fn validate_marketplace_name(name: &str) -> Result<(), MarketplaceError> {
    check_name(name, "marketplace name")?;
    if RESERVED_MARKETPLACE_NAMES.contains(&name) {
        return Err(MarketplaceError::ReservedName(name.to_string()));
    }
    Ok(())
}

fn check_name(name: &str, what: &'static str) -> Result<(), MarketplaceError> {
    let invalid = |reason: &'static str| MarketplaceError::InvalidName {
        what,
        value: name.to_string(),
        reason,
    };
    if name.is_empty() {
        return Err(invalid("must not be empty"));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(invalid("must be at most 64 characters"));
    }
    if name.chars().any(|ch| !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')) {
        return Err(invalid(
            "only lowercase letters, digits, and hyphens are allowed",
        ));
    }
    let first_ok = name
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric());
    let last_ok = name
        .chars()
        .last()
        .is_some_and(|ch| ch.is_ascii_alphanumeric());
    if !first_ok || !last_ok {
        return Err(invalid("must start and end with a letter or digit"));
    }
    Ok(())
}

/// 相对源路径必须是 `./` 开头的正斜杠路径，无 `.`/`..`/空组件、无反斜杠、无尾斜杠。
pub fn validate_relative_source(source: &str) -> Result<(), MarketplaceError> {
    relative_components(source).map(|_| ())
}

/// 把 `./a/b` 拆成干净组件，供写出侧拼路径；任何不合规形态都拒绝。
pub fn relative_components(source: &str) -> Result<Vec<String>, MarketplaceError> {
    let invalid = |reason: &'static str| MarketplaceError::InvalidSourcePath {
        value: source.to_string(),
        reason,
    };
    let Some(rest) = source.strip_prefix("./") else {
        return Err(invalid("must start with ./"));
    };
    if source.contains('\\') {
        return Err(invalid("backslashes are not allowed"));
    }
    if rest.is_empty() {
        return Err(invalid("path is empty"));
    }
    let mut components = Vec::new();
    for component in rest.split('/') {
        if component.is_empty() {
            return Err(invalid("empty path component (double slash or trailing slash)"));
        }
        if component == "." || component == ".." {
            return Err(invalid("dot components are not allowed"));
        }
        components.push(component.to_string());
    }
    Ok(components)
}

/// 整份市场一致性：市场名合法、非保留，条目名合法且唯一，相对源路径合法。
pub fn validate_marketplace(manifest: &MarketplaceManifest) -> Result<(), MarketplaceError> {
    validate_marketplace_name(&manifest.name)?;
    let mut seen = BTreeSet::new();
    for entry in &manifest.plugins {
        validate_entry_name(&entry.name)?;
        if !seen.insert(entry.name.clone()) {
            return Err(MarketplaceError::DuplicateEntry(entry.name.clone()));
        }
        if let PluginSource::Relative(source) = &entry.source {
            relative_components(source)?;
        }
    }
    Ok(())
}
