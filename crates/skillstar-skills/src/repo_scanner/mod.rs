pub mod cache;
pub mod detect;
pub(crate) mod inventory;
pub mod maintenance;
pub mod ops;
pub mod scan;
pub mod scan_install;

use anyhow::Context;
use serde::{Deserialize, Serialize};

pub use crate::discovery::DiscoveredSkill;

pub use cache::{
    cache_dir_name, cache_key_for, cached_repo_dir_if_present, clone_or_fetch_repo_at_in_session,
    clone_or_fetch_repo_in_session, existing_hub_checkout, existing_repo_cache_dir,
};
pub use detect::detect_new_skills_in_cached_repos;
pub(crate) use detect::{skill_at_revision, upstream_added_dirs};
pub use maintenance::{RepoCacheInfo, clean_unused_cache, get_cache_info};
pub use ops::pull_repo_skill_update_in_session;
pub use scan::{scan_skills_in_repo, scan_skills_in_repo_at};
pub use scan_install::{
    install_from_repo_at, install_from_repo_at_with_source_migrations, install_from_repo_in_session,
};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoNewSkill {
    pub repo_source: String,
    pub repo_url: String,
    pub skill_id: String,
    pub folder_path: String,
    pub description: String,
    /// Installed Skill the last update check identified this one as the
    /// successor of — the source renamed or moved it here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renamed_from: Option<String>,
}

pub(crate) fn scan_repo_with_mode_in_session(
    input: &str,
    full_depth: bool,
    session: &crate::git::transport::GitOperationSession,
) -> anyhow::Result<ScanResult> {
    let parsed = crate::source_resolver::Source::parse(input).context("Invalid repository URL")?;
    crate::skill_mutation::policy().ensure_repository_mutation_allowed(&parsed.repo_url)?;
    let repo_dir = clone_or_fetch_repo_at_in_session(
        &parsed.repo_url,
        &parsed.short,
        parsed.git_ref.as_deref(),
        session,
    )?;
    if let Some(subpath) = parsed.subpath.as_deref() {
        // A sparse cold clone (06) never materializes an arbitrary subpath on
        // its own — only the pin (materialize_dirs, done at install time) or
        // the default representative copy do. The scan preview must see the
        // same pinned folder the install would use.
        inventory::materialize_dirs(&repo_dir, session, &[subpath.to_string()]);
    }
    let (_, _, repo_dir, skills) =
        crate::skill_install::scan_parsed_checkout(&parsed, repo_dir, full_depth);
    let plugin = crate::plugin_manifest::plugin_hint_for_repo(&repo_dir);
    Ok(ScanResult {
        spec: parsed,
        skills,
        plugin,
    })
}
