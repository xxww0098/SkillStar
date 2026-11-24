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

#[test]
fn unmanaged_foreign_directory_takeover_and_adopt_with_mirror_cleanup() -> Result<()> {
    let sandbox = Sandbox::new();
    ensure_enabled("antigravity", true);
    let canon = create_canonical_skill("ego-browser", "hub v2");

    let ag_target = sandbox
        .home()
        .join(".gemini/antigravity/skills/ego-browser");
    let ide_mirror_target = sandbox
        .home()
        .join(".gemini/antigravity-ide/skills/ego-browser");
    let gemini_cli_reserved = sandbox.home().join(".gemini/skills/ego-browser");

    for dir in [&ag_target, &ide_mirror_target, &gemini_cli_reserved] {
        fs::create_dir_all(dir)?;
        fs::write(
            dir.join("SKILL.md"),
            "---\nname: ego-browser\ndescription: external installer\n---\nexternal v1\n",
        )?;
    }

    // Normal toggle stays fail-closed (D-094/D-096).
    let skipped = toggle_skill_for_agent("ego-browser", "antigravity", true)?;
    match skipped {
        ToggleSkillOutcome::Skipped { code, .. } => {
            assert_eq!(code, SKIP_UNMANAGED_REAL_DIRECTORY);
        }
        other => panic!("expected SKIP_UNMANAGED_REAL_DIRECTORY, got {other:?}"),
    }

    let inspected = inspect_unmanaged_agent_entries(Some("ego-browser"));
    assert_eq!(inspected.len(), 1);
    assert_eq!(inspected[0].agent_id, "antigravity");
    assert!(!inspected[0].is_symlink);
    assert!(inspected[0].can_adopt_to_hub);

    // Explicit takeover archives the unmanaged dir, deploys Antigravity + IDE mirror,
    // and leaves gemini-cli's reserved ~/.gemini/skills untouched.
    let backup = take_over_unmanaged_agent_entry("ego-browser", "antigravity")?
        .expect("real directory takeover creates a backup");
    assert_eq!(backup.reason, BackupReason::CopyConflict);
    assert!(ownership::owned_deployment(&ag_target, "ego-browser").is_owned());
    assert!(ownership::owned_deployment(&ide_mirror_target, "ego-browser").is_owned());
    assert_eq!(
        ownership::owned_deployment(&gemini_cli_reserved, "ego-browser"),
        Ownership::Foreign,
        "reserved gemini-cli directory must not be touched by antigravity takeover"
    );
    assert!(inspect_unmanaged_agent_entries(Some("ego-browser")).is_empty());

    // Simulate external tool re-creating an updated real directory, then absorb into Hub.
    unlink_skill_from_agent("ego-browser", "antigravity")?;
    fs::create_dir_all(&ag_target)?;
    fs::write(
        ag_target.join("SKILL.md"),
        "---\nname: ego-browser\ndescription: external v3\n---\nabsorbed from agent\n",
    )?;
    adopt_unmanaged_agent_entry("ego-browser", "antigravity")?;
    assert!(fs::read_to_string(canon.join("SKILL.md"))?.contains("absorbed from agent"));
    assert!(ownership::owned_deployment(&ag_target, "ego-browser").is_owned());
    Ok(())
}

#[test]
fn foreign_antigravity_symlink_skips_with_foreign_link_and_takes_over_with_mirrors() -> Result<()> {
    let sandbox = Sandbox::new();
    ensure_enabled("antigravity", true);
    let canon = create_canonical_skill("ego-browser", "hub v2");

    let external_root = sandbox.root().join("panego/ego-browser");
    fs::create_dir_all(&external_root)?;
    fs::write(
        external_root.join("SKILL.md"),
        "---\nname: ego-browser\ndescription: external link\n---\nexternal link\n",
    )?;

    let ag_skills = sandbox.home().join(".gemini/antigravity/skills");
    let ide_skills = sandbox.home().join(".gemini/antigravity-ide/skills");
    let cli_parent = sandbox.home().join(".gemini/antigravity-cli");
    let gemini_reserved = sandbox.home().join(".gemini/skills/ego-browser");
    fs::create_dir_all(&ag_skills)?;
    fs::create_dir_all(&ide_skills)?;
    fs::create_dir_all(&cli_parent)?;
    fs::create_dir_all(gemini_reserved.parent().unwrap())?;

    let ag_target = ag_skills.join("ego-browser");
    let ide_target = ide_skills.join("ego-browser");
    fs_ops::create_symlink(&external_root, &ag_target)?;
    fs_ops::create_symlink(&external_root, &ide_target)?;
    fs_ops::create_symlink(&external_root, &gemini_reserved)?;

    for enable in [true, false] {
        match toggle_skill_for_agent("ego-browser", "antigravity", enable)? {
            ToggleSkillOutcome::Skipped { code, .. } => assert_eq!(code, SKIP_FOREIGN_LINK),
            other => panic!("expected SKIP_FOREIGN_LINK, got {other:?}"),
        }
    }

    let batch = batch_link_skills_to_agent_with_report(&["ego-browser".to_string()], "antigravity")?;
    assert_eq!(batch.linked, 0);
    assert_eq!(batch.skipped.len(), 1);
    assert_eq!(batch.skipped[0].code, SKIP_FOREIGN_LINK);

    let inspected = inspect_unmanaged_agent_entries(Some("ego-browser"));
    assert_eq!(inspected.len(), 1);
    assert_eq!(inspected[0].agent_id, "antigravity");
    assert!(inspected[0].is_symlink);
    assert!(!inspected[0].can_adopt_to_hub);

    assert!(
        take_over_unmanaged_agent_entry("ego-browser", "antigravity")?.is_none(),
        "taking over a foreign symlink must not archive a directory backup"
    );
    assert!(ownership::owned_deployment(&ag_target, "ego-browser").is_owned());
    assert!(ownership::owned_deployment(&ide_target, "ego-browser").is_owned());
    assert!(
        ownership::owned_deployment(&cli_parent.join("skills/ego-browser"), "ego-browser")
            .is_owned()
    );
    assert_eq!(
        std::fs::canonicalize(&ag_target)?,
        std::fs::canonicalize(&canon)?
    );
    assert_eq!(
        ownership::owned_deployment(&gemini_reserved, "ego-browser"),
        Ownership::Foreign,
        "reserved gemini-cli symlink must not be touched by antigravity takeover"
    );
    assert!(inspect_unmanaged_agent_entries(Some("ego-browser")).is_empty());
    Ok(())
}
