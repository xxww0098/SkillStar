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

/// The scope string `sync_scope` expects for a tab + query pair — the exact
/// strings `parse_scope` understands (sync_state.rs:46-88) and the ones the
/// GPUI toolbar builds (mod.rs:324-330). Exposed so the page (and its tests)
/// never hand-assemble them wrong.
fn scope_for(tab: &str, query: &str) -> String {
    let query = query.trim();
    if !query.is_empty() {
        format!("search_seed:{}", query.to_ascii_lowercase())
    } else if tab == "official" {
        "official_publishers".to_owned()
    } else {
        format!("leaderboard_{tab}")
    }
}

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

fn skill_json(skill: &Skill) -> JsonValue {
    let mut map = JsonMap::new();
    map.insert("name".to_owned(), JsonValue::String(skill.name.clone()));
    map.insert(
        "description".to_owned(),
        JsonValue::String(skill.description.clone()),
    );
    if let Some(text) = &skill.localized_description {
        map.insert(
            "localizedDescription".to_owned(),
            JsonValue::String(text.clone()),
        );
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

fn skills_result(result: LocalFirstResult<Vec<Skill>>) -> JsonValue {
    let mut map = JsonMap::new();
    map.insert(
        "data".to_owned(),
        JsonValue::Array(result.data.iter().map(skill_json).collect()),
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
        match result.error {
            Some(text) => JsonValue::String(text),
            None => JsonValue::Null,
        },
    );
    JsonValue::Object(map)
}

fn publishers_result(result: LocalFirstResult<Vec<OfficialPublisher>>) -> JsonValue {
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
        match result.error {
            Some(text) => JsonValue::String(text),
            None => JsonValue::Null,
        },
    );
    JsonValue::Object(map)
}

/// The snapshot runtime wiring bootstrap.rs owns on the GPUI side, repeated
/// because the bridge cannot depend on ss-app: the real data dir, the
/// installed-skill markers, and the installed-skill loader. Called before
/// every read (configure_runtime is a cheap RwLock write that also clears
/// the pool when the db path changed); migrations run once per path inside
/// `initialize`.
fn ensure_runtime() -> Result<(), String> {
    let config = SnapshotRuntimeConfig::new(
        ss_core::infra::paths::marketplace_db_path(),
        ss_core::infra::paths::data_root(),
        ss_skills::installed_skill::installed_snapshot_markers,
        || -> InstalledSkillsFuture {
            Box::pin(ss_skills::installed_skill::list_installed_skills())
        },
    );
    market_snapshot::configure_runtime(config);
    market_snapshot::initialize().map_err(|err| format!("{err:#}"))
}

#[module]
impl Marketplace {
    /// True when `repoUrl` is an exact `https://github.com/owner/repo`
    /// clone URL — the only source SkillStar may query install-time
    /// security audits for. Pure URL-shape check: read-only, no IO, no side
    /// effects. The same gate the GPUI import dialog uses.
    pub fn may_query_install_audits(repo_url: String) -> bool {
        ss_marketplace::may_query_install_audits(&repo_url)
    }

    /// The scope a refresh button press should sync for this tab + query
    /// (`search_seed:<q>` / `official_publishers` / `leaderboard_<tab>`).
    /// Pure; the page hands the result back to `sync_scope`.
    pub fn scope_for(tab: String, query: String) -> String {
        scope_for(&tab, &query)
    }

    /// One leaderboard read — the all/trending/hot tabs
    /// (`get_leaderboard_local`, mod.rs `refresh` query-empty branch).
    /// Read-only local-first: falls back to remote inside the domain.
    /// Resolves `{data, snapshotStatus, snapshotUpdatedAt, error}`.
    pub async fn leaderboard(category: String) -> Result<JsonValue, String> {
        ensure_runtime()?;
        get_leaderboard_local(&category)
            .await
            .map(skills_result)
            .map_err(|err| format!("{err:#}"))
    }

    /// One search read (`search_local`, limit 50 like mod.rs). Read-only
    /// local-first; the search seed write happens on `sync_scope`, not here.
    /// Same result shape as `leaderboard`.
    pub async fn search(query: String) -> Result<JsonValue, String> {
        ensure_runtime()?;
        search_local(&query, Some(50))
            .await
            .map(skills_result)
            .map_err(|err| format!("{err:#}"))
    }

    /// The official publishers list (`get_publishers_local`, the Official
    /// tab's dataset). Read-only local-first. Same result shape with
    /// publisher rows: `{name, repo, repoCount, skillCount, url}`.
    pub async fn publishers() -> Result<JsonValue, String> {
        ensure_runtime()?;
        get_publishers_local()
            .await
            .map(publishers_result)
            .map_err(|err| format!("{err:#}"))
    }

    /// Sync one scope into the local snapshot (`sync_marketplace_scope`,
    /// the toolbar refresh and the banner retry). The scope string comes
    /// from `scope_for`. Also kicks the domain's background description
    /// backfill — the page re-reads once more ~8s later to pick it up
    /// (mod.rs `SYNC_BACKFILL_RELOAD`).
    pub async fn sync_scope(scope: String) -> Result<(), String> {
        ensure_runtime()?;
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

    /// Uninstall a skill by name (`skill_install::uninstall_skill`, the
    /// market card's uninstall action after the confirm). Runs on the
    /// blocking pool. Resolves with the name.
    pub async fn uninstall_skill(name: String) -> Result<String, String> {
        blocking(move || {
            ss_skills::skill_install::uninstall_skill(&name)
                .map(|_| name.clone())
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
        ensure_runtime()?;
        market_snapshot::maybe_seed_sparse_search(&query)
            .await
            .map_err(|err| format!("{err:#}"))
    }
}

#[cfg(test)]
mod tests {
    use super::Marketplace;
    use ss_core::types::skill::Skill;

    #[test]
    fn gates_exact_github_clone_urls_only() {
        assert!(Marketplace::may_query_install_audits(
            "https://github.com/anthropics/skills".to_owned()
        ));
        assert!(!Marketplace::may_query_install_audits(
            "https://gitlab.com/owner/repo".to_owned()
        ));
        assert!(!Marketplace::may_query_install_audits(
            "https://github.com/anthropics/skills/tree/main".to_owned()
        ));
        assert!(!Marketplace::may_query_install_audits(String::new()));
    }

    /// mod.rs:324-330 — the refresh scope a tab + query maps onto. Pure, so
    /// the vocabulary is locked without touching the network or the db.
    #[test]
    fn scope_for_matches_the_sync_grammar() {
        assert_eq!(
            Marketplace::scope_for("all".into(), "".into()),
            "leaderboard_all"
        );
        assert_eq!(
            Marketplace::scope_for("trending".into(), "  ".into()),
            "leaderboard_trending"
        );
        assert_eq!(
            Marketplace::scope_for("official".into(), "".into()),
            "official_publishers"
        );
        // Search wins over the tab and case-folds (search_seed_scope parity).
        assert_eq!(
            Marketplace::scope_for("hot".into(), "  Rust  ".into()),
            "search_seed:rust"
        );
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
