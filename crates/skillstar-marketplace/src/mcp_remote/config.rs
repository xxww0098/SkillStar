//! Persistence for user-added MCP catalog sources.
//!
//! Stored at `<config_dir>/mcp_sources.json`, resolved through
//! `skillstar_core::infra::paths` so `SKILLSTAR_DATA_DIR` keeps working (tests
//! and portable installs depend on that override).
//!
//! Read failures degrade to "no custom sources" rather than erroring: a broken
//! config file must not take the built-in registries offline. Writes go
//! through a temp file + rename so a crash mid-write cannot truncate it.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tracing::warn;
use ts_rs::TS;

use crate::mcp_models::McpCursorStyle;

use super::sources::{
    CUSTOM_SOURCE_PREFIX, McpSourceDescriptor, McpSourceKind, McpSourceLicense, builtin_sources,
};

/// Priority floor for user sources: they never outrank a built-in registry.
const CUSTOM_PRIORITY_BASE: i32 = 50;
/// Pagination circuit breaker for user sources — generous, but an unknown
/// endpoint does not get to page forever.
const CUSTOM_MAX_PAGES: usize = 50;
const CONFIG_VERSION: u32 = 1;

/// A user-added registry URL or local directory file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpCustomSource.ts")]
pub struct McpCustomSource {
    /// Slug (without the `custom:` prefix).
    pub id: String,
    pub display_name: String,
    /// `…/servers` URL, or an absolute path for a local directory file.
    pub target: String,
    #[serde(default)]
    pub kind: McpSourceKind,
    #[serde(default)]
    pub cursor_style: McpCursorStyle,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Optional merge-authority nudge among user sources.
    #[serde(default)]
    pub priority_offset: i32,
}

fn default_true() -> bool {
    true
}

impl McpCustomSource {
    pub fn to_descriptor(&self) -> McpSourceDescriptor {
        McpSourceDescriptor {
            id: format!("{CUSTOM_SOURCE_PREFIX}{}", self.id),
            display_name: self.display_name.clone(),
            base_url: self.target.clone(),
            kind: self.kind,
            cursor_style: self.cursor_style,
            list_query: None,
            requires_key: false,
            license: McpSourceLicense::UserProvided,
            mirrorable: true,
            enabled: self.enabled,
            builtin: false,
            priority: CUSTOM_PRIORITY_BASE.saturating_add(self.priority_offset),
            max_pages: CUSTOM_MAX_PAGES,
        }
    }
}

/// On-disk shape of `mcp_sources.json`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpSourcesConfig.ts")]
pub struct McpSourcesConfig {
    pub version: u32,
    /// User-added sources.
    #[serde(default)]
    pub custom: Vec<McpCustomSource>,
    /// Built-in source ids the user switched off.
    #[serde(default)]
    pub disabled: Vec<String>,
}

impl Default for McpSourcesConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            custom: Vec::new(),
            disabled: Vec::new(),
        }
    }
}

pub fn config_path() -> PathBuf {
    skillstar_core::infra::paths::config_dir().join("mcp_sources.json")
}

/// Load the config, falling back to defaults on any read/parse problem.
pub fn load_config() -> McpSourcesConfig {
    let path = config_path();
    if !path.exists() {
        return McpSourcesConfig::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            match serde_json::from_str::<McpSourcesConfig>(content.trim_start_matches('\u{feff}')) {
                Ok(config) => config,
                Err(err) => {
                    warn!(
                        target: "mcp_marketplace",
                        error = %err,
                        "mcp_sources.json could not be parsed; falling back to built-in sources"
                    );
                    McpSourcesConfig::default()
                }
            }
        }
        Err(err) => {
            warn!(
                target: "mcp_marketplace",
                error = %err,
                "mcp_sources.json could not be read; falling back to built-in sources"
            );
            McpSourcesConfig::default()
        }
    }
}

pub fn save_config(config: &McpSourcesConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let content =
        serde_json::to_string_pretty(config).context("Failed to serialize mcp_sources.json")?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, content).with_context(|| format!("Failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("Failed to replace {}", path.display()))?;
    Ok(())
}

/// Add (or replace) a user source. Validates the target so a typo surfaces
/// here rather than as a mysterious empty catalog later.
pub fn add_custom_source(mut source: McpCustomSource) -> Result<McpSourcesConfig> {
    source.id = normalize_slug(&source.id);
    if source.id.is_empty() {
        bail!("Custom MCP source id must contain letters, digits, '-' or '_'");
    }
    if builtin_sources().iter().any(|b| b.id == source.id) {
        bail!("'{}' is a built-in MCP source id; pick another", source.id);
    }
    if source.display_name.trim().is_empty() {
        source.display_name = source.id.clone();
    }
    validate_target(&source)?;

    let mut config = load_config();
    config.version = CONFIG_VERSION;
    config.custom.retain(|existing| existing.id != source.id);
    config.custom.push(source);
    save_config(&config)?;
    Ok(config)
}

/// Remove a user source by slug (with or without the `custom:` prefix).
pub fn remove_custom_source(id: &str) -> Result<McpSourcesConfig> {
    let slug = normalize_slug(id.trim_start_matches(CUSTOM_SOURCE_PREFIX));
    let mut config = load_config();
    config.version = CONFIG_VERSION;
    config.custom.retain(|existing| existing.id != slug);
    save_config(&config)?;
    Ok(config)
}

/// Enable/disable any source — built-in ids go on the `disabled` list, user
/// sources flip their own flag.
pub fn set_source_enabled(id: &str, enabled: bool) -> Result<McpSourcesConfig> {
    let mut config = load_config();
    config.version = CONFIG_VERSION;
    if let Some(slug) = id.strip_prefix(CUSTOM_SOURCE_PREFIX) {
        let slug = normalize_slug(slug);
        let Some(custom) = config.custom.iter_mut().find(|c| c.id == slug) else {
            bail!("Unknown custom MCP source '{id}'");
        };
        custom.enabled = enabled;
    } else {
        if !builtin_sources().iter().any(|b| b.id == id) {
            bail!("Unknown built-in MCP source '{id}'");
        }
        config.disabled.retain(|existing| existing != id);
        if !enabled {
            config.disabled.push(id.to_string());
        }
    }
    save_config(&config)?;
    Ok(config)
}

fn normalize_slug(raw: &str) -> String {
    raw.trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

fn validate_target(source: &McpCustomSource) -> Result<()> {
    let target = source.target.trim();
    match source.kind {
        McpSourceKind::Registry => {
            if !(target.starts_with("https://") || target.starts_with("http://")) {
                bail!("Custom MCP registry URL must start with http:// or https://");
            }
        }
        McpSourceKind::LocalDirectory => {
            if target.is_empty() {
                bail!("Local MCP directory needs a file path");
            }
            if !PathBuf::from(target).is_absolute() {
                bail!("Local MCP directory path must be absolute");
            }
        }
    }
    Ok(())
}
