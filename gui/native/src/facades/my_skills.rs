//! My-skills page facade — the page base (wave 3) owns this file
//! (`my_skills/mod.rs + toolbar/ + canvas.rs + view.rs + empty_state.rs +
//! filters.rs + updates.rs + repo_source.rs` seam: the installed-skill
//! listing, agent profiles, repo favorites, update checks/batches and
//! per-repo reinstall/remove).
//!
//! The detail drawer, the selection bar and the SKILL.md reader are parallel
//! slices: their facades are pre-registered in `my_skills_drawer.rs`,
//! `my_skills_selection.rs` and `my_skills_reader.rs`; nothing here is
//! theirs. Every call is a thin pass-through to `ss_skills`/`ss_core` (the
//! same functions the GPUI page's `spawn_domain` jobs run, mod.rs:169-240,
//! updates.rs:71-176, repo_source.rs:16-126), with the same UI-thread
//! courtesy: git-touching jobs ride the blocking pool, cheap local reads run
//! inline (the deck facades' precedent).

use std::collections::BTreeSet;

use nsh_core::JsonValue;
use nsh_core::prelude::*;
use ss_core::types::skill::{Skill, SkillType, UpstreamChange};
use ss_skills::git_skill::GitSkillFacade;
use ss_skills::skill_update::SkillUpdateReport;

// The crate cannot depend on serde_json directly (its manifest is frozen
// for the parallel waves), so the JSON builders go through nsh-core's
// re-exports (same trick as the deck and marketplace facades).
use nsh_core::element::__private::JsonMap;

fn obj(pairs: Vec<(&str, JsonValue)>) -> JsonValue {
    let mut map = JsonMap::new();
    for (key, value) in pairs {
        map.insert(key.to_owned(), value);
    }
    JsonValue::Object(map)
}

fn str(value: &str) -> JsonValue {
    JsonValue::String(value.to_owned())
}

fn num(value: usize) -> JsonValue {
    JsonValue::from(value as u64)
}

/// The my-skills page seam (`MySkills` in TSX).
#[module]
pub struct MySkills;

/// Bridge-side favorites generation: orders overlapping saves from this
/// process exactly like the GPUI page's `favorites_epoch` (mod.rs:355-415 —
/// each click bumps, a stale write must not clobber a newer one).
static FAVORITES_GENERATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[module]
impl MySkills {
    /// The page's one refresh payload (mod.rs `refresh_from`): the installed
    /// skills (`installed_skill::list_installed_skills`), the agent profiles
    /// with their enabled flags (`agents::list_profiles` — the page filters
    /// the strip itself, matching toolbar.rs:71), and the repo favorites
    /// (`repo_favorites::load`, with the `favoritesLocked` gate for a file a
    /// newer SkillStar wrote). Read-only.
    pub async fn snapshot() -> Result<JsonValue, String> {
        let skills = ss_skills::installed_skill::list_installed_skills()
            .await
            .map_err(|err| format!("{err:#}"))?;
        blocking(move || snapshot_payload(&skills))
            .await
            .map_err(|join| format!("snapshot task failed: {join}"))
    }

    /// The GitHub session the channels-scope empty state gates on
    /// (`github_sign_in::shared_auth_facade().status()`, empty_state.rs
    /// `channels_gate`). The identity cache the GPUI side keeps between
    /// refreshes lives in the page signal; a rejected call leaves the page's
    /// gate at Unknown. Read-only (it may hit the GitHub API).
    pub async fn github_status() -> Result<JsonValue, String> {
        let facade = auth_facade();
        let status = facade.status().await.map_err(|err| format!("{err:#}"))?;
        Ok(github_status_json(&status))
    }

    /// Toggle one repository favorite and persist the set (mod.rs
    /// `toggle_repo_favorite`): the generation orders overlapping saves from
    /// this process, `repo_favorites::save` keeps the newest, and a
    /// NewerVersion file re-locks editing without touching the set. Resolves
    /// with the favorites this click leaves behind plus `locked`/`error` so
    /// the page can toast `toolbar.favoriteSourceLocked` /
    /// `toolbar.favoriteSourceFailed` exactly like the GPUI callback.
    pub async fn toggle_favorite_repo(source: String) -> Result<JsonValue, String> {
        blocking(move || toggle_favorite_repo_sync(&source))
            .await
            .map_err(|join| format!("toggle favorite task failed: {join}"))?
    }

    /// Compare installed skills with upstream (updates.rs `check_updates`):
    /// `GitSkillFacade::refresh_skill_updates`. Resolves with the number of
    /// skills the check marked updatable, so a manual check can toast
    /// `mySkills.checkUpdatesNone` / `checkUpdatesFound`. The silent
    /// background check and its 30-minute throttle are page state.
    pub async fn check_updates() -> Result<JsonValue, String> {
        let states = GitSkillFacade::from_file_store()
            .refresh_skill_updates()
            .await
            .map_err(|err| format!("{err:#}"))?;
        let pending = states
            .iter()
            .filter(|state| state.update_available)
            .count();
        Ok(obj(vec![("pending", num(pending))]))
    }

    /// Update skills by name (updates.rs `run_batch_update` /
    /// `update_skill`: both funnel into `update_skills`). Runs on the
    /// blocking pool like the GPUI `spawn_domain` job. Resolves with the
    /// report JSON (`updated/failed/skipped/identityChanged/…`) the page's
    /// one-toast summary reads; Err rejects with the domain chain.
    pub async fn update_skills(names: Vec<String>) -> Result<JsonValue, String> {
        blocking(move || {
            let report = GitSkillFacade::from_file_store().update_skills(&names);
            Ok(update_report_json(&report))
        })
        .await
        .map_err(|join| format!("update task failed: {join}"))?
    }

    /// Uninstall one skill by name (commands.rs `uninstall_skill`:
    /// `skill_install::uninstall_skill`). Blocking pool; Err rejects with
    /// the domain reason.
    pub async fn uninstall_skill(name: String) -> Result<(), String> {
        blocking(move || {
            ss_skills::skill_install::uninstall_skill(&name).map_err(|err| format!("{err:#}"))
        })
        .await
        .map_err(|join| format!("uninstall task failed: {join}"))?
    }

    /// Uninstall every skill installed from one repository
    /// (repo_source.rs `uninstall_repo_source` tail): one name per
    /// uninstall, failures collected as `name: err` lines joined with
    /// newlines. Blocking pool.
    pub async fn uninstall_repo_skills(names: Vec<String>) -> Result<(), String> {
        blocking(move || {
            let mut failed = Vec::new();
            for name in names {
                if let Err(err) = ss_skills::skill_install::uninstall_skill(&name) {
                    failed.push(format!("{name}: {err:#}"));
                }
            }
            if failed.is_empty() {
                Ok(())
            } else {
                Err(failed.join("\n"))
            }
        })
        .await
        .map_err(|join| format!("repo uninstall task failed: {join}"))?
    }

    /// Full-depth rescan of one hub repository, then overwrite-install
    /// everything the scan found (repo_source.rs `reinstall_repo_blocking`).
    /// Resolves with the installed count; Err carries
    /// `mySkills.reinstallRepoNoSkills` cases as the plain domain message.
    /// Blocking pool.
    pub async fn reinstall_repo_source(url: String) -> Result<JsonValue, String> {
        blocking(move || {
            let facade = GitSkillFacade::from_file_store();
            let scan = facade.scan_repo(&url, true).map_err(|err| format!("{err:#}"))?;
            if scan.skills.is_empty() {
                return Err("no skills found in repository".to_owned());
            }
            let targets: Vec<ss_skills::repo_scanner::SkillInstallTarget> = scan
                .skills
                .iter()
                .map(|skill| ss_skills::repo_scanner::SkillInstallTarget {
                    id: skill.id.clone(),
                    folder_path: skill.folder_path.clone(),
                    pinned: false,
                })
                .collect();
            let installed = facade
                .install_from_scan(&scan, &targets)
                .map_err(|err| format!("{err:#}"))?;
            Ok(num(installed.len()))
        })
        .await
        .map_err(|join| format!("reinstall task failed: {join}"))?
    }

    /// Reveal one skill's folder in the platform file manager (the local
    /// badge click, skill_card/mod.rs `SkillCardCommand::OpenFolder`:
    /// `content::open_skill_folder`).
    pub async fn open_skill_folder(name: String) -> Result<(), String> {
        blocking(move || ss_skills::content::open_skill_folder(&name).map_err(|err| format!("{err:#}")))
            .await
            .map_err(|join| format!("open folder task failed: {join}"))?
    }

    /// The stored per-skill description-translation choices
    /// (`ss_core::translation::load_config().description_choices`) the cards
    /// and the selection bar's translate action read (mod.rs:132).
    pub async fn description_choices() -> Result<JsonValue, String> {
        blocking(description_choices_sync)
            .await
            .map_err(|join| format!("description choices task failed: {join}"))?
    }

    /// Persist per-skill description-translation choices
    /// (`ss_core::translation::set_description_choices` — the drawer's
    /// language button and the selection bar's translate action). The
    /// Settings switch itself is never written by this seam.
    pub async fn set_description_choices(
        choices: JsonValue,
    ) -> Result<(), String> {
        let pairs = parse_description_choices(&choices)?;
        blocking(move || set_description_choices_sync(&pairs))
            .await
            .map_err(|join| format!("set description choices task failed: {join}"))?
    }
}

fn parse_description_choices(choices: &JsonValue) -> Result<Vec<(String, bool)>, String> {
    let entries = choices
        .as_object()
        .ok_or_else(|| "choices must be an object".to_owned())?;
    let mut pairs: Vec<(String, bool)> = Vec::new();
    for (skill, value) in entries {
        let Some(on) = value.as_bool() else {
            return Err(format!("choice for {skill} must be a boolean"));
        };
        pairs.push((skill.clone(), on));
    }
    Ok(pairs)
}

fn set_description_choices_sync(pairs: &[(String, bool)]) -> Result<(), String> {
    ss_core::translation::set_description_choices(
        pairs.iter().map(|(skill, on)| (skill.as_str(), *on)),
    )
    .map_err(|err| format!("{err:#}"))
}

fn snapshot_payload(skills: &[Skill]) -> JsonValue {
    let profiles = ss_skills::agents::list_profiles();
    let (favorites, locked) = match ss_skills::repo_favorites::load() {
        ss_skills::repo_favorites::LoadedFavorites::NewerVersion => (BTreeSet::new(), true),
        ss_skills::repo_favorites::LoadedFavorites::Ready(sources) => (sources, false),
    };
    let agents = profiles
        .iter()
        .map(|profile| {
            obj(vec![
                ("id", str(&profile.id)),
                ("displayName", str(&profile.display_name)),
                ("icon", str(&profile.icon)),
                ("enabled", JsonValue::Bool(profile.enabled)),
                (
                    "targetable",
                    JsonValue::Bool(profile.enabled && profile.has_global_skills()),
                ),
            ])
        })
        .collect::<Vec<_>>();
    let translation_config = ss_core::translation::load_config().ok();
    let choices_map = translation_config
        .as_ref()
        .map(|config| {
            let mut map = JsonMap::new();
            for (skill, on) in &config.description_choices {
                map.insert(skill.clone(), JsonValue::Bool(*on));
            }
            map
        })
        .unwrap_or_default();
    obj(vec![
        (
            "skills",
            JsonValue::Array(
                skills
                    .iter()
                    .map(|skill| skill_json_with_config(skill, translation_config.as_ref()))
                    .collect(),
            ),
        ),
        ("agents", JsonValue::Array(agents)),
        (
            "favorites",
            JsonValue::Array(favorites.iter().map(|s| str(s)).collect()),
        ),
        ("favoritesLocked", JsonValue::Bool(locked)),
        ("descriptionChoices", JsonValue::Object(choices_map)),
    ])
}

fn toggle_favorite_repo_sync(source: &str) -> Result<JsonValue, String> {
    if source.is_empty() {
        return Err("empty source".to_owned());
    }
    let (previous, locked) = match ss_skills::repo_favorites::load() {
        ss_skills::repo_favorites::LoadedFavorites::NewerVersion => (BTreeSet::new(), true),
        ss_skills::repo_favorites::LoadedFavorites::Ready(sources) => (sources, false),
    };
    if locked {
        return Ok(obj(vec![
            (
                "favorites",
                JsonValue::Array(previous.iter().map(|s| str(s)).collect()),
            ),
            ("locked", JsonValue::Bool(true)),
            ("stored", JsonValue::Bool(false)),
            ("error", JsonValue::Null),
        ]));
    }
    let mut next = previous.clone();
    if !next.remove(source) {
        next.insert(source.to_owned());
    }
    let generation =
        FAVORITES_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    let write = ss_skills::repo_favorites::save(generation, next.clone())
        .map_err(|err| err.to_string())?;
    let (kept, locked_now) = match write {
        ss_skills::repo_favorites::FavoriteWrite::Stored
        | ss_skills::repo_favorites::FavoriteWrite::Stale => (next, false),
        ss_skills::repo_favorites::FavoriteWrite::NewerVersion => (previous, true),
    };
    Ok(obj(vec![
        (
            "favorites",
            JsonValue::Array(kept.iter().map(|s| str(s)).collect()),
        ),
        ("locked", JsonValue::Bool(locked_now)),
        (
            "stored",
            JsonValue::Bool(matches!(
                write,
                ss_skills::repo_favorites::FavoriteWrite::Stored
            )),
        ),
        ("error", JsonValue::Null),
    ]))
}

fn description_choices_sync() -> Result<JsonValue, String> {
    let config = ss_core::translation::load_config().map_err(|err| format!("{err:#}"))?;
    let mut map = JsonMap::new();
    for (skill, on) in config.description_choices {
        map.insert(skill, JsonValue::Bool(on));
    }
    Ok(obj(vec![("choices", JsonValue::Object(map))]))
}

/// One `GitHubAuthFacade` for the process — the GPUI side caches identity
/// between refreshes behind `shared_auth_facade` (github_sign_in.rs:30-41);
/// the sign-in flow itself is that slice's, not this base's.
fn auth_facade(
) -> ss_skills::github_auth::GitHubAuthFacade<
    ss_skills::github_auth::ProductionGitHubGateway,
    ss_skills::github_auth::FileCredentialStore,
    ss_skills::github_auth::SystemClock,
> {
    ss_skills::github_auth::GitHubAuthFacade::new(
        ss_skills::github_auth::ProductionGitHubGateway::from_environment(),
        ss_skills::github_auth::FileCredentialStore::default(),
        ss_skills::github_auth::SystemClock,
    )
}

fn github_status_json(status: &ss_skills::github_auth::GitHubConnectionStatus) -> JsonValue {
    let (state, login) = match status {
        ss_skills::github_auth::GitHubConnectionStatus::SignedOut => {
            ("signed_out".to_owned(), None)
        }
        ss_skills::github_auth::GitHubConnectionStatus::Expired { identity } => (
            "expired".to_owned(),
            identity.as_ref().map(|item| item.login.clone()),
        ),
        ss_skills::github_auth::GitHubConnectionStatus::Connected { identity, .. } => {
            ("connected".to_owned(), Some(identity.login.clone()))
        }
    };
    obj(vec![
        ("state", str(&state)),
        (
            "login",
            match login {
                Some(name) => str(&name),
                None => JsonValue::Null,
            },
        ),
    ])
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

/// One skill as the page's TS record: every field `filters.rs` (name,
/// description, localizedDescription, source, author, agentLinks,
/// skillType, lastUpdated, updateAvailable, upstreamChange) and the compact
/// card body (gitUrl, stars, rank) read. `upstreamChange` keeps the GPUI
/// kinds the status pill branches on (render.rs:352-383).
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
    map.insert("name".to_owned(), str(&skill.name));
    map.insert("description".to_owned(), str(&skill.description));
    if let Some(text) = lookup_localized_description(skill, config) {
        map.insert("localizedDescription".to_owned(), str(&text));
    }
    map.insert(
        "skillType".to_owned(),
        str(match skill.skill_type {
            SkillType::Hub => "hub",
            SkillType::Local => "local",
        }),
    );
    map.insert("stars".to_owned(), JsonValue::from(skill.stars as u64));
    map.insert("installed".to_owned(), JsonValue::Bool(skill.installed));
    map.insert(
        "updateAvailable".to_owned(),
        JsonValue::Bool(skill.update_available),
    );
    map.insert(
        "upstreamChange".to_owned(),
        match &skill.upstream_change {
            None => JsonValue::Null,
            Some(UpstreamChange::Removed { .. }) => obj(vec![
                ("kind", str("removed")),
                // The pill only branches on the kind; the successors the
                // detail drawer explains stay that slice's to surface.
            ]),
            Some(UpstreamChange::IdentityChanged { upstream_name }) => obj(vec![
                ("kind", str("identity_changed")),
                ("upstreamName", str(upstream_name)),
            ]),
            Some(UpstreamChange::LocalChanges { baseline_missing }) => obj(vec![
                ("kind", str("local_changes")),
                ("baselineMissing", JsonValue::Bool(*baseline_missing)),
            ]),
        },
    );
    map.insert("lastUpdated".to_owned(), str(&skill.last_updated));
    map.insert("gitUrl".to_owned(), str(&skill.git_url));
    if let Some(author) = &skill.author {
        map.insert("author".to_owned(), str(author));
    }
    map.insert(
        "agentLinks".to_owned(),
        match &skill.agent_links {
            Some(links) => {
                JsonValue::Array(links.iter().map(|name| str(name)).collect())
            }
            None => JsonValue::Array(vec![]),
        },
    );
    if let Some(rank) = skill.rank {
        map.insert("rank".to_owned(), JsonValue::from(rank as u64));
    }
    if let Some(source) = &skill.source {
        map.insert("source".to_owned(), str(source));
    }
    if let Some(called) = &skill.called {
        map.insert("called".to_owned(), str(called));
    }
    JsonValue::Object(map)
}

/// One batch-update report as the page's TS record: the fields the
/// one-toast summary reads (updates.rs `report_lines` /
/// `report_needs_attention`), kebab-cased like the deck report.
fn update_report_json(report: &SkillUpdateReport) -> JsonValue {
    let updated = report
        .updated
        .iter()
        .map(|result| {
            obj(vec![
                ("name", str(&result.skill.name)),
                (
                    "agentLinkFailures",
                    JsonValue::Array(
                        result.agent_link_failures.iter().map(|s| str(s)).collect(),
                    ),
                ),
                (
                    "projectsUpdated",
                    JsonValue::Array(result.projects_updated.iter().map(|s| str(s)).collect()),
                ),
            ])
        })
        .collect::<Vec<_>>();
    let failed = report
        .failed
        .iter()
        .map(|failure| {
            obj(vec![
                ("name", str(&failure.name)),
                ("error", str(&failure.error)),
            ])
        })
        .collect::<Vec<_>>();
    let identity_changed = report
        .identity_changed
        .iter()
        .map(|change| {
            obj(vec![
                ("name", str(&change.name)),
                ("upstreamName", str(&change.upstream_name)),
            ])
        })
        .collect::<Vec<_>>();
    obj(vec![
        ("updated", JsonValue::Array(updated)),
        ("failed", JsonValue::Array(failed)),
        (
            "skipped",
            JsonValue::Array(report.skipped.iter().map(|s| str(s)).collect()),
        ),
        (
            "channelManaged",
            JsonValue::Array(
                report
                    .channel_managed
                    .iter()
                    .map(|entry| str(&entry.name))
                    .collect(),
            ),
        ),
        (
            "identityChanged",
            JsonValue::Array(identity_changed),
        ),
        (
            "notUpdatable",
            JsonValue::Array(report.not_updatable.iter().map(|s| str(s)).collect()),
        ),
        (
            "projectsUpdated",
            JsonValue::Array(report.projects_updated.iter().map(|s| str(s)).collect()),
        ),
        (
            "projectFailures",
            JsonValue::Array(report.project_failures.iter().map(|s| str(s)).collect()),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use ss_core::types::skill::{Skill, SkillType};

    /// The shared sandbox guard (facades/mod.rs TEST_ENV_LOCK): every facade
    /// suite that flips SKILLSTAR_DATA_DIR borrows it so the cargo threads
    /// serialize — module-local locks cannot (wave-1 lesson).
    struct IsolatedDataDir {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Option<std::ffi::OsString>,
        root: std::path::PathBuf,
    }

    impl IsolatedDataDir {
        fn new() -> Self {
            let lock = crate::facades::TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let root = std::env::temp_dir().join(format!(
                "skillstar-nsh-my-skills-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
            unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &root) };
            Self { _lock: lock, previous, root }
        }
    }

    impl Drop for IsolatedDataDir {
        fn drop(&mut self) {
            unsafe {
                match self.previous.take() {
                    Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                    None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
                }
            }
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn iso_probe_snapshot() {
        let _iso = IsolatedDataDir::new();
        let skills = spin(ss_skills::installed_skill::list_installed_skills()).expect("list");
        let snap = super::snapshot_payload(&skills);
        assert!(snap["skills"].as_array().map(Vec::len) == Some(0));
    }

    fn skill(name: &str) -> Skill {
        Skill::from_skills_sh(
            name.to_owned(),
            String::new(),
            0,
            "acme".to_owned(),
            String::new(),
        )
    }

    #[test]
    fn snapshot_shapes_skills_agents_and_favorites() {
        let _iso = IsolatedDataDir::new();
        let skills = spin(ss_skills::installed_skill::list_installed_skills()).expect("list");
        let snap = super::snapshot_payload(&skills);
        assert_eq!(
            snap["skills"].as_array().map(Vec::len),
            Some(0),
            "fresh isolated store has no skills"
        );
        // The built-in agent roster always exists (registry `specs`: built-in
        // first, then prefs customs) — assert its shape, not its emptiness.
        let agents = snap["agents"].as_array().expect("agents array");
        assert!(!agents.is_empty(), "the built-in roster always rides along");
        for agent in agents {
            assert!(agent["id"].as_str().is_some());
            assert!(agent["displayName"].as_str().is_some());
            assert!(agent["enabled"].as_bool().is_some());
            assert!(agent["targetable"].as_bool().is_some());
        }
        assert_eq!(snap["favorites"].as_array().map(Vec::len), Some(0));
        assert_eq!(
            snap["favoritesLocked"].as_bool(),
            Some(false),
            "a missing favorites file is editable, not locked"
        );
        assert!(snap["descriptionChoices"].is_object());
    }

    #[test]
    fn toggle_favorite_round_trips_and_rejects_empty_sources() {
        let _iso = IsolatedDataDir::new();
        let first = super::toggle_favorite_repo_sync("acme/skills")
            .expect("first favorite write");
        assert_eq!(first["stored"].as_bool(), Some(true));
        assert_eq!(first["locked"].as_bool(), Some(false));
        assert_eq!(
            first["favorites"].as_array().map(|list| list[0].as_str()),
            Some(Some("acme/skills"))
        );

        let second = super::toggle_favorite_repo_sync("acme/skills")
            .expect("second write toggles the star off");
        assert_eq!(
            second["favorites"].as_array().map(Vec::len),
            Some(0),
            "toggling the only favorite empties the set"
        );

        let err = super::toggle_favorite_repo_sync("")
            .expect_err("an empty source is not a favorite");
        assert!(!err.is_empty());
    }

    #[test]
    fn a_locked_favorites_file_does_not_flip_the_star() {
        let _iso = IsolatedDataDir::new();
        // A file this build does not understand: version above the current.
        let path = ss_core::infra::paths::repo_favorites_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, r#"{"version":99,"sources":["kept/repo"]}"#).unwrap();

        let result = super::toggle_favorite_repo_sync("acme/skills")
            .expect("a locked file is not an error");
        assert_eq!(result["locked"].as_bool(), Some(true));
        assert_eq!(result["stored"].as_bool(), Some(false));
        assert_eq!(
            result["favorites"].as_array().map(Vec::len),
            Some(0),
            "a locked store clears the projected set (mod.rs NewerVersion arm)"
        );
    }

    #[test]
    fn update_report_json_carries_every_summary_field() {
        let mut report = ss_skills::skill_update::SkillUpdateReport::default();
        report.updated.push(ss_skills::skill_update::UpdateResult {
            skill: skill("alpha"),
            siblings_cleared: Vec::new(),
            agent_link_failures: vec!["codex".to_owned()],
            projects_updated: Vec::new(),
            project_failures: Vec::new(),
        });
        report.failed.push(ss_skills::skill_update::SkillUpdateFailure {
            name: "beta".to_owned(),
            error: "network".to_owned(),
        });
        report.skipped.push("gone".to_owned());
        report.not_updatable.push("mine".to_owned());
        report.projects_updated.push("proj-a".to_owned());
        report
            .identity_changed
            .push(ss_skills::skill_update::SkillIdentityChange {
                name: "moved".to_owned(),
                upstream_name: "renamed".to_owned(),
            });

        let json = super::update_report_json(&report);
        assert_eq!(json["updated"][0]["name"].as_str(), Some("alpha"));
        assert_eq!(
            json["updated"][0]["agentLinkFailures"][0].as_str(),
            Some("codex")
        );
        assert_eq!(json["failed"][0]["error"].as_str(), Some("network"));
        assert_eq!(json["skipped"][0].as_str(), Some("gone"));
        assert_eq!(json["notUpdatable"][0].as_str(), Some("mine"));
        assert_eq!(
            json["identityChanged"][0]["upstreamName"].as_str(),
            Some("renamed")
        );
        assert_eq!(json["projectsUpdated"][0].as_str(), Some("proj-a"));
    }

    #[test]
    fn upstream_change_kinds_serialize_the_pill_vocabulary() {
        let mut removed = skill("removed");
        removed.upstream_change = Some(ss_core::types::skill::UpstreamChange::Removed {
            suggested_local_name: "removed.local".to_owned(),
            successor: None,
        });
        assert_eq!(
            super::skill_json(&removed)["upstreamChange"]["kind"].as_str(),
            Some("removed")
        );

        let mut local = skill("local-card");
        local.skill_type = SkillType::Local;
        local.called = Some("原始名".into());
        assert_eq!(super::skill_json(&local)["skillType"].as_str(), Some("local"));
        assert_eq!(super::skill_json(&local)["called"].as_str(), Some("原始名"));
        assert!(super::skill_json(&local)["upstreamChange"].is_null());

        let mut edited = skill("edited-card");
        edited.upstream_change = Some(ss_core::types::skill::UpstreamChange::LocalChanges {
            baseline_missing: true,
        });
        assert_eq!(
            super::skill_json(&edited)["upstreamChange"]["kind"].as_str(),
            Some("local_changes")
        );
        assert_eq!(
            super::skill_json(&edited)["upstreamChange"]["baselineMissing"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn description_choices_round_trip_the_translation_config() {
        let _iso = IsolatedDataDir::new();
        let payload = nsh_core::JsonValue::Object({
            let mut map = super::JsonMap::new();
            map.insert("demo".to_owned(), nsh_core::JsonValue::Bool(true));
            map
        });
        let pairs = super::parse_description_choices(&payload).expect("valid payload");
        super::set_description_choices_sync(&pairs).expect("choices persist");
        let read = super::description_choices_sync().expect("choices read back");
        assert_eq!(
            read["choices"]["demo"].as_bool(),
            Some(true),
            "the stored choice survives the round trip"
        );
    }

    #[test]
    fn github_status_distinguishes_expired_from_signed_out() {
        use ss_skills::github_auth::{GitHubConnectionStatus, GitHubIdentity};
        let expired = GitHubConnectionStatus::Expired {
            identity: Some(GitHubIdentity {
                id: 1,
                login: "octocat".to_owned(),
                avatar_url: None,
            }),
        };
        let json = super::github_status_json(&expired);
        assert_eq!(json["state"].as_str(), Some("expired"));
        assert_eq!(json["login"].as_str(), Some("octocat"));

        let expired_anon = GitHubConnectionStatus::Expired { identity: None };
        let anon_json = super::github_status_json(&expired_anon);
        assert_eq!(anon_json["state"].as_str(), Some("expired"));
        assert!(anon_json["login"].is_null());

        let out_json = super::github_status_json(&GitHubConnectionStatus::SignedOut);
        assert_eq!(out_json["state"].as_str(), Some("signed_out"));
        assert!(out_json["login"].is_null());
    }

    #[test]
    fn skill_json_backfills_cached_translation_when_enabled() {
        let _iso = IsolatedDataDir::new();
        let mut cfg = ss_core::translation::TranslationConfig {
            translate_descriptions: true,
            target_lang: "zh-CN".to_owned(),
            ..Default::default()
        };
        ss_core::translation::save_config(&cfg).expect("save config");
        ss_core::translation::remember("Review pull requests", "zh-CN", "审查拉取请求");

        let mut s = skill("pr-review");
        s.description = "Review pull requests".to_owned();
        let json = super::skill_json_with_config(&s, Some(&cfg));
        assert_eq!(json["localizedDescription"].as_str(), Some("审查拉取请求"));

        cfg.translate_descriptions = false;
        let off_json = super::skill_json_with_config(&s, Some(&cfg));
        assert!(off_json.get("localizedDescription").is_none());

        cfg.description_choices.insert("pr-review".to_owned(), true);
        let override_json = super::skill_json_with_config(&s, Some(&cfg));
        assert_eq!(
            override_json["localizedDescription"].as_str(),
            Some("审查拉取请求")
        );
    }

    /// The #[module] macro turns `async fn` into a task-start call in JS;
    /// from Rust tests the same bodies run as plain futures. Drive them on
    /// a minimal poll loop (these calls complete on first poll).
    fn spin<T>(future: impl std::future::Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => unreachable!("domain calls finish on first poll"),
        }
    }
}
