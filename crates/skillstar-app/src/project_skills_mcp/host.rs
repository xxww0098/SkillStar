//! Desktop approval. Reads a plan and records a SkillStar approval.
//! It does not deploy.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use ts_rs::TS;

use super::approval::record_from_skillstar;
use super::plan::{self, DeploymentPlan, PlanAction};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "ProjectSkillPlanChange.ts")]
pub struct ProjectSkillPlanChange {
    pub name: String,
    pub action: String,
    pub skill_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "ProjectSkillPlanDiff.ts")]
pub struct ProjectSkillPlanDiff {
    pub plan_id: String,
    pub plan_hash: String,
    pub root: String,
    pub will_register: bool,
    pub owner_id: String,
    pub affected_agents: Vec<String>,
    pub changes: Vec<ProjectSkillPlanChange>,
}

pub fn pending_plan_diffs(
    project_path: &str,
    now: DateTime<Utc>,
) -> Result<Vec<ProjectSkillPlanDiff>> {
    Ok(plan::pending_plans_for_root(project_path, now)?
        .into_iter()
        .map(|plan| plan_diff(&plan))
        .collect())
}

pub fn approve_plan_from_skillstar(plan_id: &str, now: DateTime<Utc>) -> Result<()> {
    let plan = plan::load_plan(plan_id, now)?;
    record_from_skillstar(&plan.plan_id, &plan.plan_hash)?;
    Ok(())
}

fn plan_diff(plan: &DeploymentPlan) -> ProjectSkillPlanDiff {
    ProjectSkillPlanDiff {
        changes: plan
            .skills
            .iter()
            .map(|skill| {
                let action = match skill.action {
                    PlanAction::Create => "create",
                    PlanAction::Already => "already",
                };
                ProjectSkillPlanChange {
                    skill_path: format!("{}/{}/SKILL.md", plan.physical_rel, skill.name)
                        .replace('\\', "/"),
                    name: skill.name.clone(),
                    action: action.to_string(),
                }
            })
            .collect(),
        plan_id: plan.plan_id.clone(),
        plan_hash: plan.plan_hash.clone(),
        root: plan.root.clone(),
        will_register: plan.will_register,
        owner_id: plan.owner_id.clone(),
        affected_agents: plan.affected_agents.clone(),
    }
}

#[cfg(test)]
mod skillstar_approve_command_tests {
    use super::approve_plan_from_skillstar;
    use crate::project_skills_mcp::approval::record_from_elicitation;
    use crate::project_skills_mcp::plan::{PlanAction, PlanDraft, PlanSkill, create_plan};
    use chrono::{TimeZone, Utc};
    use skillstar_core::infra::paths as fs_paths;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 22, 9, 0, 0).unwrap()
    }

    struct EnvGuard {
        root: PathBuf,
        home: Option<std::ffi::OsString>,
        data: Option<std::ffi::OsString>,
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
            let root = std::env::temp_dir().join(format!("skillstar-gui-approve-{label}-{nanos}"));
            fs::create_dir_all(root.join("home")).unwrap();
            fs::create_dir_all(root.join("data")).unwrap();
            let guard = Self {
                home: std::env::var_os("HOME"),
                data: std::env::var_os("SKILLSTAR_DATA_DIR"),
                #[cfg(windows)]
                userprofile: std::env::var_os("USERPROFILE"),
                root,
                _lock,
            };
            unsafe {
                std::env::set_var("HOME", guard.root.join("home"));
                std::env::set_var("SKILLSTAR_DATA_DIR", guard.root.join("data"));
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

    fn plan(root: &std::path::Path) -> crate::project_skills_mcp::plan::DeploymentPlan {
        create_plan(
            PlanDraft {
                root: root.to_path_buf(),
                will_register: true,
                agent_ids: vec!["codex".into()],
                skills: vec![PlanSkill {
                    name: "demo".into(),
                    content_hash: "hash-demo".into(),
                    action: PlanAction::Create,
                }],
                physical_rel: ".agents/skills".into(),
                owner_id: "codex".into(),
                affected_agents: vec!["codex".into(), "deepseek".into()],
                scores: Vec::new(),
                reranker: "passthrough".into(),
            },
            now(),
        )
        .unwrap()
        .unwrap()
    }

    #[test]
    fn skillstar_approve_command_rejects_a_plan_already_approved_by_elicitation() {
        let env = EnvGuard::new("elicited");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let stored = plan(&project);
        record_from_elicitation(&stored.plan_id, &stored.plan_hash).unwrap();
        let err = approve_plan_from_skillstar(&stored.plan_id, now()).unwrap_err();
        assert!(err.to_string().contains("another source"), "{err}");
        assert!(!project.join(".agents").exists());
    }

    #[test]
    fn skillstar_approve_command_does_not_deploy() {
        let env = EnvGuard::new("no-deploy");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let stored = plan(&project);
        approve_plan_from_skillstar(&stored.plan_id, now()).unwrap();
        assert!(!project.join(".agents").exists());
        assert!(!fs_paths::projects_manifest_path().exists());
        let record: serde_json::Value = serde_json::from_slice(
            &fs::read(
                fs_paths::state_dir()
                    .join("project-skill-approvals")
                    .join(format!("{}.json", stored.plan_id)),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(record["source"], "skillstar");
    }
}
