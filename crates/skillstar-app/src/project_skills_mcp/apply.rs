//! Apply an approved deployment plan under the project write lock.
//!
//! This function does not ask the user. Elicitation and the CLI write the
//! approval record before they call here.

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use skillstar_skills::content::snapshot;
use skillstar_skills::projects::{
    SkillDiskKind, StrictSkillStatus, classify_project_skill, enable_project_skills_strict,
    inspect_project_skills, load_skills_list, observe_project, register_canonical_project,
    shared_path_owner, write_lock::with_project_write_lock,
};

use super::approval::load_approval;
use super::inspect::RuntimeVisibility;
use super::plan::{DeploymentPlan, PlanAction, load_plan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStatus {
    Applied,
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    Applied,
    Already,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NextAction {
    LoadSkill,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptItem {
    pub name: String,
    pub status: ItemStatus,
    pub skill_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub idempotency_key: String,
    pub plan_id: String,
    pub plan_hash: String,
    pub deployment_status: DeploymentStatus,
    pub items: Vec<ReceiptItem>,
    pub project_scope: String,
    pub runtime_visibility: RuntimeVisibility,
    pub next_action: NextAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    ApprovalRequired,
    Applied(Receipt),
    Partial(Receipt),
}

pub fn apply_project_skills(
    plan_id: &str,
    idempotency_key: &str,
    now: DateTime<Utc>,
) -> Result<ApplyOutcome> {
    if !valid_key(idempotency_key) {
        anyhow::bail!("idempotency key must match [A-Za-z0-9_-]{{1,64}}");
    }
    with_project_write_lock(|| apply_locked(plan_id, idempotency_key, now))
}

fn apply_locked(plan_id: &str, key: &str, now: DateTime<Utc>) -> Result<ApplyOutcome> {
    let plan = load_plan(plan_id, now)?;
    if let Some(existing) = load_receipt(key)? {
        if existing.plan_hash == plan.plan_hash {
            return Ok(outcome_from_receipt(existing));
        }
        anyhow::bail!("idempotency key conflicts with a different plan hash");
    }
    let Some(approval) = load_approval(plan_id)? else {
        return Ok(ApplyOutcome::ApprovalRequired);
    };
    if approval.plan_hash != plan.plan_hash {
        anyhow::bail!("approval hash does not match the plan");
    }
    recheck_plan(&plan)?;

    let observed = if plan.will_register {
        register_canonical_project(&plan.root)?;
        observe_project(&plan.root)?
    } else {
        let observed = observe_project(&plan.root)?;
        if observed.name.is_none() {
            anyhow::bail!("registered project for this plan is gone");
        }
        observed
    };
    if observed.name.is_none() {
        anyhow::bail!("project is not registered");
    }

    let skills: Vec<(String, String)> = plan
        .skills
        .iter()
        .map(|skill| (skill.name.clone(), skill.content_hash.clone()))
        .collect();
    let enabled = enable_project_skills_strict(&observed, &plan.agent_id, &skills)?;
    if !enabled.committed {
        anyhow::bail!("strict enable did not commit");
    }

    let facts = inspect_project_skills(&observed);
    let items = receipt_items(&plan, &facts);
    let all_good = items
        .iter()
        .all(|item| matches!(item.status, ItemStatus::Applied | ItemStatus::Already));
    let receipt = Receipt {
        idempotency_key: key.to_string(),
        plan_id: plan.plan_id.clone(),
        plan_hash: plan.plan_hash.clone(),
        deployment_status: if all_good {
            DeploymentStatus::Applied
        } else {
            DeploymentStatus::Partial
        },
        items,
        project_scope: observed.root.to_string_lossy().replace('\\', "/"),
        runtime_visibility: RuntimeVisibility::Unverified,
        next_action: NextAction::LoadSkill,
    };
    if all_good {
        write_receipt(&receipt)?;
        Ok(ApplyOutcome::Applied(receipt))
    } else {
        Ok(ApplyOutcome::Partial(receipt))
    }
}

fn recheck_plan(plan: &DeploymentPlan) -> Result<()> {
    let profiles = skillstar_skills::agents::list_profiles();
    let profile = profiles
        .iter()
        .find(|profile| profile.id == plan.agent_id && profile.has_project_skills())
        .context("agent has no project skills")?;
    if profile.project_skills_rel != plan.physical_rel {
        anyhow::bail!("plan path does not match the agent");
    }
    let observed = observe_project(&plan.root)?;
    let skills_list = observed
        .name
        .as_deref()
        .and_then(load_skills_list)
        .unwrap_or_default();
    let owner = shared_path_owner(&profiles, &skills_list, &plan.physical_rel, &plan.agent_id);
    if owner.owner_id.as_deref() != Some(plan.owner_id.as_str()) {
        anyhow::bail!("plan owner does not match the project");
    }
    let mode = skills_list.deploy_modes.get(&plan.physical_rel).copied();
    for skill in &plan.skills {
        let hash = snapshot(&skill.name)?.content_hash;
        if hash != skill.content_hash {
            anyhow::bail!("skill {name} content hash changed", name = skill.name);
        }
        let status =
            classify_project_skill(&observed.root, &plan.physical_rel, &skill.name, &hash, mode)?;
        let matches_plan = matches!(
            (skill.action, status),
            (PlanAction::Create, StrictSkillStatus::Create)
                | (PlanAction::Already, StrictSkillStatus::Already)
        );
        if !matches_plan {
            anyhow::bail!("skill {name} no longer matches the plan", name = skill.name);
        }
    }
    Ok(())
}

fn receipt_items(
    plan: &DeploymentPlan,
    facts: &skillstar_skills::projects::ProjectSkillFacts,
) -> Vec<ReceiptItem> {
    plan.skills
        .iter()
        .map(|skill| {
            let skill_path =
                format!("{}/{}/SKILL.md", plan.physical_rel, skill.name).replace('\\', "/");
            let live = facts.rows.iter().find_map(|row| {
                (row.project_skills_rel == plan.physical_rel)
                    .then(|| row.skills.iter().find(|item| item.name == skill.name))
                    .flatten()
            });
            let status = match (skill.action, live.map(|item| item.disk)) {
                (PlanAction::Already, Some(SkillDiskKind::Symlink | SkillDiskKind::Junction)) => {
                    ItemStatus::Already
                }
                (PlanAction::Create, Some(SkillDiskKind::Symlink | SkillDiskKind::Junction)) => {
                    ItemStatus::Applied
                }
                _ => ItemStatus::Blocked,
            };
            ReceiptItem {
                name: skill.name.clone(),
                status,
                skill_path,
            }
        })
        .collect()
}

fn outcome_from_receipt(receipt: Receipt) -> ApplyOutcome {
    if receipt.deployment_status == DeploymentStatus::Applied {
        ApplyOutcome::Applied(receipt)
    } else {
        ApplyOutcome::Partial(receipt)
    }
}

fn valid_key(key: &str) -> bool {
    (1..=64).contains(&key.len())
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

fn receipt_path(key: &str) -> PathBuf {
    skillstar_core::infra::paths::state_dir()
        .join("project-skill-receipts")
        .join(format!("{key}.json"))
}

fn load_receipt(key: &str) -> Result<Option<Receipt>> {
    let path = receipt_path(key);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    Ok(Some(serde_json::from_slice(&bytes)?))
}

fn write_receipt(receipt: &Receipt) -> Result<()> {
    let path = receipt_path(&receipt.idempotency_key);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(receipt)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod apply_project_skills_tests {
    use super::{ApplyOutcome, apply_project_skills};
    use crate::project_skills_mcp::approval::record_from_skillstar;
    use crate::project_skills_mcp::inspect::get_project_skills;
    use crate::project_skills_mcp::plan::{PlanAction, PlanDraft, PlanSkill, create_plan};
    use chrono::{Duration, TimeZone, Utc};
    use skillstar_core::infra::fs_ops::is_link;
    use skillstar_core::infra::paths as fs_paths;
    use skillstar_skills::content::snapshot;
    use skillstar_skills::projects::{
        load_skills_list, observe_project, register_project, save_skills_list,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 22, 5, 0, 0).unwrap()
    }

    struct EnvGuard {
        root: PathBuf,
        home: Option<std::ffi::OsString>,
        data: Option<std::ffi::OsString>,
        hub: Option<std::ffi::OsString>,
        #[cfg(windows)]
        userprofile: Option<std::ffi::OsString>,
        _lock: tokio::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new(label: &str) -> Self {
            let _lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("skillstar-apply-{label}-{nanos}"));
            fs::create_dir_all(root.join("home")).unwrap();
            fs::create_dir_all(root.join("data")).unwrap();
            let guard = Self {
                home: std::env::var_os("HOME"),
                data: std::env::var_os("SKILLSTAR_DATA_DIR"),
                hub: std::env::var_os("SKILLSTAR_HUB_DIR"),
                #[cfg(windows)]
                userprofile: std::env::var_os("USERPROFILE"),
                root,
                _lock,
            };
            unsafe {
                std::env::set_var("HOME", guard.root.join("home"));
                std::env::set_var("SKILLSTAR_DATA_DIR", guard.root.join("data"));
                std::env::remove_var("SKILLSTAR_HUB_DIR");
                #[cfg(windows)]
                std::env::set_var("USERPROFILE", guard.root.join("home"));
            }
            guard
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                restore("HOME", self.home.take());
                restore("SKILLSTAR_DATA_DIR", self.data.take());
                restore("SKILLSTAR_HUB_DIR", self.hub.take());
                #[cfg(windows)]
                restore("USERPROFILE", self.userprofile.take());
            }
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    unsafe fn restore(key: &str, previous: Option<std::ffi::OsString>) {
        match previous {
            Some(value) => unsafe { std::env::set_var(key, value) },
            None => unsafe { std::env::remove_var(key) },
        }
    }

    fn hub_skill(name: &str) -> String {
        let dir = fs_paths::hub_skills_dir().join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), format!("# {name}\n")).unwrap();
        snapshot(name).unwrap().content_hash
    }

    fn plan_for(
        root: &Path,
        will_register: bool,
        name: &str,
        hash: &str,
    ) -> crate::project_skills_mcp::plan::DeploymentPlan {
        create_plan(
            PlanDraft {
                root: root.to_path_buf(),
                will_register,
                agent_ids: vec!["codex".into()],
                skills: vec![PlanSkill {
                    name: name.into(),
                    content_hash: hash.into(),
                    action: PlanAction::Create,
                }],
                physical_rel: ".agents/skills".into(),
                owner_id: "codex".into(),
                affected_agents: vec!["codex".into()],
                scores: Vec::new(),
                reranker: "passthrough".into(),
            },
            now(),
        )
        .unwrap()
        .unwrap()
    }

    #[test]
    fn apply_without_approval_writes_nothing() {
        let env = EnvGuard::new("no-approval");
        let hash = hub_skill("demo");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let canonical = fs::canonicalize(&project).unwrap();
        let plan = plan_for(&canonical, true, "demo", &hash);
        let outcome = apply_project_skills(&plan.plan_id, "key-1", now()).unwrap();
        assert_eq!(outcome, ApplyOutcome::ApprovalRequired);
        assert!(!project.join(".agents/skills/demo").exists());
        assert!(!fs_paths::projects_manifest_path().exists());
        assert!(
            !fs_paths::state_dir()
                .join("project-skill-receipts/key-1.json")
                .exists()
        );
    }

    #[test]
    fn apply_rejects_expired_and_hash_mismatch() {
        let env = EnvGuard::new("expired");
        let hash = hub_skill("demo");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let canonical = fs::canonicalize(&project).unwrap();
        let plan = plan_for(&canonical, true, "demo", &hash);
        record_from_skillstar(&plan.plan_id, &plan.plan_hash).unwrap();
        let expired = apply_project_skills(&plan.plan_id, "key-exp", now() + Duration::minutes(16));
        assert!(expired.unwrap_err().to_string().contains("expired"));
        assert!(!project.join(".agents").exists());
        assert!(
            !fs_paths::state_dir()
                .join("project-skill-receipts/key-exp.json")
                .exists()
        );

        let path = fs_paths::state_dir()
            .join("project-skill-plans")
            .join(format!("{}.json", plan.plan_id));
        let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        raw["skills"][0]["content_hash"] = serde_json::json!("tampered");
        fs::write(&path, serde_json::to_vec_pretty(&raw).unwrap()).unwrap();
        let mismatch = apply_project_skills(&plan.plan_id, "key-bad", now());
        assert!(mismatch.unwrap_err().to_string().contains("hash"));
        assert!(!fs_paths::projects_manifest_path().exists());
        assert!(
            !fs_paths::state_dir()
                .join("project-skill-receipts/key-bad.json")
                .exists()
        );
    }

    #[test]
    fn apply_replays_the_same_receipt_without_relinking() {
        let env = EnvGuard::new("replay");
        let hash = hub_skill("demo");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let canonical = fs::canonicalize(&project).unwrap();
        let plan = plan_for(&canonical, true, "demo", &hash);
        record_from_skillstar(&plan.plan_id, &plan.plan_hash).unwrap();
        let first = apply_project_skills(&plan.plan_id, "replay-key", now()).unwrap();
        let ApplyOutcome::Applied(receipt) = first else {
            panic!("expected applied");
        };
        let link = project.join(".agents/skills/demo");
        let modified = fs::symlink_metadata(&link).unwrap().modified().unwrap();
        let second = apply_project_skills(&plan.plan_id, "replay-key", now()).unwrap();
        let ApplyOutcome::Applied(again) = second else {
            panic!("expected replay");
        };
        assert_eq!(again, receipt);
        assert_eq!(
            fs::symlink_metadata(&link).unwrap().modified().unwrap(),
            modified
        );
        assert!(is_link(&link));
    }

    #[test]
    fn apply_same_key_different_hash_conflicts() {
        let env = EnvGuard::new("conflict-key");
        let hash_a = hub_skill("alpha");
        let hash_b = hub_skill("beta");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let canonical = fs::canonicalize(&project).unwrap();
        let first = plan_for(&canonical, true, "alpha", &hash_a);
        let second = plan_for(&canonical, false, "beta", &hash_b);
        record_from_skillstar(&first.plan_id, &first.plan_hash).unwrap();
        record_from_skillstar(&second.plan_id, &second.plan_hash).unwrap();
        apply_project_skills(&first.plan_id, "same-key", now()).unwrap();
        let err = apply_project_skills(&second.plan_id, "same-key", now()).unwrap_err();
        assert!(err.to_string().contains("conflicts"), "{err}");
        assert!(!project.join(".agents/skills/beta").exists());
    }

    fn manifest_paths() -> Vec<String> {
        let path = fs_paths::projects_manifest_path();
        if !path.exists() {
            return Vec::new();
        }
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value
            .get("projects")
            .and_then(|projects| projects.as_array())
            .map(|projects| {
                projects
                    .iter()
                    .filter_map(|entry| entry.get("path").and_then(|path| path.as_str()))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn link_dir(target: &Path, link: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        #[cfg(windows)]
        junction::create(target, link).unwrap();
    }

    #[test]
    fn apply_registers_only_when_the_plan_says_so() {
        let env = EnvGuard::new("register");
        let hash = hub_skill("demo");

        let missing = env.root.join("missing");
        fs::create_dir_all(&missing).unwrap();
        let missing_root = fs::canonicalize(&missing).unwrap();
        let absent = plan_for(&missing_root, false, "demo", &hash);
        record_from_skillstar(&absent.plan_id, &absent.plan_hash).unwrap();
        let err = apply_project_skills(&absent.plan_id, "gone-key", now()).unwrap_err();
        assert!(err.to_string().contains("gone"), "{err}");
        assert!(!fs_paths::projects_manifest_path().exists());
        assert!(!missing.join(".agents").exists());

        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let canonical = fs::canonicalize(&project).unwrap();
        let plan = plan_for(&canonical, true, "demo", &hash);
        record_from_skillstar(&plan.plan_id, &plan.plan_hash).unwrap();
        apply_project_skills(&plan.plan_id, "reg-key", now()).unwrap();
        assert_eq!(
            manifest_paths(),
            vec![canonical.to_str().unwrap().to_string()]
        );

        let aliased = env.root.join("aliased");
        fs::create_dir_all(&aliased).unwrap();
        let alias = env.root.join("alias-link");
        link_dir(&aliased, &alias);
        let alias_root = fs::canonicalize(&aliased).unwrap();
        let entry = register_project(alias.to_str().unwrap()).unwrap();
        assert_ne!(entry.path, alias_root.to_str().unwrap());
        let before = fs::read(fs_paths::projects_manifest_path()).unwrap();
        let reuse = plan_for(&alias_root, true, "demo", &hash);
        record_from_skillstar(&reuse.plan_id, &reuse.plan_hash).unwrap();
        let outcome = apply_project_skills(&reuse.plan_id, "reuse-key", now()).unwrap();
        assert!(matches!(outcome, ApplyOutcome::Applied(_)));
        assert_eq!(
            fs::read(fs_paths::projects_manifest_path()).unwrap(),
            before
        );
        assert!(is_link(&aliased.join(".agents/skills/demo")));
    }

    #[test]
    fn apply_does_not_call_save_and_sync_or_add_skills() {
        let env = EnvGuard::new("no-loose");
        let hash = hub_skill("demo");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let canonical = fs::canonicalize(&project).unwrap();
        register_project(canonical.to_str().unwrap()).unwrap();
        let observed = observe_project(canonical.to_str().unwrap()).unwrap();
        let mut list = load_skills_list(observed.name.as_deref().unwrap()).unwrap_or_default();
        list.agents
            .insert("codex".into(), vec!["kept-manually".into()]);
        save_skills_list(observed.name.as_deref().unwrap(), &list).unwrap();
        let plan = plan_for(&canonical, false, "demo", &hash);
        record_from_skillstar(&plan.plan_id, &plan.plan_hash).unwrap();
        apply_project_skills(&plan.plan_id, "strict-key", now()).unwrap();
        let list = load_skills_list(observed.name.as_deref().unwrap()).unwrap();
        let names = list.agents.get("codex").unwrap();
        assert!(names.iter().any(|name| name == "kept-manually"));
        assert!(names.iter().any(|name| name == "demo"));
        assert!(is_link(&project.join(".agents/skills/demo")));
    }

    #[test]
    fn apply_recheck_matches_get_facts() {
        let env = EnvGuard::new("recheck");
        let hash = hub_skill("demo");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let canonical = fs::canonicalize(&project).unwrap();
        let plan = plan_for(&canonical, true, "demo", &hash);
        record_from_skillstar(&plan.plan_id, &plan.plan_hash).unwrap();
        let ApplyOutcome::Applied(receipt) =
            apply_project_skills(&plan.plan_id, "facts-key", now()).unwrap()
        else {
            panic!("expected applied");
        };
        let view = get_project_skills(canonical.to_str().unwrap()).unwrap();
        assert!(
            view.load_hints
                .iter()
                .any(|hint| hint.skill_path == receipt.items[0].skill_path)
        );
        assert_eq!(view.runtime_visibility, receipt.runtime_visibility);
    }
}
