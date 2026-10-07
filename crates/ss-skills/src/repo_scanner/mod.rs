//! Repo-scan IPC shapes and the scan helpers over a fetched checkout.
//!
//! Callers acquire a checkout through `crate::fetch` and scan it here. Install lives in
//! `crate::installer`.

pub mod scan;

use serde::{Deserialize, Serialize};

pub use crate::discovery::DiscoveredSkill;
pub use scan::{scan_skills_in_repo, scan_skills_in_repo_at};

/// `spec` flattens `source_resolver::Source` — the single owner of the
/// parsed input (ref, subpath, skill filter) — instead of `ScanResult`
/// keeping its own `(short, repo_url)` pair. JSON stays backward compatible:
/// `Source` serializes under the same `source`/`source_url` keys this struct
/// exposed before, plus the new `git_ref`/`subpath`/`skill_filter` fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    #[serde(flatten)]
    pub spec: crate::source_resolver::Source,
    pub skills: Vec<DiscoveredSkill>,
    /// Present when the repo declares a Claude Code plugin with hooks/agents
    /// SkillStar will not install — see `plugin_manifest::plugin_hint`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<crate::plugin_manifest::PluginHint>,
    /// Immutable preview commit. Unpinned local folders are read live.
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub cache_hit: bool,
    #[serde(default)]
    pub cached_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstallTarget {
    pub id: String,
    pub folder_path: String,
    /// Hard-pin the resulting lock entry to `folder_path`. IPC contract stays
    /// backward compatible: an omitted field defaults to `false`.
    #[serde(default)]
    pub pinned: bool,
}
