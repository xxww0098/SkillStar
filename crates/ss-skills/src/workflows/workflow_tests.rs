use super::{agent_managed_skills::*, global_deploy::*};
use crate::{agents, deployment};
use ss_core::infra::paths;
use std::{ffi::OsString, path::PathBuf};

struct Sandbox {
    previous: Vec<(&'static str, Option<OsString>)>,
    root: tempfile::TempDir,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl Sandbox {
    fn new() -> Self {
        let lock = crate::lock_test_env();
        let root = tempfile::tempdir().unwrap();
        let previous = [
            ("HOME", "home"),
            ("USERPROFILE", "home"),
            ("SKILLSTAR_DATA_DIR", "data"),
            ("SKILLSTAR_HUB_DIR", "hub"),
            ("SKILLSTAR_TOOL_SYNC_HOME", "home"),
            ("XDG_STATE_HOME", "state"),
        ]
        .into_iter()
        .map(|(key, suffix)| {
            let saved = std::env::var_os(key);
            let path = root.path().join(suffix);
            std::fs::create_dir_all(&path).unwrap();
            // SAFETY: the crate-wide environment lock outlives this sandbox.
            unsafe { std::env::set_var(key, path) };
            (key, saved)
        })
        .collect();
        deployment::invalidate_profile_cache();
        Self {
            previous,
            root,
            _lock: lock,
        }
    }

    fn profile(&self, id: &str, enabled: bool) -> PathBuf {
        // Both profiles intentionally share one physical destination.
        let target = self.root.path().join("agent/skills");
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
        let target = paths::hub_skills_dir().join(name);
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(
            target.join("SKILL.md"),
            "---
name: demo
description: test
---
# test
",
        )
        .unwrap();
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        for (key, previous) in self.previous.drain(..) {
            // SAFETY: still holding the same environment lock.
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
        deployment::invalidate_profile_cache();
    }
}

#[test]
fn deployment_with_no_targets_does_not_provision_storage() {
    let _sandbox = Sandbox::new();
    assert!(
        deploy_to_selected_global_agents(&["missing".into()], &[])
            .unwrap()
            .is_empty()
    );
    assert!(!paths::hub_skills_dir().exists());
}

#[test]
fn deployment_reports_failure_without_removing_installed_content() {
    let sandbox = Sandbox::new();
    sandbox.skill("writer");
    let error = deploy_to_selected_global_agents(&["writer".into()], &["unknown-agent".into()])
        .unwrap_err();
    assert!(error.contains("Installed to the hub but deployment is incomplete"));
    assert!(paths::hub_skills_dir().join("writer/SKILL.md").is_file());
}

#[test]
fn pause_and_restore_share_a_directory_journal_and_never_expand_to_the_hub() {
    let sandbox = Sandbox::new();
    let target = sandbox.profile("custom_a", true);
    sandbox.profile("custom_b", true);
    sandbox.skill("writer");
    deploy_to_selected_global_agents(&["writer".into()], &["custom_a".into()]).unwrap();
    let paused = toggle_agent_managed_skills("custom_a").unwrap();
    assert_eq!(paused.action, AgentManagedSkillsAction::Paused);
    assert_eq!(paused.state.suspended_skill_names, ["writer"]);
    assert!(!target.join("writer").exists());
    assert_eq!(
        get_agent_managed_skills_state("custom_b")
            .unwrap()
            .suspended_skill_names,
        ["writer"]
    );
    sandbox.skill("new-skill");
    let restored = toggle_agent_managed_skills("custom_b").unwrap();
    assert_eq!(restored.action, AgentManagedSkillsAction::Restored);
    assert_eq!(restored.state.active_skill_names, ["writer"]);
    assert!(restored.state.suspended_skill_names.is_empty());
    assert!(target.join("writer/SKILL.md").is_file());
    assert!(!target.join("new-skill").exists());
}

#[test]
fn disabled_profile_cannot_pause_or_restore_skills() {
    let sandbox = Sandbox::new();
    sandbox.profile("custom_disabled", false);
    let error = toggle_agent_managed_skills("custom_disabled").unwrap_err();
    assert!(error.to_string().contains("must be enabled"));
}
