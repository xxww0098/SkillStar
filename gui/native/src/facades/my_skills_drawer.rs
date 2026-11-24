//! My-skills detail-drawer facade — the `my_skills/detail_drawer.rs` slice's
//! seam to the ss-skills domain (`commands.rs` `dispatch_card_command`
//! ToggleAgent + `set_skill_agent_links` → `toggle_listed_agents`).
//!
//! The drawer's other actions ride the page base's facades (`MySkills.
//! updateSkills` / `uninstallSkill` / `openSkillFolder` /
//! `setDescriptionChoices`) and the shared translation seam
//! (`MarketplaceTranslation`: descriptionsEnabled / needsTranslation /
//! lookup / fillDescriptions); this slot owns only what the drawer alone
//! runs — the per-agent deploy switch and the master switch's one-shot
//! batch. Thin pass-throughs with the GPUI jobs' blocking-pool courtesy and
//! the same zero-wait update-transaction lock (commands.rs:210-218,
//! 300-308): a running skill update fails fast with its message instead of
//! freezing the column.

use std::time::Duration;

use nsh_core::JsonValue;
use nsh_core::prelude::*;

use ss_skills::deployment::{
    self, SKIP_CANONICAL_ROOT_AGENT, ToggleSkillOutcome,
};
use ss_skills::{library, skill_update::try_acquire_update_transaction_lock, update_state};
// The manifest stays frozen for the parallel waves, so the JSON builders go
// through nsh-core's re-exports (the deck and my-skills facades' trick).
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

/// The detail-drawer seam (`MySkillsDrawer` in TSX).
#[module]
pub struct MySkillsDrawer;

#[module]
impl MySkillsDrawer {
    /// One agent row's switch (commands.rs `toggle_skill_agent` →
    /// `dispatch_card_command` ToggleAgent): the zero-wait update lock, then
    /// `deployment::toggle_skill_for_agent`. Resolves `{"kind":"applied"}` or
    /// `{"kind":"skipped","code","path","reason"}` — the caller maps a skip
    /// back to a user-facing message exactly like `toggle_skip_message`
    /// (canonical-root agents are served without a link; an unmanaged
    /// directory is left in place). Err carries the domain chain (unknown
    /// agent, unknown skill, a busy update transaction). Blocking pool.
    pub async fn toggle_agent(
        skill: String,
        agent_id: String,
        enable: bool,
    ) -> Result<JsonValue, String> {
        blocking(move || toggle_one(&skill, &agent_id, enable))
            .await
            .map_err(|join| format!("toggle task failed: {join}"))?
    }

    /// The master switch's batch (commands.rs `set_skill_agent_links` →
    /// `toggle_listed_agents`): ONE update transaction for the whole list,
    /// then one `toggle_skill_for_agent` per agent. A canonical-root agent is
    /// not a failure (it already sees every installed skill); name collisions
    /// count as skipped and hard errors as failed. Resolves
    /// `{"total","failed","skipped","canonical"}` — the caller assembles the
    /// partial-failure notice and the all-canonical explanation. Err means
    /// the transaction lock was held (nothing was touched). Blocking pool.
    pub async fn set_agent_links(
        skill: String,
        agent_ids: Vec<String>,
        enable: bool,
    ) -> Result<JsonValue, String> {
        blocking(move || toggle_listed(&skill, &agent_ids, enable))
            .await
            .map_err(|join| format!("agent-link batch task failed: {join}"))?
    }

    /// The drawer's own detail read: the upstream-change descriptor of one
    /// installed skill, including the `baselineMissing` bit the page snapshot
    /// drops (`MySkills.skill_json` keeps only the kind — the drawer's
    /// local-edits hint branches on it, detail_facts.rs `upstream_note`).
    /// `null` when the skill is not installed. Read-only listing.
    pub async fn upstream_note(skill: String) -> JsonValue {
        let skills = match ss_skills::installed_skill::list_installed_skills().await {
            Ok(skills) => skills,
            Err(_) => return JsonValue::Null,
        };
        let Some(found) = skills.iter().find(|entry| entry.name == skill) else {
            return JsonValue::Null;
        };
        match &found.upstream_change {
            None => JsonValue::Null,
            Some(ss_core::types::skill::UpstreamChange::Removed { .. }) => {
                obj(vec![("kind", str("removed"))])
            }
            Some(ss_core::types::skill::UpstreamChange::IdentityChanged { upstream_name }) => {
                obj(vec![
                    ("kind", str("identity_changed")),
                    ("upstreamName", str(upstream_name)),
                ])
            }
            Some(ss_core::types::skill::UpstreamChange::LocalChanges {
                baseline_missing,
            }) => obj(vec![
                ("kind", str("local_changes")),
                ("baselineMissing", JsonValue::Bool(*baseline_missing)),
            ]),
        }
    }

    /// Inspect `called`, discovered `besideSkills` siblings, `modifiedCopies`,
    /// and `unmanagedEntries` requiring conflict recovery. Blocking pool.
    pub async fn inspect_drawer_extras(skill: String) -> Result<JsonValue, String> {
        blocking(move || inspect_drawer_extras_job(&skill))
            .await
            .map_err(|join| format!("inspect-drawer-extras task failed: {join}"))?
    }

    /// Adopt edits from an agent copy back into `data/skills/installed/<skill>`.
    /// Blocking pool under the zero-wait update transaction lock.
    pub async fn take_agent_copy_edits(skill: String, agent_id: String) -> Result<(), String> {
        blocking(move || take_agent_copy_edits_job(&skill, &agent_id))
            .await
            .map_err(|join| format!("take-agent-copy-edits task failed: {join}"))?
    }

    /// Backup-archive and overwrite an agent copy with the canonical installed copy.
    /// Blocking pool under the zero-wait update transaction lock.
    pub async fn overwrite_agent_copy_with_backup(
        skill: String,
        agent_id: String,
    ) -> Result<JsonValue, String> {
        blocking(move || overwrite_agent_copy_with_backup_job(&skill, &agent_id))
            .await
            .map_err(|join| format!("overwrite-agent-copy task failed: {join}"))?
    }

    /// Backup-archive an unmanaged agent directory (when real dir) and switch
    /// it to SkillStar's managed symlink/copy deployment. Blocking pool under
    /// the zero-wait update transaction lock.
    pub async fn take_over_unmanaged_agent_entry(
        skill: String,
        agent_id: String,
    ) -> Result<JsonValue, String> {
        blocking(move || take_over_unmanaged_agent_entry_job(&skill, &agent_id))
            .await
            .map_err(|join| format!("take-over-unmanaged-entry task failed: {join}"))?
    }

    /// Absorb an unmanaged real skill directory from `agent_id` into
    /// `data/skills/installed/<skill>` (archiving the previous canonical copy)
    /// and switch `agent_id` to a managed deployment. Blocking pool.
    pub async fn adopt_unmanaged_agent_entry(
        skill: String,
        agent_id: String,
    ) -> Result<(), String> {
        blocking(move || adopt_unmanaged_agent_entry_job(&skill, &agent_id))
            .await
            .map_err(|join| format!("adopt-unmanaged-entry task failed: {join}"))?
    }
}

fn inspect_drawer_extras_job(skill: &str) -> Result<JsonValue, String> {
    let called = match library::called_name(skill) {
        Some(value) => str(&value),
        None => JsonValue::Null,
    };
    let beside_skills: Vec<JsonValue> = update_state::beside_skills(skill)
        .into_iter()
        .map(|sib| {
            obj(vec![
                ("name", str(&sib.name)),
                ("source", str(&sib.source)),
                ("folderPath", str(&sib.folder_path)),
            ])
        })
        .collect();
    let modified_copies: Vec<JsonValue> = deployment::inspect_modified_agent_copies(Some(skill))
        .into_iter()
        .map(|info| {
            obj(vec![
                ("skillName", str(&info.skill_name)),
                ("agentId", str(&info.agent_id)),
                ("agentDisplayName", str(&info.agent_name)),
                ("targetDir", str(&info.path)),
            ])
        })
        .collect();
    let unmanaged_entries: Vec<JsonValue> =
        deployment::inspect_unmanaged_agent_entries(Some(skill))
            .into_iter()
            .map(|info| {
                obj(vec![
                    ("skillName", str(&info.skill_name)),
                    ("agentId", str(&info.agent_id)),
                    ("agentDisplayName", str(&info.agent_name)),
                    ("targetDir", str(&info.path)),
                    ("isSymlink", JsonValue::Bool(info.is_symlink)),
                    ("canAdoptToHub", JsonValue::Bool(info.can_adopt_to_hub)),
                ])
            })
            .collect();
    Ok(obj(vec![
        ("called", called),
        ("besideSkills", JsonValue::Array(beside_skills)),
        ("modifiedCopies", JsonValue::Array(modified_copies)),
        ("unmanagedEntries", JsonValue::Array(unmanaged_entries)),
    ]))
}

fn take_agent_copy_edits_job(skill: &str, agent_id: &str) -> Result<(), String> {
    let _guard = try_acquire_update_transaction_lock(Duration::ZERO)
        .map_err(|err| format!("{err:#}"))?;
    deployment::take_agent_copy_edits(skill, agent_id).map_err(|err| format!("{err:#}"))
}

fn overwrite_agent_copy_with_backup_job(skill: &str, agent_id: &str) -> Result<JsonValue, String> {
    let _guard = try_acquire_update_transaction_lock(Duration::ZERO)
        .map_err(|err| format!("{err:#}"))?;
    let entry = deployment::overwrite_agent_copy_with_backup(skill, agent_id)
        .map_err(|err| format!("{err:#}"))?;
    Ok(obj(vec![
        ("backupId", str(&entry.id)),
        ("path", str(&entry.path.display().to_string())),
    ]))
}

fn take_over_unmanaged_agent_entry_job(skill: &str, agent_id: &str) -> Result<JsonValue, String> {
    let _guard = try_acquire_update_transaction_lock(Duration::ZERO)
        .map_err(|err| format!("{err:#}"))?;
    let entry = deployment::take_over_unmanaged_agent_entry(skill, agent_id)
        .map_err(|err| format!("{err:#}"))?;
    Ok(match entry {
        Some(backup) => obj(vec![
            ("backupId", str(&backup.id)),
            ("path", str(&backup.path.display().to_string())),
        ]),
        None => obj(vec![
            ("backupId", JsonValue::Null),
            ("path", JsonValue::Null),
        ]),
    })
}

fn adopt_unmanaged_agent_entry_job(skill: &str, agent_id: &str) -> Result<(), String> {
    let _guard = try_acquire_update_transaction_lock(Duration::ZERO)
        .map_err(|err| format!("{err:#}"))?;
    deployment::adopt_unmanaged_agent_entry(skill, agent_id).map_err(|err| format!("{err:#}"))
}

/// `toggle_agent`'s synchronous core (run on the blocking pool by the module
/// method; called directly by the tests, which have no runtime to park on —
/// `blocking` is `tokio::task::spawn_blocking`).
fn toggle_one(skill: &str, agent_id: &str, enable: bool) -> Result<JsonValue, String> {
    let _guard = try_acquire_update_transaction_lock(Duration::ZERO)
        .map_err(|err| format!("{err:#}"))?;
    match ss_skills::deployment::toggle_skill_for_agent(skill, agent_id, enable) {
        Ok(ToggleSkillOutcome::Applied) => Ok(obj(vec![("kind", str("applied"))])),
        Ok(ToggleSkillOutcome::Skipped { code, path, reason }) => Ok(obj(vec![
            ("kind", str("skipped")),
            ("code", str(&code)),
            ("path", str(&path)),
            ("reason", str(&reason)),
        ])),
        Err(err) => Err(format!("{err:#}")),
    }
}

/// `set_agent_links`'s synchronous core — commands.rs `toggle_listed_agents`
/// verbatim (one lock for the whole list; failures/skips counted, never
/// fatal; the first error is logged by the GPUI side, which this crate's
/// frozen manifest cannot link a tracing subscriber into).
fn toggle_listed(skill: &str, agent_ids: &[String], enable: bool) -> Result<JsonValue, String> {
    let _guard = try_acquire_update_transaction_lock(Duration::ZERO)
        .map_err(|err| format!("{err:#}"))?;
    let total = agent_ids.len();
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut canonical = 0usize;
    for agent_id in agent_ids {
        match ss_skills::deployment::toggle_skill_for_agent(skill, agent_id, enable) {
            Ok(ToggleSkillOutcome::Applied) => {}
            Ok(ToggleSkillOutcome::Skipped { code, .. }) if code == SKIP_CANONICAL_ROOT_AGENT => {
                canonical += 1;
            }
            Ok(ToggleSkillOutcome::Skipped { .. }) => skipped += 1,
            Err(_) => failed += 1,
        }
    }
    Ok(obj(vec![
        ("total", num(total)),
        ("failed", num(failed)),
        ("skipped", num(skipped)),
        ("canonical", num(canonical)),
    ]))
}

#[cfg(test)]
mod tests {
    use super::MySkillsDrawer;

    /// The shared sandbox guard (facades/mod.rs TEST_ENV_LOCK): every facade
    /// suite that flips SKILLSTAR_DATA_DIR borrows it so the cargo threads
    /// serialize — module-local locks cannot (wave-1 lesson).
    struct IsolatedDataDir {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
        root: std::path::PathBuf,
    }

    impl IsolatedDataDir {
        fn new() -> Self {
            let lock = crate::facades::TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let root = std::env::temp_dir().join(format!(
                "skillstar-nsh-my-skills-drawer-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let previous = [
                ("HOME", "home"),
                ("USERPROFILE", "home"),
                ("SKILLSTAR_TOOL_SYNC_HOME", "home"),
                ("SKILLSTAR_DATA_DIR", "data"),
            ]
            .into_iter()
            .map(|(key, sub)| {
                let saved = std::env::var_os(key);
                let dir = root.join(sub);
                std::fs::create_dir_all(&dir).unwrap();
                unsafe { std::env::set_var(key, dir) };
                (key, saved)
            })
            .collect();
            ss_skills::deployment::invalidate_profile_cache();
            Self { _lock: lock, previous, root }
        }
    }

    impl Drop for IsolatedDataDir {
        fn drop(&mut self) {
            unsafe {
                for (key, saved) in self.previous.drain(..) {
                    match saved {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
            ss_skills::deployment::invalidate_profile_cache();
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn an_empty_master_batch_reports_zeros() {
        let _iso = IsolatedDataDir::new();
        let report = super::toggle_listed("demo", &[], true).expect("empty batch");
        assert_eq!(report["total"].as_u64(), Some(0));
        assert_eq!(report["failed"].as_u64(), Some(0));
        assert_eq!(report["skipped"].as_u64(), Some(0));
        assert_eq!(report["canonical"].as_u64(), Some(0));
    }

    #[test]
    fn toggling_an_uninstalled_skill_rejects_with_the_domain_reason() {
        let _iso = IsolatedDataDir::new();
        let err = super::toggle_one("definitely-not-installed", "cursor", true)
            .expect_err("an uninstalled skill has nothing to link");
        assert!(!err.is_empty());
    }

    #[test]
    fn the_master_batch_counts_an_unknown_agent_as_failed() {
        let _iso = IsolatedDataDir::new();
        let report = super::toggle_listed(
            "definitely-not-installed",
            &["no-such-agent".to_owned()],
            true,
        )
        .expect("the batch itself resolves");
        assert_eq!(report["total"].as_u64(), Some(1));
        assert_eq!(report["failed"].as_u64(), Some(1));
        assert_eq!(report["skipped"].as_u64(), Some(0));
    }

    #[test]
    fn an_invalid_skill_name_is_rejected_by_validation() {
        let _iso = IsolatedDataDir::new();
        let err = super::toggle_one("../escape", "cursor", true)
            .expect_err("path-like names are not skills");
        assert!(!err.is_empty());
    }

    #[test]
    fn an_unknown_skill_has_no_upstream_note() {
        let _iso = IsolatedDataDir::new();
        let mut future =
            std::pin::pin!(MySkillsDrawer::upstream_note("definitely-not-installed".to_owned()));
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        let value = match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => unreachable!("a listing over an empty dir settles"),
        };
        assert!(value.is_null());
    }

    #[test]
    fn drawer_extras_and_modified_copy_recovery_roundtrip() {
        let iso = IsolatedDataDir::new();
        let hub_dir = ss_core::infra::paths::hub_skills_dir().join("demo");
        std::fs::create_dir_all(&hub_dir).unwrap();
        std::fs::write(
            hub_dir.join("SKILL.md"),
            "---\nname: demo\ndescription: v1\n---\n\n# v1\n",
        )
        .unwrap();
        ss_skills::library::record_remote_install(
            "demo",
            &ss_skills::skill_lock::SkillLockEntry {
                source: "acme/skills".into(),
                source_type: ss_skills::skill_lock::SourceType::Github,
                source_url: "https://github.com/acme/skills.git".into(),
                skill_path: Some("skills/demo".into()),
                git_ref: None,
                skill_folder_hash: Some("h1".into()),
                installed_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:00Z".into(),
                extra: std::collections::BTreeMap::new(),
            },
            Some("原始名".into()),
            None,
            false,
        )
        .unwrap();
        ss_skills::update_state::record_beside_skills(
            "demo",
            vec![ss_skills::update_state::BesideSkill {
                name: "helper".into(),
                source: "acme/skills".into(),
                folder_path: "skills/helper".into(),
            }],
        );

        let agent_dir = iso.root.join("custom-agent-skills");
        ss_skills::agents::add_custom_profile(ss_skills::agents::CustomProfileDef {
            id: "custom_copy_drawer".into(),
            display_name: "Custom Copy".into(),
            global_skills_dir: agent_dir.to_string_lossy().into_owned(),
            project_skills_rel: ".custom/skills".into(),
            icon_data_uri: None,
        })
        .unwrap();
        ss_skills::agents::toggle_profile("custom_copy_drawer").unwrap();
        ss_skills::agents::set_agent_deploy_mode_override(
            "custom_copy_drawer",
            Some(ss_skills::agents::AgentDeployMode::Copy),
        )
        .unwrap();
        ss_skills::deployment::batch_deploy_skills_to_agents(
            &["demo".into()],
            &["custom_copy_drawer".into()],
            ss_skills::projects::ProjectDeployMode::Copy,
        )
        .unwrap();

        // Simulate external edit in the copy directory.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            agent_dir.join("demo/SKILL.md"),
            "---\nname: demo\ndescription: edited-in-agent\n---\n\n# edited\n",
        )
        .unwrap();

        let extras = super::inspect_drawer_extras_job("demo").unwrap();
        assert_eq!(extras["called"].as_str(), Some("原始名"));
        assert_eq!(extras["besideSkills"][0]["name"].as_str(), Some("helper"));
        assert_eq!(
            extras["modifiedCopies"][0]["agentId"].as_str(),
            Some("custom_copy_drawer")
        );

        super::take_agent_copy_edits_job("demo", "custom_copy_drawer").unwrap();
        let content = std::fs::read_to_string(hub_dir.join("SKILL.md")).unwrap();
        assert!(content.contains("edited-in-agent"));
        assert!(
            super::inspect_drawer_extras_job("demo").unwrap()["modifiedCopies"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        // Now modify copy again and test backup + overwrite recovery.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            agent_dir.join("demo/SKILL.md"),
            "---\nname: demo\ndescription: discard-me\n---\n",
        )
        .unwrap();
        let backup =
            super::overwrite_agent_copy_with_backup_job("demo", "custom_copy_drawer").unwrap();
        assert!(backup["backupId"].as_str().is_some());
        let restored = std::fs::read_to_string(agent_dir.join("demo/SKILL.md")).unwrap();
        assert!(restored.contains("edited-in-agent"));
    }

    #[test]
    fn unmanaged_antigravity_directory_takeover_and_adopt_roundtrip() {
        let iso = IsolatedDataDir::new();
        let hub_dir = ss_core::infra::paths::hub_skills_dir().join("ego-browser");
        std::fs::create_dir_all(&hub_dir).unwrap();
        std::fs::write(
            hub_dir.join("SKILL.md"),
            "---\nname: ego-browser\ndescription: hub version\n---\n\n# hub\n",
        )
        .unwrap();

        ss_skills::agents::toggle_profile("antigravity").unwrap();
        let antigravity_dir = iso
            .root
            .join("home/.gemini/antigravity/skills/ego-browser");
        std::fs::create_dir_all(&antigravity_dir).unwrap();
        std::fs::write(
            antigravity_dir.join("SKILL.md"),
            "---\nname: ego-browser\ndescription: external app copy\n---\n\n# external\n",
        )
        .unwrap();

        let extras = super::inspect_drawer_extras_job("ego-browser").unwrap();
        assert_eq!(
            extras["unmanagedEntries"][0]["agentId"].as_str(),
            Some("antigravity")
        );
        assert_eq!(
            extras["unmanagedEntries"][0]["canAdoptToHub"].as_bool(),
            Some(true)
        );

        // Adopt unmanaged directory into Hub first.
        super::adopt_unmanaged_agent_entry_job("ego-browser", "antigravity").unwrap();
        assert!(
            std::fs::read_to_string(hub_dir.join("SKILL.md"))
                .unwrap()
                .contains("external app copy")
        );
        assert!(
            super::inspect_drawer_extras_job("ego-browser").unwrap()["unmanagedEntries"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        // Simulate external installer overwriting Antigravity's folder again, then take over.
        ss_skills::deployment::unlink_skill_from_agent("ego-browser", "antigravity").unwrap();
        std::fs::create_dir_all(&antigravity_dir).unwrap();
        std::fs::write(
            antigravity_dir.join("SKILL.md"),
            "---\nname: ego-browser\ndescription: stale external\n---\n",
        )
        .unwrap();
        let takeover =
            super::take_over_unmanaged_agent_entry_job("ego-browser", "antigravity").unwrap();
        assert!(takeover["backupId"].as_str().is_some());
        assert!(
            ss_skills::installed_skill::agent_links_for("ego-browser")
                .iter()
                .any(|name| name == "Antigravity")
        );
    }
}
