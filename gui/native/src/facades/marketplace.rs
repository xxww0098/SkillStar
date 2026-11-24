//! Marketplace-domain facade. The market page base (wave 2) owns this file:
//! the local-first read-side state machine needs the four snapshot readers,
//! the scope sync, and install/uninstall (mod.rs `refresh`/`sync_snapshot`/
//! `set_installed` parity). The detail-drawer and publisher slices land in
//! their own pre-registered facades (`marketplace_drawer.rs`,
//! `marketplace_publisher.rs`); nothing here is theirs.
//!
//! Every read goes through `ss_marketplace::snapshot` over the same
//! `~/.skillstar` store the GPUI shell opens (bootstrap.rs `runtime_config`
//! parity — the bridge crate cannot depend on ss-app, so the three-line
//! wiring is repeated here). Reads are read-only; the two writers
//! (`install_skill` / `uninstall_skill`) mirror the GPUI `spawn_domain`
//! jobs, including their `spawn_blocking` shape.

use nsh_core::prelude::*;
use nsh_core::JsonValue;
use ss_core::types::skill::{OfficialPublisher, Skill, SkillCategory, UpstreamChange};
use ss_marketplace::snapshot::{
    self as market_snapshot, InstalledSkillsFuture, LocalFirstResult, SnapshotStatus,
    get_leaderboard_local, get_publishers_local, search_local, sync_marketplace_scope,
};
use ss_marketplace::SnapshotRuntimeConfig;

// The crate cannot depend on serde_json directly, so the JSON builders go
// through nsh-core's re-exports (same trick as the deck facades).
use nsh_core::element::__private::JsonMap;

/// The marketplace seam (`Marketplace` in TSX).
#[module]
pub struct Marketplace;

/// `SnapshotStatus` as the page's string tag (Fresh/Stale/Seizing/Miss/
/// ErrorFallback/RemoteError become the banner vocabulary).
fn status_tag(status: SnapshotStatus) -> &'static str {
    match status {
        SnapshotStatus::Fresh => "fresh",
        SnapshotStatus::Stale => "stale",
        SnapshotStatus::Seeding => "seeding",
        SnapshotStatus::Miss => "miss",
        SnapshotStatus::ErrorFallback => "error_fallback",
        SnapshotStatus::RemoteError => "remote_error",
    }
}

fn lookup_localized_description(
    skill: &Skill,
    config: Option<&ss_core::translation::TranslationConfig>,
) -> Option<String> {
    if let Some(text) = &skill.localized_description {
        return Some(text.clone());
    }
    let cfg = config?;
    let enabled = cfg
        .description_choices
        .get(&skill.name)
        .copied()
        .unwrap_or(cfg.translate_descriptions);
    if !enabled {
        return None;
    }
    let target = ss_core::translation::canonical_target(&cfg.target_lang);
    if !ss_core::translation::needs_translation(&skill.description, target) {
        return None;
    }
    ss_core::translation::lookup(&skill.description, target)
}

#[cfg_attr(not(test), allow(dead_code))]
fn skill_json(skill: &Skill) -> JsonValue {
    let config = ss_core::translation::load_config().ok();
    skill_json_with_config(skill, config.as_ref())
}

fn skill_json_with_config(
    skill: &Skill,
    config: Option<&ss_core::translation::TranslationConfig>,
) -> JsonValue {
    let mut map = JsonMap::new();
    map.insert("name".to_owned(), JsonValue::String(skill.name.clone()));
    map.insert(
        "description".to_owned(),
        JsonValue::String(skill.description.clone()),
    );
    if let Some(text) = lookup_localized_description(skill, config) {
        map.insert("localizedDescription".to_owned(), JsonValue::String(text));
    }
    map.insert(
        "skillType".to_owned(),
        JsonValue::String(
            match skill.skill_type {
                ss_core::types::skill::SkillType::Hub => "hub",
                ss_core::types::skill::SkillType::Local => "local",
            }
            .to_owned(),
        ),
    );
    map.insert("stars".to_owned(), JsonValue::from(skill.stars as u64));
    map.insert("installed".to_owned(), JsonValue::Bool(skill.installed));
    map.insert(
        "updateAvailable".to_owned(),
        JsonValue::Bool(skill.update_available),
    );
    map.insert(
        "lastUpdated".to_owned(),
        JsonValue::String(skill.last_updated.clone()),
    );
    map.insert("gitUrl".to_owned(), JsonValue::String(skill.git_url.clone()));
    map.insert(
        "category".to_owned(),
        JsonValue::String(
            match skill.category {
                SkillCategory::Hot => "hot",
                SkillCategory::Popular => "popular",
                SkillCategory::Rising => "rising",
                SkillCategory::New => "new",
                SkillCategory::None => "none",
            }
            .to_owned(),
        ),
    );
    if let Some(author) = &skill.author {
        map.insert("author".to_owned(), JsonValue::String(author.clone()));
    }
    if let Some(rank) = skill.rank {
        map.insert("rank".to_owned(), JsonValue::from(rank as u64));
    }
    if let Some(source) = &skill.source {
        map.insert("source".to_owned(), JsonValue::String(source.clone()));
    }
    if let Some(called) = &skill.called {
        map.insert("called".to_owned(), JsonValue::String(called.clone()));
    }
    // The one upstream state the market card paints: 「已下架」.
    let removed = matches!(skill.upstream_change, Some(UpstreamChange::Removed { .. }));
    map.insert("upstreamRemoved".to_owned(), JsonValue::Bool(removed));
    JsonValue::Object(map)
}

fn error_json(status: SnapshotStatus, scope: &str, error: Option<String>) -> JsonValue {
    match error {
        Some(detail) => {
            let mut err = JsonMap::new();
            err.insert(
                "kind".to_owned(),
                JsonValue::String(status_tag(status).to_owned()),
            );
            err.insert("scope".to_owned(), JsonValue::String(scope.to_owned()));
            err.insert("detail".to_owned(), JsonValue::String(detail));
            JsonValue::Object(err)
        }
        None => JsonValue::Null,
    }
}

fn skills_result(scope: &str, result: LocalFirstResult<Vec<Skill>>) -> JsonValue {
    let config = ss_core::translation::load_config().ok();
    let mut map = JsonMap::new();
    map.insert(
        "data".to_owned(),
        JsonValue::Array(
            result
                .data
                .iter()
                .map(|skill| skill_json_with_config(skill, config.as_ref()))
                .collect(),
        ),
    );
    map.insert(
        "snapshotStatus".to_owned(),
        JsonValue::String(status_tag(result.snapshot_status).to_owned()),
    );
    map.insert(
        "snapshotUpdatedAt".to_owned(),
        match result.snapshot_updated_at {
            Some(text) => JsonValue::String(text),
            None => JsonValue::Null,
        },
    );
    map.insert(
        "error".to_owned(),
        error_json(result.snapshot_status, scope, result.error),
    );
    JsonValue::Object(map)
}

fn publishers_result(scope: &str, result: LocalFirstResult<Vec<OfficialPublisher>>) -> JsonValue {
    let mut map = JsonMap::new();
    map.insert(
        "data".to_owned(),
        JsonValue::Array(
            result
                .data
                .iter()
                .map(|pub_| {
                    let mut entry = JsonMap::new();
                    entry.insert("name".to_owned(), JsonValue::String(pub_.name.clone()));
                    entry.insert("repo".to_owned(), JsonValue::String(pub_.repo.clone()));
                    entry.insert(
                        "repoCount".to_owned(),
                        JsonValue::from(pub_.repo_count as u64),
                    );
                    entry.insert(
                        "skillCount".to_owned(),
                        JsonValue::from(pub_.skill_count as u64),
                    );
                    entry.insert("url".to_owned(), JsonValue::String(pub_.url.clone()));
                    JsonValue::Object(entry)
                })
                .collect(),
        ),
    );
    map.insert(
        "snapshotStatus".to_owned(),
        JsonValue::String(status_tag(result.snapshot_status).to_owned()),
    );
    map.insert(
        "snapshotUpdatedAt".to_owned(),
        match result.snapshot_updated_at {
            Some(text) => JsonValue::String(text),
            None => JsonValue::Null,
        },
    );
    map.insert(
        "error".to_owned(),
        error_json(result.snapshot_status, scope, result.error),
    );
    JsonValue::Object(map)
}

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static RUNTIME_READY_PATHS: OnceLock<Mutex<Option<(PathBuf, PathBuf)>>> = OnceLock::new();

/// The snapshot runtime wiring bootstrap.rs owns on the GPUI side, repeated
/// because the bridge cannot depend on ss-app: the real data dir, the
/// installed-skill markers, and the installed-skill loader. Cached per
/// `(db_path, data_root)` and initialized on the blocking pool so reads do not
/// re-run schema setup or touch SQLite on Tokio async worker threads.
async fn ensure_runtime() -> Result<(), String> {
    let db_path = ss_core::infra::paths::marketplace_db_path();
    let data_root = ss_core::infra::paths::data_root();
    {
        let guard = RUNTIME_READY_PATHS
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if guard.as_ref() == Some(&(db_path.clone(), data_root.clone())) {
            return Ok(());
        }
    }
    let db_for_init = db_path.clone();
    let root_for_init = data_root.clone();
    blocking(move || {
        let config = SnapshotRuntimeConfig::new(
            db_for_init,
            root_for_init,
            ss_skills::installed_skill::installed_snapshot_markers,
            || -> InstalledSkillsFuture {
                Box::pin(ss_skills::installed_skill::list_installed_skills())
            },
        );
        market_snapshot::configure_runtime(config);
        market_snapshot::initialize().map_err(|err| format!("{err:#}"))
    })
    .await
    .map_err(|join| format!("runtime init task failed: {join}"))??;

    let mut guard = RUNTIME_READY_PATHS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    *guard = Some((db_path, data_root));
    Ok(())
}

#[module]
impl Marketplace {
    /// One leaderboard read — the all/trending/hot tabs
    /// (`get_leaderboard_local`, mod.rs `refresh` query-empty branch).
    /// Read-only local-first: falls back to remote inside the domain.
    /// Resolves `{data, snapshotStatus, snapshotUpdatedAt, error}`.
    pub async fn leaderboard(category: String) -> Result<JsonValue, String> {
        ensure_runtime().await?;
        let scope = format!(
            "leaderboard_{}",
            if category.trim().is_empty() {
                "all".to_owned()
            } else {
                category.trim().to_ascii_lowercase()
            }
        );
        get_leaderboard_local(&category)
            .await
            .map(|result| skills_result(&scope, result))
            .map_err(|err| format!("{err:#}"))
    }

    /// One search read (`search_local`, limit 50 like mod.rs). Read-only
    /// local-first; the search seed write happens on `sync_scope`, not here.
    /// Same result shape as `leaderboard`.
    pub async fn search(query: String) -> Result<JsonValue, String> {
        ensure_runtime().await?;
        let scope = format!("search_seed:{}", query.trim().to_ascii_lowercase());
        search_local(&query, Some(50))
            .await
            .map(|result| skills_result(&scope, result))
            .map_err(|err| format!("{err:#}"))
    }

    /// The official publishers list (`get_publishers_local`, the Official
    /// tab's dataset). Read-only local-first. Same result shape with
    /// publisher rows: `{name, repo, repoCount, skillCount, url}`.
    pub async fn publishers() -> Result<JsonValue, String> {
        ensure_runtime().await?;
        get_publishers_local()
            .await
            .map(|result| publishers_result("official_publishers", result))
            .map_err(|err| format!("{err:#}"))
    }

    /// Sync one scope into the local snapshot (`sync_marketplace_scope`,
    /// the toolbar refresh and the banner retry). The scope string comes
    /// from `scope_for`. Also kicks the domain's background description
    /// backfill — the page re-reads once more ~8s later to pick it up
    /// (mod.rs `SYNC_BACKFILL_RELOAD`).
    pub async fn sync_scope(scope: String) -> Result<(), String> {
        ensure_runtime().await?;
        sync_marketplace_scope(&scope)
            .await
            .map(|_| ())
            .map_err(|err| format!("{err:#}"))
    }

    /// Install a skill from its git URL (`GitSkillFacade::install_skill`,
    /// the market card's install action). Runs on the blocking pool like
    /// the GPUI `spawn_domain` job. Resolves with the installed name.
    pub async fn install_skill(url: String, name: String) -> Result<String, String> {
        blocking(move || {
            ss_skills::git_skill::GitSkillFacade::from_file_store()
                .install_skill(url, Some(name))
                .map(|skill| skill.name)
                .map_err(|err| format!("{err:#}"))
        })
        .await
        .map_err(|join| format!("install task failed: {join}"))?
    }

    /// Uninstall the Skill a market row refers to (`skill_install::uninstall_skill`,
    /// the market card's uninstall action after the confirm). The row carries the
    /// upstream `name` and `source`; the installed folder is not always `name`,
    /// because a taken name makes the installer put the Skill in `<name>-<owner>`,
    /// so the folder is resolved the way the row is marked installed. Runs on the
    /// blocking pool. Resolves with the folder that was removed.
    pub async fn uninstall_skill(source: String, name: String) -> Result<String, String> {
        // When the snapshot runtime cannot be set up, or nothing installed
        // answers for the row, fall back to the row's name: nothing is
        // installed under it, so removing only prunes stale entries.
        let _ = ensure_runtime().await;
        let folder = market_snapshot::installed_folder_for(&source, &name)
            .await
            .unwrap_or(name);
        blocking(move || {
            ss_skills::skill_install::uninstall_skill(&folder)
                .map(|_| folder.clone())
                .map_err(|err| format!("{err:#}"))
        })
        .await
        .map_err(|join| format!("uninstall task failed: {join}"))?
    }

    /// Detect whether the Marketplace search bar holds a pasted install command
    /// (`npx skills add ...`, `skillstar install ...`) or a direct GitHub/Git source.
    /// Returns `{ source, raw, skills, wasCommand }` or `null`.
    pub fn parse_direct_source(query: String) -> JsonValue {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return JsonValue::Null;
        }
        let Ok(parsed) = ss_skills::source_resolver::parse_install_input(trimmed) else {
            return JsonValue::Null;
        };
        if !parsed.was_command && !ss_skills::source_resolver::looks_like_direct_source(trimmed) {
            return JsonValue::Null;
        }
        let mut map = JsonMap::new();
        map.insert("source".to_owned(), JsonValue::String(parsed.source));
        map.insert("raw".to_owned(), JsonValue::String(trimmed.to_owned()));
        map.insert(
            "skills".to_owned(),
            JsonValue::Array(parsed.skills.into_iter().map(JsonValue::String).collect()),
        );
        map.insert("wasCommand".to_owned(), JsonValue::Bool(parsed.was_command));
        JsonValue::Object(map)
    }

    /// Seed sparse local search results (<5 hits, subject to 10-minute cooldown)
    /// into the local marketplace SQLite snapshot.
    pub async fn maybe_seed_search(query: String) -> Result<bool, String> {
        ensure_runtime().await?;
        market_snapshot::maybe_seed_sparse_search(&query)
            .await
            .map_err(|err| format!("{err:#}"))
    }
}

#[cfg(test)]
mod tests {
    use super::Marketplace;
    use ss_core::types::skill::Skill;
    use ss_marketplace::SnapshotStatus;

    #[test]
    fn structured_error_carries_kind_scope_and_detail() {
        let json = super::error_json(
            SnapshotStatus::ErrorFallback,
            "leaderboard_all",
            Some("timeout reaching skills.sh".to_owned()),
        );
        assert_eq!(json["kind"].as_str(), Some("error_fallback"));
        assert_eq!(json["scope"].as_str(), Some("leaderboard_all"));
        assert_eq!(json["detail"].as_str(), Some("timeout reaching skills.sh"));
        assert!(super::error_json(SnapshotStatus::Fresh, "leaderboard_all", None).is_null());
    }

    /// The wire shape every reader resolves with: the four keys and the six
    /// status tags the page's banner vocabulary needs. Built from a fixture
    /// through the real serializer (no IO).
    #[test]
    fn skill_json_carries_the_fields_the_card_paints() {
        let mut skill = Skill::from_skills_sh(
            "demo".into(),
            "A demo skill".into(),
            12,
            "acme".into(),
            "https://github.com/acme/demo".into(),
        );
        skill.rank = Some(3);
        skill.source = Some("acme/demo".into());
        skill.called = Some("原始名".into());
        let json = super::skill_json(&skill);
        assert_eq!(json["name"].as_str(), Some("demo"));
        assert_eq!(json["stars"].as_u64(), Some(12));
        assert_eq!(json["installed"].as_bool(), Some(false));
        assert_eq!(json["rank"].as_u64(), Some(3));
        assert_eq!(json["source"].as_str(), Some("acme/demo"));
        assert_eq!(json["called"].as_str(), Some("原始名"));
        assert_eq!(json["upstreamRemoved"].as_bool(), Some(false));
        assert!(json["description"].as_str().is_some());
    }

    #[test]
    fn parse_direct_source_recognizes_commands_and_direct_repos() {
        let cmd = Marketplace::parse_direct_source(
            "npx skills add vercel-labs/agent-skills --skill find-skills".into(),
        );
        assert_eq!(cmd["source"].as_str(), Some("vercel-labs/agent-skills"));
        assert_eq!(cmd["wasCommand"].as_bool(), Some(true));
        assert_eq!(cmd["skills"][0].as_str(), Some("find-skills"));

        let direct = Marketplace::parse_direct_source("acme/skills".into());
        assert_eq!(direct["source"].as_str(), Some("acme/skills"));
        assert_eq!(direct["wasCommand"].as_bool(), Some(false));

        assert!(Marketplace::parse_direct_source("python linter".into()).is_null());
        assert!(Marketplace::parse_direct_source("   ".into()).is_null());
    }

    #[test]
    fn status_tags_cover_the_six_banner_states() {
        use super::status_tag;
        use ss_marketplace::SnapshotStatus;
        let all = [
            status_tag(SnapshotStatus::Fresh),
            status_tag(SnapshotStatus::Stale),
            status_tag(SnapshotStatus::Seeding),
            status_tag(SnapshotStatus::Miss),
            status_tag(SnapshotStatus::ErrorFallback),
            status_tag(SnapshotStatus::RemoteError),
        ];
        assert_eq!(all, ["fresh", "stale", "seeding", "miss", "error_fallback", "remote_error"]);
    }
}
