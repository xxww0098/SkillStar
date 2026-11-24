//! Publisher-detail facade — the `marketplace/publisher/` slice
//! (`mod.rs` + `board.rs` + `card.rs` + `hero.rs`) owns this file.
//! Pre-registered by the market base wave so the slice never touches a
//! shared registry; the hero board's core read ships here so the slice
//! starts from a live seam.
//!
//! The slice's remaining domain seams, all already covered elsewhere:
//! - `get_repo_skills_local(source)` — a repo's skill grid; add the wrap in
//!   this file when the board drills into one (the cards reuse the base
//!   page's market-card body);
//! - `sync_marketplace_scope("publisher_repos:<name>")` and
//!   `"repo_skills:<owner>/<repo>"` — the base's `Marketplace.sync_scope`;
//! - publisher rows themselves come from the base page's
//!   `Marketplace.publishers()` snapshot (no separate list call exists).
//!
//! Shell contract (nav.ts railPage): the publisher page is a market
//! drill-down — the sidebar selection box STAYS on the market row while it
//! shows. The base already wires `selectPublisher` (events.ts) to hop there;
//! keep that channel as the only way in.

use nsh_core::prelude::*;
use nsh_core::JsonValue;
use ss_core::types::skill::{Skill, SkillCategory, SkillType, UpstreamChange};
use ss_marketplace::snapshot::{
    self as market_snapshot, InstalledSkillsFuture, LocalFirstResult, SnapshotStatus,
    get_publisher_repos_local, get_repo_skills_local, repo_description_progress,
    repo_description_running, reread_repo_skills_local,
};
use ss_marketplace::{PublisherRepo, SnapshotRuntimeConfig};

use nsh_core::element::__private::JsonMap;

/// The publisher-detail seam (`MarketplacePublisher` in TSX).
#[module]
pub struct MarketplacePublisher;

fn opt_str(value: &Option<String>) -> JsonValue {
    match value {
        Some(text) => JsonValue::String(text.clone()),
        None => JsonValue::Null,
    }
}

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

fn repos_result(scope: &str, result: LocalFirstResult<Vec<PublisherRepo>>) -> JsonValue {
    let mut map = JsonMap::new();
    map.insert(
        "data".to_owned(),
        JsonValue::Array(
            result
                .data
                .iter()
                .map(|repo| {
                    let mut entry = JsonMap::new();
                    entry.insert("repo".to_owned(), JsonValue::String(repo.repo.clone()));
                    entry.insert("source".to_owned(), JsonValue::String(repo.source.clone()));
                    entry.insert(
                        "skillCount".to_owned(),
                        JsonValue::from(repo.skill_count as u64),
                    );
                    entry.insert(
                        "installsLabel".to_owned(),
                        JsonValue::String(repo.installs_label.clone()),
                    );
                    entry.insert("installs".to_owned(), JsonValue::from(repo.installs as u64));
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
        opt_str(&result.snapshot_updated_at),
    );
    map.insert(
        "error".to_owned(),
        error_json(result.snapshot_status, scope, result.error),
    );
    JsonValue::Object(map)
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

/// One repo-skill row, the same wire shape `Marketplace.leaderboard`
/// resolves with (marketplace.rs `skill_json`, repeated here because the
/// slices never share private helpers — the page's `parseSkill` accepts
/// either producer).
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
                SkillType::Hub => "hub",
                SkillType::Local => "local",
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
    map.insert(
        "gitUrl".to_owned(),
        JsonValue::String(skill.git_url.clone()),
    );
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
    let removed = matches!(skill.upstream_change, Some(UpstreamChange::Removed { .. }));
    map.insert("upstreamRemoved".to_owned(), JsonValue::Bool(removed));
    JsonValue::Object(map)
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
        opt_str(&result.snapshot_updated_at),
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
/// from marketplace.rs (the slices never share private helpers), cached per
/// `(db_path, data_root)` and initialized on the blocking pool.
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
impl MarketplacePublisher {
    /// One publisher's repo board — the hero's dataset
    /// (`get_publisher_repos_local`, publisher/mod.rs). Read-only
    /// local-first. Resolves `{data: [{repo, source, skillCount,
    /// installsLabel, installs}…], snapshotStatus, snapshotUpdatedAt,
    /// error}`.
    pub async fn publisher_repos(publisher_name: String) -> Result<JsonValue, String> {
        ensure_runtime().await?;
        let scope = format!("publisher_repos:{publisher_name}");
        get_publisher_repos_local(&publisher_name)
            .await
            .map(|result| repos_result(&scope, result))
            .map_err(|err| format!("{err:#}"))
    }

    /// One repo's skill grid (`get_repo_skills_local`, publisher/mod.rs
    /// `load_repo_skills_with`). Read-only local-first; also kicks the
    /// domain's background description round (the page polls
    /// `repo_description_state` and re-reads through `reread_repo_skills`).
    /// Same wire shape as `Marketplace.leaderboard`; drop the answer when the
    /// open repo changed while it was in flight.
    pub async fn repo_skills(source: String) -> Result<JsonValue, String> {
        ensure_runtime().await?;
        let scope = format!("repo_skills:{source}");
        get_repo_skills_local(&source)
            .await
            .map(|result| skills_result(&scope, result))
            .map_err(|err| format!("{err:#}"))
    }

    /// Descriptions only (`reread_repo_skills_local`): pulls rows the
    /// in-flight description round has already written. Does not flip the
    /// grid back to a skeleton and does not start another round.
    pub async fn reread_repo_skills(source: String) -> Result<JsonValue, String> {
        ensure_runtime().await?;
        let scope = format!("repo_skills:{source}");
        reread_repo_skills_local(&source)
            .await
            .map(|result| skills_result(&scope, result))
            .map_err(|err| format!("{err:#}"))
    }

    /// The description round's watermark for `source`
    /// (`repo_description_progress` / `repo_description_running`, sync.rs).
    /// Pure in-process counters: how many description pages this process has
    /// written since the round started, and whether it is still fetching.
    /// Resolves `{progress, running}` — the page polls this instead of
    /// sleeping.
    pub fn repo_description_state(source: String) -> Result<JsonValue, String> {
        let mut map = JsonMap::new();
        map.insert(
            "progress".to_owned(),
            JsonValue::from(repo_description_progress(&source)),
        );
        map.insert("running".to_owned(), JsonValue::Bool(repo_description_running(&source)));
        Ok(JsonValue::Object(map))
    }
}

#[cfg(test)]
mod tests {
    use super::{MarketplacePublisher, skill_json};
    use ss_core::types::skill::Skill;

    /// The wire shape the grid paints, built from a fixture through the real
    /// serializer (no IO): the page's `parseSkill` reads exactly these keys.
    #[test]
    fn skill_json_carries_the_fields_the_repo_grid_paints() {
        let mut skill = Skill::from_skills_sh(
            "demo".into(),
            String::new(),
            7,
            "acme".into(),
            "https://github.com/acme/skills".into(),
        );
        skill.source = Some("acme/skills".into());
        let json = skill_json(&skill);
        assert_eq!(json["name"].as_str(), Some("demo"));
        assert_eq!(json["description"].as_str(), Some(""));
        assert_eq!(json["stars"].as_u64(), Some(7));
        assert_eq!(json["installed"].as_bool(), Some(false));
        assert_eq!(json["source"].as_str(), Some("acme/skills"));
        assert_eq!(json["gitUrl"].as_str(), Some("https://github.com/acme/skills"));
        assert_eq!(json["upstreamRemoved"].as_bool(), Some(false));
    }

    /// The description-round watermark is the pure sync.rs counters: zero
    /// progress and not running for a source this process never fetched.
    /// (The async `repo_skills` readers need a tokio runtime this crate's
    /// test binary does not link — the bridge is a thin no-branch wrap, and
    /// the live path runs in the nsh UI tests through the real module.)
    #[test]
    fn repo_description_state_reads_the_round_counters() {
        let state = MarketplacePublisher::repo_description_state("nvidia/skills".to_owned())
            .expect("pure counter read");
        assert_eq!(state["progress"].as_u64(), Some(0));
        assert_eq!(state["running"].as_bool(), Some(false));
    }
}
