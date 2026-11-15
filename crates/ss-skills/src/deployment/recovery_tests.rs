use super::*;
use crate::agents::{
    self, AgentDeployMode, agent_disallows_junction, agent_reads_shared_agents_skills,
    effective_deploy_mode, get_default_deploy_mode, set_agent_deploy_mode_override,
    set_default_deploy_mode, shared_agents_skills_dir,
};
use crate::backup::{BackupReason, list_backups};
use crate::deployment::ownership::{self, DEPLOY_MARKER, Ownership};
use crate::test_sandbox::Sandbox;
use ss_core::infra::{fs_ops, paths};
use std::fs;
use std::time::{Duration, SystemTime};

fn create_canonical_skill(name: &str, body: &str) -> std::path::PathBuf {
    let dir = paths::hub_skills_dir().join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: demo\n---\n{body}\n"),
    )
    .unwrap();
    dir
}

fn ensure_enabled(agent_id: &str, want: bool) {
    let current = cached_profiles()
        .into_iter()
        .find(|p| p.id == agent_id)
        .map(|p| p.enabled)
        .unwrap_or(false);
    if current != want {
        agents::toggle_profile(agent_id).unwrap();
        invalidate_profile_cache();
    }
}

fn backdate_tree_files(dir: &Path, age_secs: u64) {
    let past = SystemTime::now()
        .checked_sub(Duration::from_secs(age_secs))
        .unwrap();
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy() == DEPLOY_MARKER {
            continue;
        }
        if path.is_dir() {
            backdate_tree_files(&path, age_secs);
        } else if path.is_file() {
            let file = fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(past).unwrap();
        }
    }
}

#[test]
fn deploy_mode_preferences_and_capabilities_roundtrip() -> Result<()> {
    let _sandbox = Sandbox::new();
    assert_eq!(get_default_deploy_mode(), AgentDeployMode::Symlink);
    assert_eq!(effective_deploy_mode("claude"), AgentDeployMode::Symlink);
    assert!(agent_disallows_junction("antigravity"));
    assert!(agent_disallows_junction("crush"));
    assert!(!agent_disallows_junction("claude"));
    assert!(agent_reads_shared_agents_skills("cursor"));
    assert!(agent_reads_shared_agents_skills("gemini-cli"));
    assert!(agent_reads_shared_agents_skills("droid"));
    assert!(agent_reads_shared_agents_skills("opencode"));
    assert!(!agent_reads_shared_agents_skills("claude"));

    set_default_deploy_mode(AgentDeployMode::Copy)?;
    assert_eq!(get_default_deploy_mode(), AgentDeployMode::Copy);
    assert_eq!(effective_deploy_mode("claude"), AgentDeployMode::Copy);

    set_agent_deploy_mode_override("claude", Some(AgentDeployMode::Symlink))?;
    assert_eq!(effective_deploy_mode("claude"), AgentDeployMode::Symlink);
    assert_eq!(effective_deploy_mode("codex"), AgentDeployMode::Copy);

    set_agent_deploy_mode_override("claude", None)?;
    assert_eq!(effective_deploy_mode("claude"), AgentDeployMode::Copy);
    Ok(())
}

#[test]
fn deploy_marker_fast_path_detects_clean_vs_modified_copies_and_ignores_runtime_dirs() -> Result<()>
{
    let _sandbox = Sandbox::new();
    ensure_enabled("claude", true);
    set_default_deploy_mode(AgentDeployMode::Copy)?;

    let canon = create_canonical_skill("fast-copy", "v1 body");
    fs::create_dir_all(canon.join("__pycache__"))?;
    fs::write(canon.join("__pycache__/cached.pyc"), b"ignored-in-stage")?;

    assert_eq!(
        toggle_skill_for_agent("fast-copy", "claude", true)?,
        ToggleSkillOutcome::Applied
    );
    let claude = cached_profiles()
        .into_iter()
        .find(|p| p.id == "claude")
        .unwrap();
    let deployed = claude.global_skills_dir.join("fast-copy");
    assert!(!fs_ops::is_link(&deployed));
    assert!(
        !deployed.join("__pycache__").exists(),
        "copy staging must strip runtime artifact dirs"
    );
    assert_eq!(
        ownership::owned_deployment(&deployed, "fast-copy"),
        Ownership::Copy
    );

    // Put a runtime dir in the deployed target and confirm ownership stays Copy.
    fs::create_dir_all(deployed.join(".venv"))?;
    fs::write(deployed.join(".venv/marker"), b"ok")?;
    assert_eq!(
        ownership::owned_deployment(&deployed, "fast-copy"),
        Ownership::Copy
    );

    // Backdate files by >= 5s and refresh marker, then tamper contentHash inside
    // .skillstar-deploy.json. Because stats match and mtime >= 2s old, O(stat)
    // fast-path must trust the marker as clean Ownership::Copy without hashing.
    backdate_tree_files(&deployed, 5);
    ownership::mark_copy_for_test(&deployed, "fast-copy");
    let marker_path = deployed.join(DEPLOY_MARKER);
    let mut marker_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path)?)?;
    marker_json["contentHash"] = serde_json::json!("tampered-for-fast-path-test");
    fs::write(&marker_path, serde_json::to_vec(&marker_json)?)?;
    assert_eq!(
        ownership::owned_deployment(&deployed, "fast-copy"),
        Ownership::Copy
    );

    // Edit SKILL.md -> mtime/bytes/hash diverge -> SkillStarModifiedCopy.
    fs::write(deployed.join("SKILL.md"), "user customized instructions")?;
    assert_eq!(
        ownership::owned_deployment(&deployed, "fast-copy"),
        Ownership::SkillStarModifiedCopy
    );
    Ok(())
}

#[test]
fn modified_copy_is_protected_and_recoverable_with_take_edits_or_backup() -> Result<()> {
    let _sandbox = Sandbox::new();
    ensure_enabled("claude", true);
    set_default_deploy_mode(AgentDeployMode::Copy)?;

    let canon = create_canonical_skill("protected-skill", "canonical v1");
    crate::install_baseline::record(&["protected-skill".to_string()]);
    assert_eq!(
        toggle_skill_for_agent("protected-skill", "claude", true)?,
        ToggleSkillOutcome::Applied
    );

    let claude_target = cached_profiles()
        .into_iter()
        .find(|p| p.id == "claude")
        .unwrap()
        .global_skills_dir
        .join("protected-skill");
    fs::write(
        claude_target.join("SKILL.md"),
        "---\nname: protected-skill\ndescription: demo\n---\nuser agent edit\n",
    )?;

    // Unlink refuses to destroy locally modified copy edits.
    let outcome = toggle_skill_for_agent("protected-skill", "claude", false)?;
    match outcome {
        ToggleSkillOutcome::Skipped { code, .. } => {
            assert_eq!(code, SKIP_MODIFIED_AGENT_COPY);
        }
        other => panic!("expected SKIP_MODIFIED_AGENT_COPY, got {other:?}"),
    }
    assert!(claude_target.exists());

    // Inspection reports the modified agent copy.
    let modified = inspect_modified_agent_copies(Some("protected-skill"));
    assert_eq!(modified.len(), 1);
    assert_eq!(modified[0].agent_id, "claude");

    // Mode change reconciliation skips modified copies and counts them.
    set_default_deploy_mode(AgentDeployMode::Symlink)?;
    let mode_report = reconcile_deployments_for_mode_change(Some("claude"))?;
    assert_eq!(mode_report.skipped_modified.len(), 1);
    assert_eq!(mode_report.updated, 0);

    // Overwrite-with-backup archives the modified copy and re-deploys clean canonical.
    let entry = overwrite_agent_copy_with_backup("protected-skill", "claude")?;
    assert_eq!(entry.reason, BackupReason::CopyConflict);
    assert!(!list_backups()?.is_empty());
    assert!(fs_ops::is_link(&claude_target));
    assert!(fs::read_to_string(canon.join("SKILL.md"))?.contains("canonical v1"));
    assert!(inspect_modified_agent_copies(Some("protected-skill")).is_empty());

    // Now switch claude to Copy, edit again, and absorb via take_agent_copy_edits.
    set_agent_deploy_mode_override("claude", Some(AgentDeployMode::Copy))?;
    let report = reconcile_deployments_for_mode_change(Some("claude"))?;
    assert_eq!(report.updated, 1);
    assert!(!fs_ops::is_link(&claude_target));

    fs::write(
        claude_target.join("SKILL.md"),
        "---\nname: protected-skill\ndescription: demo\n---\nabsorbed agent edit\n",
    )?;
    take_agent_copy_edits("protected-skill", "claude")?;
    assert!(fs::read_to_string(canon.join("SKILL.md"))?.contains("absorbed agent edit"));
    assert!(inspect_modified_agent_copies(Some("protected-skill")).is_empty());
    assert_eq!(
        ownership::owned_deployment(&claude_target, "protected-skill"),
        Ownership::Copy
    );
    Ok(())
}

#[test]
fn shared_agents_dir_refcounting_and_cursor_anti_double_loading() -> Result<()> {
    let _sandbox = Sandbox::new();
    // Codex and Warp share ~/.agents/skills; Cursor reads ~/.agents/skills in addition
    // to ~/.cursor/skills. Antigravity uses ~/.gemini/antigravity/skills (non-shared).
    ensure_enabled("codex", true);
    ensure_enabled("warp", true);
    ensure_enabled("cursor", true);
    ensure_enabled("claude", false);

    create_canonical_skill("shared-skill", "shared body");
    let shared_target = shared_agents_skills_dir().join("shared-skill");
    let cursor_target = cached_profiles()
        .into_iter()
        .find(|p| p.id == "cursor")
        .unwrap()
        .global_skills_dir
        .join("shared-skill");

    assert_eq!(
        toggle_skill_for_agent("shared-skill", "codex", true)?,
        ToggleSkillOutcome::Applied
    );
    assert!(shared_target.exists() || fs_ops::is_link(&shared_target));

    assert_eq!(
        toggle_skill_for_agent("shared-skill", "warp", true)?,
        ToggleSkillOutcome::Applied
    );

    // Toggling cursor on while shared ~/.agents/skills satisfies it must suppress ~/.cursor/skills.
    assert_eq!(
        toggle_skill_for_agent("shared-skill", "cursor", true)?,
        ToggleSkillOutcome::Applied
    );
    assert!(
        !cursor_target.exists() && !fs_ops::is_link(&cursor_target),
        "cursor secondary entry should be suppressed while ~/.agents/skills/shared-skill is active"
    );

    // Unlink warp: codex still desires shared-skill -> disk stays & cursor stays suppressed.
    assert_eq!(
        toggle_skill_for_agent("shared-skill", "warp", false)?,
        ToggleSkillOutcome::Applied
    );
    assert!(shared_target.exists() || fs_ops::is_link(&shared_target));
    assert!(!cursor_target.exists());

    // Unlink codex: no primary ~/.agents/skills holder remains -> shared entry is removed
    // AND cursor's own ~/.cursor/skills/shared-skill is materialized automatically.
    assert_eq!(
        toggle_skill_for_agent("shared-skill", "codex", false)?,
        ToggleSkillOutcome::Applied
    );
    assert!(
        !shared_target.exists() && !fs_ops::is_link(&shared_target),
        "last primary shared holder unlinked -> shared entry must be removed"
    );
    assert!(
        cursor_target.exists() || fs_ops::is_link(&cursor_target),
        "cursor secondary entry must materialize once shared holder disappears"
    );
    Ok(())
}

#[test]
fn disallow_junction_agents_accept_symlinks_on_unix_and_switch_cleanly_to_copy() -> Result<()> {
    let _sandbox = Sandbox::new();
    ensure_enabled("antigravity", true);
    create_canonical_skill("strict-link", "body");

    assert_eq!(
        toggle_skill_for_agent("strict-link", "antigravity", true)?,
        ToggleSkillOutcome::Applied
    );
    let ag = cached_profiles()
        .into_iter()
        .find(|p| p.id == "antigravity")
        .unwrap();
    let target = ag.global_skills_dir.join("strict-link");
    assert!(target.exists());

    set_agent_deploy_mode_override("antigravity", Some(AgentDeployMode::Copy))?;
    let report = reconcile_deployments_for_mode_change(Some("antigravity"))?;
    assert_eq!(report.updated, 1);
    assert!(!fs_ops::is_link(&target));
    assert_eq!(
        ownership::owned_deployment(&target, "strict-link"),
        Ownership::Copy
    );
    Ok(())
}
