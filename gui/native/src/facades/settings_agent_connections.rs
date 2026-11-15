//! Settings agent-connections facade — the agent_connections +
//! managed_skills slice owns this file (`SettingsAgentConnections` in TSX;
//! GPUI sources `crates/ss-gpui/src/settings/agent_connections.rs` and
//! `settings/managed_skills/`, TSX section `settings-agents`, slots
//! `src/pages/settings/sections/agentConnections.tsx` and
//! `sections/managedSkills.tsx`). Pre-registered empty by the wave-7
//! wiring pass so the slice never touches a shared registry — it only adds
//! methods inside this file. Do not edit `settings.rs` (the base seam) or
//! the `lib.rs` / `facades/mod.rs` registration.
//!
//! Domain seams this slice wires (the crate already depends on ss-skills):
//! - `ss_skills::agents::{list_profiles, toggle_profile, add_custom_profile}`
//!   — the enriched registry the settings page reads at construction and
//!   the enable switch (managed_skills/mod.rs `toggle_agent_enabled` parity;
//!   the `AgentsChanged` shell event is a TS-side channel publish).
//! - `ss_skills::deployment::{list_linked_skills, unlink_skill_from_agent,
//!   unlink_all_skills_from_agent}` — the linked-cards panel and its
//!   one-click clear (one physical Global skills directory = one journal,
//!   shared by every profile that resolves to it).
//! - `ss_skills::workflows::agent_managed_skills::{get_agent_managed_skills_state,
//!   toggle_agent_managed_skills}` — per-directory pause/restore. The
//!   wire shapes are the domain's own `Serialize` output (snake_case
//!   `active_skill_names` / `suspended_skill_names`, report with
//!   `action`/`state`/`succeeded`/`skipped`/`failed`).
//!
//! Blocking discipline: the unlink/toggle bodies walk the physical skills
//! directory the way the GPUI page's `spawn_blocking` jobs do, so they ride
//! the `blocking` pool (`nsh_core::prelude::blocking`). The cheap registry
//! reads run inline. Unit tests drive the plain `_job` bodies — never a
//! blocking-pool future (the facade-test precedent).
//!
//! Test discipline (wave-1 lesson): any test touching the
//! `SKILLSTAR_DATA_DIR` sandbox borrows the shared `TEST_ENV_LOCK` from
//! `facades/mod.rs` for the whole sandbox lifetime — never a module-private
//! lock, never `--test-threads=1`.

use nsh_core::prelude::*;
use nsh_core::JsonValue;
use nsh_core::element::__private::JsonMap;

use ss_skills::agents;
use ss_skills::deployment;
use ss_skills::workflows::agent_managed_skills;

/// The agent-connections + managed-skills settings seam
/// (`SettingsAgentConnections` in TSX).
#[module]
pub struct SettingsAgentConnections;

// The `#[module]` methods wrap these in `blocking`; the suite drives the
// plain bodies instead (a blocking-pool future needs a runtime the
// unit-test host does not start).
fn unlink_skill_job(skill: &str, agent_id: &str) -> Result<(), String> {
    deployment::unlink_skill_from_agent(skill, agent_id).map_err(|err| format!("{err:#}"))
}

fn unlink_all_job(agent_id: &str) -> Result<u32, String> {
    deployment::unlink_all_skills_from_agent(agent_id).map_err(|err| format!("{err:#}"))
}

fn managed_state_job(agent_id: &str) -> Result<JsonValue, String> {
    let state = agent_managed_skills::get_agent_managed_skills_state(agent_id)
        .map_err(|err| format!("{err:#}"))?;
    Ok(serde_json::to_value(&state).unwrap_or(JsonValue::Null))
}

fn toggle_managed_job(agent_id: &str) -> Result<JsonValue, String> {
    let report = agent_managed_skills::toggle_agent_managed_skills(agent_id)
        .map_err(|err| format!("{err:#}"))?;
    Ok(serde_json::to_value(&report).unwrap_or(JsonValue::Null))
}

fn deploy_mode_label(mode: agents::AgentDeployMode) -> &'static str {
    match mode {
        agents::AgentDeployMode::Symlink => "symlink",
        agents::AgentDeployMode::Copy => "copy",
    }
}

fn parse_deploy_mode(raw: &str) -> Result<agents::AgentDeployMode, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "symlink" | "link" => Ok(agents::AgentDeployMode::Symlink),
        "copy" => Ok(agents::AgentDeployMode::Copy),
        other => Err(format!("Unsupported deploy mode: {other}")),
    }
}

fn deploy_modes_wire() -> JsonValue {
    let default_mode = agents::get_default_deploy_mode();
    let mut overrides = JsonMap::new();
    for (id, mode) in agents::get_agent_deploy_mode_overrides() {
        overrides.insert(id, JsonValue::String(deploy_mode_label(mode).to_owned()));
    }
    let disallow_junction: Vec<JsonValue> = agents::list_profiles()
        .into_iter()
        .filter(|profile| agents::agent_disallows_junction(&profile.id))
        .map(|profile| JsonValue::String(profile.id))
        .collect();
    let mut map = JsonMap::new();
    map.insert(
        "defaultMode".to_owned(),
        JsonValue::String(deploy_mode_label(default_mode).to_owned()),
    );
    map.insert("overrides".to_owned(), JsonValue::Object(overrides));
    map.insert(
        "disallowJunction".to_owned(),
        JsonValue::Array(disallow_junction),
    );
    JsonValue::Object(map)
}

fn set_default_deploy_mode_job(mode: &str) -> Result<JsonValue, String> {
    let parsed = parse_deploy_mode(mode)?;
    agents::set_default_deploy_mode(parsed).map_err(|err| format!("{err:#}"))?;
    let report = deployment::reconcile_deployments_for_mode_change(None)
        .map_err(|err| format!("{err:#}"))?;
    let mut map = JsonMap::new();
    map.insert(
        "defaultMode".to_owned(),
        JsonValue::String(deploy_mode_label(parsed).to_owned()),
    );
    map.insert(
        "reconciled".to_owned(),
        serde_json::to_value(&report).unwrap_or(JsonValue::Null),
    );
    Ok(JsonValue::Object(map))
}

fn set_agent_deploy_mode_job(agent_id: &str, mode: Option<&str>) -> Result<JsonValue, String> {
    let parsed = match mode.map(str::trim) {
        None | Some("") | Some("inherit") => None,
        Some(raw) => Some(parse_deploy_mode(raw)?),
    };
    agents::set_agent_deploy_mode_override(agent_id, parsed).map_err(|err| format!("{err:#}"))?;
    let report = deployment::reconcile_deployments_for_mode_change(Some(agent_id))
        .map_err(|err| format!("{err:#}"))?;
    let mut map = JsonMap::new();
    map.insert(
        "overrides".to_owned(),
        deploy_modes_wire()
            .get("overrides")
            .cloned()
            .unwrap_or(JsonValue::Object(JsonMap::new())),
    );
    map.insert(
        "reconciled".to_owned(),
        serde_json::to_value(&report).unwrap_or(JsonValue::Null),
    );
    Ok(JsonValue::Object(map))
}

#[module]
impl SettingsAgentConnections {
    /// The enriched agent registry (`agents::list_profiles` parity — the
    /// call the GPUI page makes at construction). Each profile serializes
    /// with the FROZEN 8-field `AgentProfile` shape (`id`, `display_name`,
    /// `icon`, `global_skills_dir`, `project_skills_rel`, `installed`,
    /// `enabled`, `synced_count`). Read-only.
    pub fn list_profiles() -> JsonValue {
        serde_json::to_value(agents::list_profiles()).unwrap_or(JsonValue::Array(Vec::new()))
    }

    /// Flip one agent's enabled flag (`agents::toggle_profile` parity,
    /// managed_skills/mod.rs `toggle_agent_enabled`) and resolve the new
    /// value. Persisting to the profile store is the domain's job; the
    /// TSX side publishes `agentsChanged` after a success.
    pub fn toggle_profile(id: String) -> Result<bool, String> {
        agents::toggle_profile(&id).map_err(|err| format!("{err:#}"))
    }

    /// Linked card names for one agent (`deployment::list_linked_skills`
    /// parity, managed_skills/mod.rs `toggle_agent_expand` fallback branch).
    pub fn list_linked_skills(agent_id: String) -> Result<JsonValue, String> {
        let names = deployment::list_linked_skills(&agent_id).map_err(|err| format!("{err:#}"))?;
        Ok(JsonValue::Array(
            names.into_iter().map(JsonValue::String).collect(),
        ))
    }

    /// Unlink one card (`deployment::unlink_skill_from_agent` parity,
    /// managed_skills/mod.rs `unlink_linked_skill`). Blocking pool.
    pub async fn unlink_skill_from_agent(skill: String, agent_id: String) -> Result<(), String> {
        blocking(move || unlink_skill_job(&skill, &agent_id))
            .await
            .map_err(|join| format!("unlink task failed: {join}"))?
    }

    /// One-click clear for an agent's linked cards
    /// (`deployment::unlink_all_skills_from_agent` parity,
    /// managed_skills/mod.rs `unlink_all_linked_skills`): sweeps the
    /// physical Global skills directory, so every profile sharing it loses
    /// the same links; unmanaged real directories stay (domain rule).
    /// Resolves the removal count. Blocking pool.
    pub async fn unlink_all_skills_from_agent(agent_id: String) -> Result<u32, String> {
        blocking(move || unlink_all_job(&agent_id))
            .await
            .map_err(|join| format!("unlink-all task failed: {join}"))?
    }

    /// Managed pause/restore state for one agent's directory
    /// (`agent_managed_skills::get_agent_managed_skills_state` parity).
    /// Resolves `{active_skill_names[], suspended_skill_names[]}`; the
    /// journal is keyed by the physical directory, so the same wire comes
    /// back for every profile that resolves to it. Blocking pool.
    pub async fn get_managed_state(agent_id: String) -> Result<JsonValue, String> {
        blocking(move || managed_state_job(&agent_id))
            .await
            .map_err(|join| format!("managed-state task failed: {join}"))?
    }

    /// Pause or restore the managed set
    /// (`agent_managed_skills::toggle_agent_managed_skills` parity,
    /// managed_skills/mod.rs `toggle_managed_skills`). Resolves the
    /// `AgentManagedSkillsToggleReport` wire: `action` (`paused` /
    /// `restored`), `state` (the fresh managed state), `succeeded[]`,
    /// `skipped[]` (`skill_name` / `code` / `path` / `reason`) and
    /// `failed[]` (`skill_name` / `error`). Blocking pool.
    pub async fn toggle_managed_skills(agent_id: String) -> Result<JsonValue, String> {
        blocking(move || toggle_managed_job(&agent_id))
            .await
            .map_err(|join| format!("toggle-managed task failed: {join}"))?
    }

    /// Global default deploy mode, per-agent overrides, and agents that disallow
    /// Windows junctions / links (`antigravity`, `crush`). Read-only.
    pub fn get_deploy_modes() -> JsonValue {
        deploy_modes_wire()
    }

    /// Persist global default deploy mode (`symlink` / `copy`) in `profiles.toml`
    /// and reconcile existing managed agent deployments cleanly on blocking pool.
    pub async fn set_default_deploy_mode(mode: String) -> Result<JsonValue, String> {
        blocking(move || set_default_deploy_mode_job(&mode))
            .await
            .map_err(|join| format!("set-default-deploy-mode task failed: {join}"))?
    }

    /// Persist per-agent deploy mode override (`None` / `""` / `"inherit"` clears)
    /// and reconcile existing deployments for that agent on blocking pool.
    pub async fn set_agent_deploy_mode(
        agent_id: String,
        mode: Option<String>,
    ) -> Result<JsonValue, String> {
        blocking(move || set_agent_deploy_mode_job(&agent_id, mode.as_deref()))
            .await
            .map_err(|join| format!("set-agent-deploy-mode task failed: {join}"))?
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SettingsAgentConnections, managed_state_job, set_agent_deploy_mode_job,
        set_default_deploy_mode_job, toggle_managed_job, unlink_all_job,
    };
    use nsh_core::JsonValue;

    use ss_skills::agents;
    use ss_skills::deployment;
    use ss_skills::workflows::agent_managed_skills;

    /// The GPUI suites' IsolatedDataDir pattern (facades/mod.rs), widened to
    /// the ss-skills workflow-test sandbox shape: the builtin profiles
    /// resolve their Global skills directories from `$HOME`, so the sandbox
    /// re-roots HOME/USERPROFILE/SKILLSTAR_DATA_DIR/SKILLSTAR_HUB_DIR for
    /// the duration, borrowing the shared TEST_ENV_LOCK so every facade
    /// suite that flips these variables serializes against each other
    /// (racing threads abort the binary). Never touches the real $HOME.
    struct AgentSandbox {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
        root: std::path::PathBuf,
    }

    impl AgentSandbox {
        fn new() -> Self {
            let lock = crate::facades::TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let root = std::env::temp_dir().join(format!(
                "skillstar-nsh-settings-agents-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let previous = [
                ("HOME", "home"),
                ("USERPROFILE", "home"),
                ("SKILLSTAR_DATA_DIR", "data"),
                ("SKILLSTAR_HUB_DIR", "hub"),
            ]
            .into_iter()
            .map(|(key, suffix)| {
                let saved = std::env::var_os(key);
                let path = root.join(suffix);
                std::fs::create_dir_all(&path).unwrap();
                // SAFETY: the shared env lock outlives this sandbox.
                unsafe { std::env::set_var(key, path) };
                (key, saved)
            })
            .collect();
            deployment::invalidate_profile_cache();
            Self { _lock: lock, previous, root }
        }

        /// One custom agent whose Global skills directory is `agent/skills`
        /// under the sandbox — two ids sharing one physical directory when
        /// called twice (the workflow_tests.rs fixture shape).
        fn profile(&self, id: &str, enabled: bool) -> std::path::PathBuf {
            let target = self.root.join("agent/skills");
            agents::add_custom_profile(agents::CustomProfileDef {
                id: id.into(),
                display_name: id.into(),
                global_skills_dir: target.to_string_lossy().into_owned(),
                project_skills_rel: ".agent/skills".into(),
                icon_data_uri: None,
            })
            .unwrap();
            if enabled {
                assert!(agents::toggle_profile(id).unwrap());
            }
            deployment::invalidate_profile_cache();
            target
        }

        fn skill(&self, name: &str) {
            let target =
                ss_core::infra::paths::hub_skills_dir().join(name);
            std::fs::create_dir_all(&target).unwrap();
            std::fs::write(
                target.join("SKILL.md"),
                "---\nname: demo\ndescription: test\n---\n# test\n",
            )
            .unwrap();
        }
    }

    impl Drop for AgentSandbox {
        fn drop(&mut self) {
            for (key, previous) in self.previous.drain(..) {
                // SAFETY: still holding the same shared lock.
                unsafe {
                    match previous {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
            deployment::invalidate_profile_cache();
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn str_vec(value: &JsonValue, key: &str) -> Vec<String> {
        value
            .get(key)
            .and_then(|item| item.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `list_profiles` parity: the frozen 8-field wire the TSX DTO mirrors,
    /// including a custom profile registered under the sandbox, with the
    /// enabled flag reflecting the persisted toggle.
    #[test]
    fn profiles_wire_the_frozen_eight_field_shape() {
        let sandbox = AgentSandbox::new();
        sandbox.profile("custom_wire", true);
        let profiles = SettingsAgentConnections::list_profiles();
        let rows = profiles.as_array().expect("profiles serialize as array");
        let custom = rows
            .iter()
            .find(|row| row.get("id").and_then(|v| v.as_str()) == Some("custom_wire"))
            .expect("the custom profile is listed");
        for field in [
            "id",
            "display_name",
            "icon",
            "global_skills_dir",
            "project_skills_rel",
        ] {
            assert!(
                custom.get(field).and_then(|v| v.as_str()).is_some(),
                "{field} serializes as a string"
            );
        }
        assert_eq!(custom.get("enabled").and_then(|v| v.as_bool()), Some(true));
        assert!(custom.get("synced_count").and_then(|v| v.as_u64()).is_some());
        assert!(custom.get("installed").and_then(|v| v.as_bool()).is_some());
        // The custom directory resolves inside the sandbox, never real $HOME.
        let dir = custom.get("global_skills_dir").and_then(|v| v.as_str()).unwrap();
        assert!(dir.starts_with(sandbox.root.to_str().unwrap()));
    }

    /// `toggle_profile` parity: the wire resolves the persisted flag and a
    /// re-list observes it (what the TSX `applyToggle` reconciles).
    #[test]
    fn toggle_profile_round_trips_the_enabled_flag() {
        let sandbox = AgentSandbox::new();
        sandbox.profile("custom_flip", false);
        let flipped = SettingsAgentConnections::toggle_profile("custom_flip".into());
        assert_eq!(flipped.unwrap(), true);
        let profiles = SettingsAgentConnections::list_profiles();
        let row = profiles
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row.get("id").and_then(|v| v.as_str()) == Some("custom_flip"))
            .unwrap();
        assert_eq!(row.get("enabled").and_then(|v| v.as_bool()), Some(true));
        // An unknown id rejects instead of flipping anything.
        assert!(SettingsAgentConnections::toggle_profile("no-such-agent".into()).is_err());
    }

    /// The shared-directory journal, driven through the facade bodies: two
    /// profiles resolving to one physical directory see one pause set, the
    /// restore from the *other* profile brings the exact names back, and a
    /// skill that was never linked is not synthesized (workflow_tests.rs
    /// `pause_and_restore_share_a_directory_journal…` re-read through the
    /// seam).
    #[test]
    fn shared_directory_journal_pauses_and_restores_through_the_seam() {
        let sandbox = AgentSandbox::new();
        let target = sandbox.profile("custom_a", true);
        sandbox.profile("custom_b", true);
        sandbox.skill("writer");
        ss_skills::deployment::batch_deploy_skills_to_agents(
            &["writer".into()],
            &["custom_a".into()],
            ss_skills::projects::ProjectDeployMode::Symlink,
        )
        .unwrap();

        let state = managed_state_job("custom_b").unwrap();
        assert_eq!(str_vec(&state, "active_skill_names"), ["writer"]);

        let report = toggle_managed_job("custom_a").unwrap();
        assert_eq!(report.get("action").and_then(|v| v.as_str()), Some("paused"));
        assert_eq!(str_vec(&report.get("state").unwrap(), "suspended_skill_names"), ["writer"]);
        assert!(!target.join("writer").exists());
        // The sibling profile reads the same journal through its own id.
        let sibling = managed_state_job("custom_b").unwrap();
        assert_eq!(str_vec(&sibling, "suspended_skill_names"), ["writer"]);

        sandbox.skill("never-linked");
        let restored = toggle_managed_job("custom_b").unwrap();
        assert_eq!(
            restored.get("action").and_then(|v| v.as_str()),
            Some("restored")
        );
        assert_eq!(str_vec(&restored.get("state").unwrap(), "active_skill_names"), ["writer"]);
        assert!(target.join("writer/SKILL.md").is_file());
        assert!(
            !target.join("never-linked").exists(),
            "restore never synthesizes a Hub-wide set"
        );
    }

    /// `unlink_all` parity through the seam: the clear reports the removal
    /// count, and an empty directory clears to zero without error (the
    /// `unlinkAllFromAgentNone` copy the TSX renders).
    #[test]
    fn unlink_all_reports_the_removed_count() {
        let sandbox = AgentSandbox::new();
        let _target = sandbox.profile("custom_clear", true);
        sandbox.skill("writer");
        ss_skills::deployment::batch_deploy_skills_to_agents(
            &["writer".into()],
            &["custom_clear".into()],
            ss_skills::projects::ProjectDeployMode::Symlink,
        )
        .unwrap();
        assert_eq!(unlink_all_job("custom_clear").unwrap(), 1);
        let names = deployment::list_linked_skills("custom_clear").unwrap();
        assert!(names.is_empty());
        assert_eq!(unlink_all_job("custom_clear").unwrap(), 0);
    }

    /// `list_linked_skills` rejects for an unknown profile instead of
    /// resolving an empty list (find_profile parity).
    #[test]
    fn linked_skills_reject_an_unknown_profile() {
        let _sandbox = AgentSandbox::new();
        assert!(managed_state_job("no-such-agent").is_err());
        let names = SettingsAgentConnections::list_linked_skills("no-such-agent".into());
        assert!(names.is_err());
    }

    /// The skip wire the TSX notice builder consumes: a paused directory
    /// whose slot an unmanaged real folder occupies reports the
    /// `unmanaged_real_directory` code path. Exercised on the domain
    /// report type (the serialize seam is the same one the facade rides).
    #[test]
    fn toggle_report_serializes_skip_details() {
        let sandbox = AgentSandbox::new();
        let target = sandbox.profile("custom_skip", true);
        sandbox.skill("writer");
        ss_skills::deployment::batch_deploy_skills_to_agents(
            &["writer".into()],
            &["custom_skip".into()],
            ss_skills::projects::ProjectDeployMode::Symlink,
        )
        .unwrap();
        // Pause, then occupy the freed slot with an unmanaged real folder.
        toggle_managed_job("custom_skip").unwrap();
        std::fs::create_dir_all(target.join("writer")).unwrap();

        let report = agent_managed_skills::toggle_agent_managed_skills("custom_skip").unwrap();
        let wire = serde_json::to_value(&report).unwrap();
        assert_eq!(wire.get("action").and_then(|v| v.as_str()), Some("restored"));
        let skipped = wire
            .get("skipped")
            .and_then(|v| v.as_array())
            .expect("the occupied slot is skipped, not overwritten");
        assert_eq!(skipped.len(), 1);
        assert_eq!(
            skipped[0].get("code").and_then(|v| v.as_str()),
            Some("unmanaged_real_directory")
        );
        assert!(target.join("writer").is_dir(), "the user folder survives");
    }

    #[test]
    fn deploy_modes_roundtrip_default_and_per_agent_override() {
        let sandbox = AgentSandbox::new();
        let target = sandbox.profile("custom_mode", true);
        sandbox.skill("writer");
        ss_skills::deployment::batch_deploy_skills_to_agents(
            &["writer".into()],
            &["custom_mode".into()],
            ss_skills::projects::ProjectDeployMode::Symlink,
        )
        .unwrap();

        let initial = SettingsAgentConnections::get_deploy_modes();
        assert_eq!(initial["defaultMode"].as_str(), Some("symlink"));
        assert!(
            initial["disallowJunction"]
                .as_array()
                .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("antigravity")))
        );

        let updated = set_default_deploy_mode_job("copy").unwrap();
        assert_eq!(updated["defaultMode"].as_str(), Some("copy"));
        assert_eq!(updated["reconciled"]["updated"].as_u64(), Some(1));
        assert!(!target.join("writer").is_symlink());
        assert!(target.join("writer/SKILL.md").is_file());

        let over = set_agent_deploy_mode_job("custom_mode", Some("symlink")).unwrap();
        assert_eq!(over["overrides"]["custom_mode"].as_str(), Some("symlink"));
        assert_eq!(over["reconciled"]["updated"].as_u64(), Some(1));

        let cleared = set_agent_deploy_mode_job("custom_mode", Some("inherit")).unwrap();
        assert!(cleared["overrides"].get("custom_mode").is_none());
    }
}
